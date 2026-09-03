use seer_core::proto::{ClientMsg, ServerMsg, codec};
use std::fs;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use crate::UserSession;

const POLL_INTERVAL: Duration = Duration::from_millis(20);

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
            match shared.send_selected_tree(connection_id, &workspace, &tab) {
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
            shared.apply_and_broadcast(message)?;
            Ok(false)
        }
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
        connections.push(Connection { id, stream });
        self.connection_opened.notify_one();
        Ok(())
    }

    fn remove_connection(&self, id: u64) -> io::Result<()> {
        lock(&self.connections)?.retain(|connection| connection.id != id);
        Ok(())
    }

    fn send_selected_tree(&self, id: u64, workspace: &str, tab: &str) -> io::Result<()> {
        let session = lock(&self.session)?;
        let message = ServerMsg::Tree {
            tree: session.selected_tree(workspace, tab)?,
        };
        drop(session);
        self.send_to(id, &message)
    }

    fn send_refused(&self, id: u64, reason: String) -> io::Result<()> {
        self.send_to(id, &ServerMsg::Refused { reason })
    }

    fn send_to(&self, id: u64, message: &ServerMsg) -> io::Result<()> {
        let mut connections = lock(&self.connections)?;
        let position = connections
            .iter()
            .position(|connection| connection.id == id)
            .ok_or_else(connection_closed)?;
        let result = write_messages(
            &mut connections[position].stream,
            std::slice::from_ref(message),
        );
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
mod tests {
    use super::{ClientMsg, SharedSession, is_mutating, lock};
    use crate::UserSession;
    use std::os::unix::net::UnixStream;

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
            ClientMsg::Invite,
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
        let shared = SharedSession::new(UserSession::new("alice", "sh"));
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
}
