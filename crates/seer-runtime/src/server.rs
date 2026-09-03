use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::PaneSize;
use std::fs;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::UserSession;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
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
    shared.touch_connection(connection_id)?;
    let read_only = shared.is_read_only(connection_id)?;
    match message {
        ClientMsg::Detach | ClientMsg::StopPeek => Ok(true),
        ClientMsg::Peek { .. } => {
            shared.enter_peek(connection_id)?;
            shared.send_tree(connection_id)?;
            Ok(false)
        }
        ClientMsg::Resize { cols, rows } => {
            shared.client_resize(connection_id, cols, rows)
        }
        message if read_only && is_mutating(&message) => {
            eprintln!("runtime dropped read-only message: {message:?}");
            Ok(false)
        }
        message => {
            shared.apply_and_broadcast(message)?;
            Ok(false)
        }
    }
}

fn is_mutating(message: &ClientMsg) -> bool {
    matches!(
        message,
        ClientMsg::Input { .. }
            | ClientMsg::CreateTab
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
        let connections = shared.connections.lock().ok().and_then(|connections| {
            shared
                .connection_opened
                .wait_while(connections, |connections| connections.is_empty())
                .ok()
        });
        let Some(connections) = connections else {
            eprintln!("runtime poll error: {}", lock_poisoned());
            return;
        };
        drop(connections);
        thread::sleep(POLL_INTERVAL);
        if let Err(error) = shared.poll_and_broadcast() {
            eprintln!("runtime poll error: {error}");
            return;
        }
    }
}

struct SharedSession {
    session: Mutex<UserSession>,
    connections: Mutex<Vec<Connection>>,
    connection_opened: Condvar,
}

impl SharedSession {
    fn new(session: UserSession) -> Self {
        Self {
            session: Mutex::new(session),
            connections: Mutex::new(Vec::new()),
            connection_opened: Condvar::new(),
        }
    }

    fn add_connection(&self, id: u64, mut stream: UnixStream) -> io::Result<()> {
        let mut session = lock(&self.session)?;
        session.ensure_first_shell()?;
        let mut connections = lock(&self.connections)?;
        write_messages(
            &mut stream,
            &[ServerMsg::Tree {
                tree: session.tree.clone(),
            }],
        )?;
        let now = Instant::now();
        let owner_stale = connections.iter().any(|connection| {
            connection.size_owner
                && connection.last_active + SIZE_LEASE_TIMEOUT <= now
        });
        let size_owner =
            !connections.iter().any(|connection| connection.size_owner) || owner_stale;
        if owner_stale {
            eprintln!("runtime size lease expired; connection {id} took ownership");
        }
        connections.push(Connection {
            id,
            stream,
            viewport: None,
            read_only: false,
            size_owner,
            last_active: now,
        });
        self.connection_opened.notify_one();
        Ok(())
    }

    fn remove_connection(&self, id: u64) -> io::Result<()> {
        let mut session = lock(&self.session)?;
        let mut connections = lock(&self.connections)?;
        let removed_owner = connections
            .iter()
            .any(|connection| connection.id == id && connection.size_owner);
        connections.retain(|connection| connection.id != id);
        if removed_owner {
            grant_next_owner(&mut session, &mut connections)?;
        }
        Ok(())
    }

    fn touch_connection(&self, id: u64) -> io::Result<()> {
        let mut connections = lock(&self.connections)?;
        if let Some(connection) = connections.iter_mut().find(|connection| connection.id == id) {
            connection.last_active = Instant::now();
        }
        Ok(())
    }

    fn is_read_only(&self, id: u64) -> io::Result<bool> {
        let connections = lock(&self.connections)?;
        Ok(connections
            .iter()
            .find(|connection| connection.id == id)
            .is_some_and(|connection| connection.read_only))
    }

    fn client_resize(&self, id: u64, cols: u16, rows: u16) -> io::Result<bool> {
        let mut session = lock(&self.session)?;
        let mut connections = lock(&self.connections)?;
        let Some(position) = connections
            .iter()
            .position(|connection| connection.id == id)
        else {
            return Ok(false);
        };
        if connections[position].read_only {
            eprintln!("runtime dropped read-only resize: {cols}x{rows}");
            return Ok(false);
        }
        connections[position].viewport = Some(PaneSize { cols, rows });
        let is_owner = connections[position].size_owner;
        let owner_missing = !connections.iter().any(|connection| connection.size_owner);
        let owner_stale = connections.iter().any(|connection| {
            connection.size_owner
                && connection.last_active + SIZE_LEASE_TIMEOUT <= Instant::now()
        });
        if !is_owner && !owner_missing && !owner_stale {
            eprintln!("runtime denied resize for connection {id}: {cols}x{rows}");
            return Ok(false);
        }
        if owner_stale && !is_owner {
            eprintln!("runtime size lease expired; connection {id} took ownership");
        }
        for connection in connections.iter_mut() {
            connection.size_owner = connection.id == id;
        }
        let messages = session.apply(ClientMsg::Resize { cols, rows })?;
        broadcast(&mut connections, &messages);
        Ok(false)
    }

    fn enter_peek(&self, id: u64) -> io::Result<()> {
        let mut session = lock(&self.session)?;
        let mut connections = lock(&self.connections)?;
        let Some(position) = connections
            .iter()
            .position(|connection| connection.id == id)
        else {
            return Ok(());
        };
        connections[position].read_only = true;
        if connections[position].size_owner {
            connections[position].size_owner = false;
            grant_next_owner(&mut session, &mut connections)?;
        }
        Ok(())
    }

    fn send_tree(&self, id: u64) -> io::Result<()> {
        let session = lock(&self.session)?;
        let message = ServerMsg::Tree {
            tree: session.tree.clone(),
        };
        let mut connections = lock(&self.connections)?;
        let position = connections
            .iter()
            .position(|connection| connection.id == id)
            .ok_or_else(connection_closed)?;
        let result = write_messages(&mut connections[position].stream, &[message]);
        if result.is_err() {
            connections.remove(position);
        }
        result
    }

    fn apply_and_broadcast(&self, message: ClientMsg) -> io::Result<()> {
        let mut session = lock(&self.session)?;
        let messages = session.apply(message)?;
        self.broadcast(&messages)
    }

    fn poll_and_broadcast(&self) -> io::Result<()> {
        let mut session = lock(&self.session)?;
        let mut connections = lock(&self.connections)?;
        if connections.is_empty() {
            return Ok(());
        }
        let messages = session.poll();
        broadcast(&mut connections, &messages);
        Ok(())
    }

    fn broadcast(&self, messages: &[ServerMsg]) -> io::Result<()> {
        let mut connections = lock(&self.connections)?;
        broadcast(&mut connections, messages);
        Ok(())
    }
}

struct Connection {
    id: u64,
    stream: UnixStream,
    /// The terminal size the attachment last reported.
    viewport: Option<PaneSize>,
    /// The attachment is a read-only peek viewer.
    read_only: bool,
    /// The attachment holds the size lease for the shared session.
    size_owner: bool,
    /// When the attachment last sent a message to the runtime.
    last_active: Instant,
}

fn grant_next_owner(
    session: &mut MutexGuard<'_, UserSession>,
    connections: &mut Vec<Connection>,
) -> io::Result<()> {
    let Some(position) = connections
        .iter()
        .position(|connection| !connection.read_only)
    else {
        return Ok(());
    };
    connections[position].size_owner = true;
    if let Some(size) = connections[position].viewport {
        let _messages = session.apply(ClientMsg::Resize {
            cols: size.cols,
            rows: size.rows,
        })?;
    }
    Ok(())
}

fn broadcast(connections: &mut Vec<Connection>, messages: &[ServerMsg]) {
    connections.retain_mut(|connection| write_messages(&mut connection.stream, messages).is_ok());
}

fn write_messages(stream: &mut UnixStream, messages: &[ServerMsg]) -> io::Result<()> {
    for message in messages {
        codec::encode(stream, message)?;
    }
    Ok(())
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
