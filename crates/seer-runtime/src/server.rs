use seer_core::proto::{ClientMsg, ServerMsg, codec};
use std::fs;
use std::io::{self, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::UserSession;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const DETACHED_POLL_INTERVAL: Duration = Duration::from_millis(100);
// A burst of one poll tick can hold many pane messages; the queue and the deadline must be larger than one tick.
const OUTPUT_QUEUE_CAPACITY: usize = 64;
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
/// A size-lease holder that stays silent for this long is treated as gone.
const SIZE_LEASE_TIMEOUT: Duration = Duration::from_secs(30);

pub fn bind(path: &Path) -> io::Result<UnixListener> {
    remove_stale_socket(path)?;
    UnixListener::bind(path)
}

pub fn serve(listener: UnixListener, session: UserSession) -> io::Result<()> {
    let shared = Arc::new(SharedSession::new(session));
    start_poll_driver(Arc::clone(&shared))?;

    for connection_id in 0_u64.. {
        let (stream, _) = listener.accept()?;
        let connection = Arc::clone(&shared);
        thread::Builder::new()
            .name(format!("runtime-connection-{connection_id}"))
            .spawn(move || {
                if let Err(error) = handle_connection(stream, &connection, connection_id) {
                    eprintln!("runtime connection error: {error}");
                }
            })?;
    }
    Ok(())
}

fn remove_stale_socket(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    match UnixStream::connect(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "runtime socket is already in use",
        )),
        Err(_) => fs::remove_file(path),
    }
}

fn handle_connection(
    mut stream: UnixStream,
    shared: &SharedSession,
    connection_id: u64,
) -> io::Result<()> {
    shared.add_connection(connection_id, stream.try_clone()?)?;
    let result = connection_loop(&mut stream, shared, connection_id);
    shared.remove_connection(connection_id)?;
    let _ = stream.shutdown(std::net::Shutdown::Both);
    result
}

fn connection_loop(
    stream: &mut UnixStream,
    shared: &SharedSession,
    connection_id: u64,
) -> io::Result<()> {
    loop {
        let Ok(message) = codec::decode(stream) else {
            return Ok(());
        };
        if handle_message(shared, connection_id, message)? {
            return Ok(());
        }
    }
}

fn handle_message(
    shared: &SharedSession,
    connection_id: u64,
    message: ClientMsg,
) -> io::Result<bool> {
    let read_only = shared.refresh_read_only(connection_id)?;
    match message {
        ClientMsg::Detach | ClientMsg::StopPeek => Ok(true),
        ClientMsg::Peek { workspace, tab, .. } => {
            match shared.send_snapshot(connection_id, &workspace, &tab) {
                Ok(()) => shared.enter_peek(connection_id)?,
                Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
                    shared.send_refused(connection_id, error.to_string())?;
                }
                Err(error) => return Err(error),
            }
            Ok(false)
        }
        ClientMsg::Resize {
            workspace,
            tab,
            cols,
            rows,
        } => shared.client_resize(connection_id, &workspace, &tab, cols, rows),
        message if read_only && is_mutating(&message) => {
            eprintln!("runtime dropped read-only message: {message:?}");
            Ok(false)
        }
        message => {
            apply_or_refuse(shared, connection_id, message)?;
            Ok(false)
        }
    }
}

fn apply_or_refuse(
    shared: &SharedSession,
    connection_id: u64,
    message: ClientMsg,
) -> io::Result<()> {
    match shared.apply_and_broadcast(message) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            shared.send_refused(connection_id, error.to_string())
        }
        Err(error) => Err(error),
    }
}

fn is_mutating(message: &ClientMsg) -> bool {
    matches!(
        message,
        ClientMsg::TerminalInput { .. }
            | ClientMsg::CreateTab { .. }
            | ClientMsg::SplitPane { .. }
            | ClientMsg::ClosePane { .. }
            | ClientMsg::FocusPane { .. }
            | ClientMsg::Resize { .. }
    )
}

fn start_poll_driver(shared: Arc<SharedSession>) -> io::Result<()> {
    thread::Builder::new()
        .name("runtime-poll".into())
        .spawn(move || poll_driver(&shared))?;
    Ok(())
}

fn poll_driver(shared: &SharedSession) {
    loop {
        let has_connections = match shared.poll_and_broadcast() {
            Ok((_, has_connections)) => has_connections,
            Err(error) => {
                eprintln!("runtime poll error: {error}");
                return;
            }
        };
        let interval = if has_connections {
            POLL_INTERVAL
        } else {
            DETACHED_POLL_INTERVAL
        };
        thread::sleep(interval);
    }
}

struct SharedSession {
    session: Mutex<UserSession>,
    connections: Mutex<Vec<Connection>>,
}

impl SharedSession {
    fn new(session: UserSession) -> Self {
        Self {
            session: Mutex::new(session),
            connections: Mutex::new(Vec::new()),
        }
    }

    fn add_connection(&self, id: u64, stream: UnixStream) -> io::Result<()> {
        let messages = {
            let mut session = lock(&self.session)?;
            session.ensure_first_shell()?;
            session.snapshot()
        };
        let mut connection = Connection::new(id, stream)?;
        if !connection.send_messages(&messages)? {
            return Err(connection_closed());
        }
        let mut connections = lock(&self.connections)?;
        let owner_stale = connections
            .iter()
            .any(|c| c.size_owner && c.last_active + SIZE_LEASE_TIMEOUT <= Instant::now());
        if owner_stale {
            for owner in connections.iter_mut() {
                owner.size_owner = false;
            }
            connection.size_owner = true;
        } else {
            connection.size_owner = !connections.iter().any(|c| c.size_owner);
        }
        connections.push(connection);
        Ok(())
    }

    fn remove_connection(&self, id: u64) -> io::Result<()> {
        let adopt = {
            let mut connections = lock(&self.connections)?;
            let removed_owner = connections
                .iter()
                .any(|connection| connection.id == id && connection.size_owner);
            connections.retain(|connection| connection.id != id);
            if removed_owner {
                grant_next_owner(&mut *connections)
            } else {
                None
            }
        };
        if let Some(viewport) = adopt {
            self.adopt_viewport(viewport)?;
        }
        Ok(())
    }

    fn refresh_read_only(&self, id: u64) -> io::Result<bool> {
        let mut connections = lock(&self.connections)?;
        let Some(connection) = connections.iter_mut().find(|c| c.id == id) else {
            return Ok(false);
        };
        connection.last_active = Instant::now();
        Ok(connection.read_only)
    }

    fn client_resize(
        &self,
        id: u64,
        workspace: &str,
        tab: &str,
        cols: u16,
        rows: u16,
    ) -> io::Result<bool> {
        {
            let mut connections = lock(&self.connections)?;
            let Some(position) = connections.iter().position(|c| c.id == id) else {
                return Ok(false);
            };
            if connections[position].read_only {
                eprintln!("runtime dropped read-only resize: {cols}x{rows}");
                return Ok(false);
            }
            connections[position].viewport = Some(ReportedViewport {
                workspace: workspace.to_owned(),
                tab: tab.to_owned(),
                cols,
                rows,
            });
            let lease_vacant = !connections.iter().any(|c| c.size_owner);
            let owner_stale = connections
                .iter()
                .any(|c| c.size_owner && c.last_active + SIZE_LEASE_TIMEOUT <= Instant::now());
            if !connections[position].size_owner && !lease_vacant && !owner_stale {
                eprintln!("runtime denied resize for connection {id}: {cols}x{rows}");
                return Ok(false);
            }
            for connection in connections.iter_mut() {
                connection.size_owner = connection.id == id;
            }
        }
        let message = ClientMsg::Resize {
            workspace: workspace.to_owned(),
            tab: tab.to_owned(),
            cols,
            rows,
        };
        match lock(&self.session)?.apply(message) {
            Ok(messages) => {
                self.broadcast(&messages)?;
                Ok(false)
            }
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
                self.send_refused(id, error.to_string())?;
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    fn enter_peek(&self, id: u64) -> io::Result<()> {
        let adopt = {
            let mut connections = lock(&self.connections)?;
            let Some(position) = connections.iter().position(|c| c.id == id) else {
                return Ok(());
            };
            connections[position].read_only = true;
            if !connections[position].size_owner {
                None
            } else {
                connections[position].size_owner = false;
                grant_next_owner(&mut *connections)
            }
        };
        if let Some(viewport) = adopt {
            self.adopt_viewport(viewport)?;
        }
        Ok(())
    }

    fn adopt_viewport(&self, viewport: ReportedViewport) -> io::Result<()> {
        let messages = lock(&self.session)?.apply(ClientMsg::Resize {
            workspace: viewport.workspace,
            tab: viewport.tab,
            cols: viewport.cols,
            rows: viewport.rows,
        })?;
        self.broadcast(&messages)
    }

    fn send_snapshot(&self, id: u64, workspace: &str, tab: &str) -> io::Result<()> {
        let session = lock(&self.session)?;
        let tree = session.selected_tree(workspace, tab)?;
        let messages = session.snapshot_for(tree);
        drop(session);
        let mut connections = lock(&self.connections)?;
        let position = connections
            .iter()
            .position(|connection| connection.id == id)
            .ok_or_else(connection_closed)?;
        if !connections[position].send_messages(&messages)? {
            connections.remove(position);
            return Err(connection_closed());
        }
        Ok(())
    }

    fn send_refused(&self, id: u64, reason: String) -> io::Result<()> {
        self.send_to(id, &ServerMsg::Refused { reason })
    }

    fn send_to(&self, id: u64, message: &ServerMsg) -> io::Result<()> {
        let output = encode_message(message)?;
        let mut connections = lock(&self.connections)?;
        let position = connections
            .iter()
            .position(|connection| connection.id == id)
            .ok_or_else(connection_closed)?;
        if !connections[position].send(output) {
            connections.remove(position);
            return Err(connection_closed());
        }
        Ok(())
    }

    fn apply_and_broadcast(&self, message: ClientMsg) -> io::Result<()> {
        let messages = lock(&self.session)?.apply(message)?;
        self.broadcast(&messages)
    }

    fn poll_and_broadcast(&self) -> io::Result<(Vec<ServerMsg>, bool)> {
        let messages = lock(&self.session)?.poll();
        let has_connections = !lock(&self.connections)?.is_empty();
        if has_connections {
            self.broadcast(&messages)?;
        }
        Ok((messages, has_connections))
    }

    fn broadcast(&self, messages: &[ServerMsg]) -> io::Result<()> {
        for message in messages {
            let output = encode_message(message)?;
            let mut connections = lock(&self.connections)?;
            connections.retain(|connection| connection.send(Arc::clone(&output)));
        }
        Ok(())
    }

    #[cfg(test)]
    fn connection_count(&self) -> usize {
        self.connections
            .lock()
            .map(|connections| connections.len())
            .unwrap_or(0)
    }
}

/// The workspace, tab, and size of the last resize an attachment sent.
#[derive(Clone)]
struct ReportedViewport {
    workspace: String,
    tab: String,
    cols: u16,
    rows: u16,
}

struct Connection {
    id: u64,
    output: SyncSender<Arc<[u8]>>,
    stream: UnixStream,
    /// The last resize the attachment reported, with its selected tab.
    viewport: Option<ReportedViewport>,
    /// The attachment is a read-only peek viewer.
    read_only: bool,
    /// The attachment holds the size lease for the shared session.
    size_owner: bool,
    /// When the attachment last sent a message to the runtime.
    last_active: Instant,
}

impl Connection {
    fn new(id: u64, stream: UnixStream) -> io::Result<Self> {
        let writer = stream.try_clone()?;
        let (output, queued) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
        spawn_writer(id, writer, queued)?;
        Ok(Self {
            id,
            output,
            stream,
            viewport: None,
            read_only: false,
            size_owner: false,
            last_active: Instant::now(),
        })
    }

    fn send(&self, output: Arc<[u8]>) -> bool {
        self.output.try_send(output).is_ok()
    }

    fn send_messages(&self, messages: &[ServerMsg]) -> io::Result<bool> {
        for message in messages {
            if !self.send(encode_message(message)?) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

fn spawn_writer(id: u64, mut stream: UnixStream, output: Receiver<Arc<[u8]>>) -> io::Result<()> {
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    thread::Builder::new()
        .name(format!("runtime-writer-{id}"))
        .spawn(move || write_output(id, &mut stream, &output))?;
    Ok(())
}

fn write_output(id: u64, stream: &mut UnixStream, output: &Receiver<Arc<[u8]>>) {
    while let Ok(bytes) = output.recv() {
        if let Err(error) = stream.write_all(&bytes) {
            eprintln!("runtime evicted slow connection {id}: {error}");
            break;
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn grant_next_owner(connections: &mut [Connection]) -> Option<ReportedViewport> {
    let position = connections
        .iter()
        .position(|connection| !connection.read_only)?;
    connections[position].size_owner = true;
    connections[position].viewport.clone()
}

fn encode_message(message: &ServerMsg) -> io::Result<Arc<[u8]>> {
    let mut output = Vec::new();
    codec::encode(&mut output, message)?;
    Ok(Arc::from(output))
}

fn lock<T>(mutex: &Mutex<T>) -> io::Result<MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| lock_poisoned())
}

fn lock_poisoned() -> io::Error {
    io::Error::other("runtime shared state lock is poisoned")
}

fn connection_closed() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "runtime connection is closed")
}

#[cfg(test)]
mod tests;
