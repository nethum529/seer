use seer_core::proto::{ClientMsg, ServerMsg, codec};
use std::fs;
use std::io::{self, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use crate::UserSession;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const DETACHED_POLL_INTERVAL: Duration = Duration::from_millis(100);
// A burst of one poll tick can hold many pane messages; the queue and the deadline must be larger than one tick.
const OUTPUT_QUEUE_CAPACITY: usize = 64;
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

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
    let mut read_only = false;
    loop {
        let Ok(message) = codec::decode(stream) else {
            return Ok(());
        };
        if handle_message(shared, connection_id, &mut read_only, message)? {
            return Ok(());
        }
    }
}

fn handle_message(
    shared: &SharedSession,
    connection_id: u64,
    read_only: &mut bool,
    message: ClientMsg,
) -> io::Result<bool> {
    match message {
        ClientMsg::Detach | ClientMsg::StopPeek => Ok(true),
        ClientMsg::Peek { workspace, tab, .. } => {
            match shared.send_snapshot(connection_id, &workspace, &tab) {
                Ok(()) => *read_only = true,
                Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
                    shared.send_refused(connection_id, error.to_string())?;
                }
                Err(error) => return Err(error),
            }
            Ok(false)
        }
        message if *read_only && is_mutating(&message) => {
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
        ClientMsg::Input { .. }
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
        let connection = Connection::new(id, stream)?;
        if !connection.send_messages(&messages)? {
            return Err(connection_closed());
        }
        let mut connections = lock(&self.connections)?;
        connections.push(connection);
        Ok(())
    }

    fn remove_connection(&self, id: u64) -> io::Result<()> {
        lock(&self.connections)?.retain(|connection| connection.id != id);
        Ok(())
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

struct Connection {
    id: u64,
    output: SyncSender<Arc<[u8]>>,
    stream: UnixStream,
}

impl Connection {
    fn new(id: u64, stream: UnixStream) -> io::Result<Self> {
        let writer = stream.try_clone()?;
        let (output, queued) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
        spawn_writer(id, writer, queued)?;
        Ok(Self { id, output, stream })
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
mod tests {
    use super::{ClientMsg, ServerMsg, SharedSession, is_mutating, lock};
    use crate::UserSession;
    use std::os::unix::net::UnixStream;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn identifies_only_mutating_messages() {
        let mutating = [
            ClientMsg::Input {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: "p1".into(),
                bytes: Vec::new(),
            },
            ClientMsg::CreateTab {
                workspace: "w1".into(),
            },
            ClientMsg::SplitPane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                direction: seer_core::SplitDirection::Right,
            },
            ClientMsg::ClosePane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: "p1".into(),
            },
            ClientMsg::FocusPane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: "p1".into(),
            },
            ClientMsg::Resize {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                cols: 80,
                rows: 24,
            },
        ];
        let deferred = [
            ClientMsg::Hello {
                user_id: "alice".into(),
                credential: "token".into(),
                version: "0.1.0".into(),
            },
            ClientMsg::Join {
                seat_token: "seat".into(),
                name: "alice".into(),
            },
            ClientMsg::Invite { hours: None },
            ClientMsg::ListPeople,
            ClientMsg::DetachClient {
                client_id: "client-1".into(),
            },
            ClientMsg::Peek {
                user: "alice".into(),
                workspace: "w1".into(),
                tab: "w1:t1".into(),
            },
            ClientMsg::StopPeek,
            ClientMsg::Detach,
        ];

        assert!(mutating.iter().all(is_mutating));
        assert!(deferred.iter().all(|message| !is_mutating(message)));
    }

    #[test]
    fn removes_only_the_requested_connection() {
        let mut session = UserSession::new("alice", "sh");
        session
            .ensure_first_shell()
            .expect("first shell must start");
        session
            .apply(ClientMsg::Resize {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                cols: 1,
                rows: 1,
            })
            .expect("session must resize");
        let shared = SharedSession::new(session);
        let (first_server, _first_client) = UnixStream::pair().expect("stream pair must open");
        let (second_server, _second_client) = UnixStream::pair().expect("stream pair must open");
        shared
            .add_connection(1, first_server)
            .expect("first connection must be added");
        shared
            .add_connection(2, second_server)
            .expect("second connection must be added");

        shared
            .remove_connection(1)
            .expect("connection must be removed");
        let connections = lock(&shared.connections).expect("connections must lock");
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].id, 2);
    }

    #[test]
    fn polls_output_without_connections() {
        let mut session = UserSession::new("alice", "sh");
        session
            .ensure_first_shell()
            .expect("first shell must start");
        let shared = SharedSession::new(session);
        shared
            .apply_and_broadcast(ClientMsg::Input {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: "w1:p1".into(),
                bytes: b"printf detached-output\\n".to_vec(),
            })
            .expect("input must succeed");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut updated = false;
        while Instant::now() < deadline {
            let (messages, has_connections) = shared
                .poll_and_broadcast()
                .expect("session poll must succeed");
            assert!(!has_connections);
            updated = messages.iter().any(|message| match message {
                ServerMsg::Cells { rows, .. } => rows
                    .iter()
                    .flatten()
                    .map(|cell| cell.character)
                    .collect::<String>()
                    .contains("detached-output"),
                _ => false,
            });
            if updated {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(updated);
        shared
            .apply_and_broadcast(ClientMsg::ClosePane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: "w1:p1".into(),
            })
            .expect("pane close must succeed");
    }

    #[test]
    fn evicts_stalled_connection_without_blocking_other_client() {
        let shared = SharedSession::new(UserSession::new("alice", "sh"));
        let (stalled_server, _stalled_client) =
            UnixStream::pair().expect("stalled stream pair must open");
        let (healthy_server, mut healthy_client) =
            UnixStream::pair().expect("healthy stream pair must open");
        healthy_client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout must set");
        shared
            .add_connection(1, stalled_server)
            .expect("stalled connection must be added");
        shared
            .add_connection(2, healthy_server)
            .expect("healthy connection must be added");
        let _: seer_core::proto::ServerMsg =
            seer_core::proto::codec::decode(&mut healthy_client).expect("initial tree must decode");

        let reader = thread::spawn(move || {
            loop {
                let message: seer_core::proto::ServerMsg =
                    seer_core::proto::codec::decode(&mut healthy_client)
                        .expect("healthy output must decode");
                if matches!(message, seer_core::proto::ServerMsg::Frame { pane, .. } if pane == "recovered")
                {
                    return;
                }
            }
        });
        let large = seer_core::proto::ServerMsg::Frame {
            pane: "fill".into(),
            bytes: vec![b'x'; 256 * 1024],
        };
        for _ in 0..68 {
            shared
                .broadcast(std::slice::from_ref(&large))
                .expect("large output must broadcast");
            if shared.connection_count() == 1 {
                break;
            }
        }

        assert_eq!(shared.connection_count(), 1);
        shared
            .broadcast(&[seer_core::proto::ServerMsg::Frame {
                pane: "recovered".into(),
                bytes: Vec::new(),
            }])
            .expect("recovery output must broadcast");
        reader.join().expect("healthy reader must finish");
    }
}
