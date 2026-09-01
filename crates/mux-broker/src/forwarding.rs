use std::io;
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use mux_core::proto::{ClientMsg, ServerMsg, codec};

use crate::UserConfig;
use crate::runtime::RuntimeManager;

pub(crate) fn forward(
    client: TcpStream,
    owner: &str,
    users: &[UserConfig],
    runtimes: &RuntimeManager,
) -> io::Result<()> {
    let mut coordinator = Coordinator::new(client, owner, users, runtimes)?;
    let result = coordinator.run();
    result.and(coordinator.close())
}

struct Coordinator<'a> {
    client: TcpStream,
    client_reader: Option<JoinHandle<()>>,
    owner: &'a str,
    users: &'a [UserConfig],
    runtimes: &'a RuntimeManager,
    event_sender: Sender<Event>,
    events: Receiver<Event>,
    runtime: Option<RuntimeConnection>,
    peeking: bool,
}

impl<'a> Coordinator<'a> {
    fn new(
        client: TcpStream,
        owner: &'a str,
        users: &'a [UserConfig],
        runtimes: &'a RuntimeManager,
    ) -> io::Result<Self> {
        let (event_sender, events) = mpsc::channel();
        let client_reader = client.try_clone()?;
        let runtime = connect_runtime(runtimes, owner, None, &event_sender)?;
        let client_reader = spawn_client_reader(client_reader, event_sender.clone());
        Ok(Self {
            client,
            client_reader: Some(client_reader),
            owner,
            users,
            runtimes,
            event_sender,
            events,
            runtime: Some(runtime),
            peeking: false,
        })
    }

    fn run(&mut self) -> io::Result<()> {
        loop {
            let event = self
                .events
                .recv()
                .map_err(|_| io::Error::other("forward event channel closed"))?;
            let action = match event {
                Event::Client(result) => self.handle_client_result(result)?,
                Event::Runtime { identity, result } => {
                    self.handle_runtime_result(&identity, result)?
                }
            };
            if action == Action::Stop {
                return Ok(());
            }
        }
    }

    fn handle_client_result(&mut self, result: io::Result<ClientMsg>) -> io::Result<Action> {
        match result {
            Ok(message) => self.handle_client_message(message),
            Err(_) => Ok(Action::Stop),
        }
    }

    fn handle_client_message(&mut self, message: ClientMsg) -> io::Result<Action> {
        match message {
            ClientMsg::Peek { ref user, .. } if self.user_exists(user) => {
                self.switch_runtime(user, Some(&message))?;
                self.peeking = true;
            }
            ClientMsg::Peek { user, .. } => {
                eprintln!("broker dropped Peek for unknown user: {user}");
            }
            ClientMsg::StopPeek if self.peeking => self.stop_peek()?,
            ClientMsg::StopPeek => {}
            ClientMsg::Input { .. } if self.peeking => {
                eprintln!("broker dropped Input while user {} peeks", self.owner);
            }
            message => self.send_to_runtime(&message)?,
        }
        Ok(Action::Continue)
    }

    fn handle_runtime_result(
        &mut self,
        identity: &Arc<()>,
        result: io::Result<ServerMsg>,
    ) -> io::Result<Action> {
        if !self.runtime_is(identity) {
            return Ok(Action::Continue);
        }
        match result {
            Ok(message) => {
                codec::encode(&mut self.client, &message)?;
                Ok(Action::Continue)
            }
            Err(_) if self.peeking => {
                self.stop_peek()?;
                Ok(Action::Continue)
            }
            Err(_) => Ok(Action::Stop),
        }
    }

    fn user_exists(&self, user: &str) -> bool {
        self.users.iter().any(|entry| entry.user == user)
    }

    fn runtime_is(&self, identity: &Arc<()>) -> bool {
        self.runtime
            .as_ref()
            .is_some_and(|runtime| Arc::ptr_eq(&runtime.identity, identity))
    }

    fn send_to_runtime(&mut self, message: &ClientMsg) -> io::Result<()> {
        let runtime = self
            .runtime
            .as_mut()
            .ok_or_else(|| io::Error::other("runtime connection is missing"))?;
        codec::encode(&mut runtime.stream, message)
    }

    fn stop_peek(&mut self) -> io::Result<()> {
        self.peeking = false;
        self.switch_runtime(self.owner, None)
    }

    fn switch_runtime(&mut self, user: &str, first: Option<&ClientMsg>) -> io::Result<()> {
        self.close_runtime()?;
        self.runtime = Some(connect_runtime(
            self.runtimes,
            user,
            first,
            &self.event_sender,
        )?);
        Ok(())
    }

    fn close_runtime(&mut self) -> io::Result<()> {
        match self.runtime.take() {
            Some(runtime) => runtime.close(),
            None => Ok(()),
        }
    }

    fn close(&mut self) -> io::Result<()> {
        let _ = self.client.shutdown(std::net::Shutdown::Both);
        let runtime_result = self.close_runtime();
        let client_result = match self.client_reader.take() {
            Some(reader) => join_reader(reader),
            None => Ok(()),
        };
        runtime_result.and(client_result)
    }
}

struct RuntimeConnection {
    stream: UnixStream,
    identity: Arc<()>,
    reader: JoinHandle<()>,
}

fn connect_runtime(
    runtimes: &RuntimeManager,
    user: &str,
    first: Option<&ClientMsg>,
    sender: &Sender<Event>,
) -> io::Result<RuntimeConnection> {
    let mut stream = runtimes.connect(user)?;
    if let Some(message) = first {
        codec::encode(&mut stream, message)?;
    }
    let identity = Arc::new(());
    let reader = spawn_runtime_reader(stream.try_clone()?, Arc::clone(&identity), sender.clone());
    Ok(RuntimeConnection {
        stream,
        identity,
        reader,
    })
}

impl RuntimeConnection {
    fn close(self) -> io::Result<()> {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        join_reader(self.reader)
    }
}

impl Drop for Coordinator<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

enum Event {
    Client(io::Result<ClientMsg>),
    Runtime {
        identity: Arc<()>,
        result: io::Result<ServerMsg>,
    },
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Action {
    Continue,
    Stop,
}

fn spawn_client_reader(mut stream: TcpStream, sender: Sender<Event>) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    if sender.send(Event::Client(Ok(message))).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Event::Client(Err(error)));
                    return;
                }
            }
        }
    })
}

fn spawn_runtime_reader(
    mut stream: UnixStream,
    identity: Arc<()>,
    sender: Sender<Event>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    let event = Event::Runtime {
                        identity: Arc::clone(&identity),
                        result: Ok(message),
                    };
                    if sender.send(event).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Event::Runtime {
                        identity: Arc::clone(&identity),
                        result: Err(error),
                    });
                    return;
                }
            }
        }
    })
}

fn join_reader(reader: JoinHandle<()>) -> io::Result<()> {
    reader
        .join()
        .map_err(|_| io::Error::other("forward reader thread panicked"))
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use mux_core::proto::{ClientMsg, codec};

    use super::{Action, Coordinator, RuntimeConnection, join_reader, spawn_client_reader};
    use crate::runtime::RuntimeManager;

    const WAIT_TIMEOUT: Duration = Duration::from_secs(1);

    #[test]
    fn stop_peek_does_nothing_in_normal_mode() {
        let runtimes = RuntimeManager::new("sh".into()).expect("runtime manager must initialize");
        let (client, _peer) = tcp_pair();
        let mut coordinator = coordinator(client, &runtimes);
        let _runtime_peer = attach_runtime(&mut coordinator);
        let identity = Arc::clone(
            &coordinator
                .runtime
                .as_ref()
                .expect("runtime connection must exist")
                .identity,
        );

        let action = coordinator
            .handle_client_message(ClientMsg::StopPeek)
            .expect("StopPeek must succeed");

        assert!(action == Action::Continue);
        assert!(coordinator.runtime_is(&identity));
    }

    #[test]
    fn runtime_connection_close_shuts_down_its_reader() {
        let (stream, peer) = UnixStream::pair().expect("runtime pair must open");
        peer.set_read_timeout(Some(WAIT_TIMEOUT))
            .expect("read timeout must set");
        let (sender, _events) = mpsc::channel();
        let identity = Arc::new(());
        let reader = super::spawn_runtime_reader(
            stream.try_clone().expect("runtime stream must clone"),
            Arc::clone(&identity),
            sender,
        );
        let runtime = RuntimeConnection {
            stream,
            identity,
            reader,
        };

        runtime.close().expect("runtime must close");

        assert_unix_closed(peer);
    }

    #[test]
    fn coordinator_closes_only_once() {
        let runtimes = RuntimeManager::new("sh".into()).expect("runtime manager must initialize");
        let (client, peer) = tcp_pair();
        let mut coordinator = coordinator(client, &runtimes);
        let runtime_peer = attach_runtime(&mut coordinator);

        coordinator.close().expect("coordinator must close");

        assert_tcp_closed(peer);
        assert_unix_closed(runtime_peer);
    }

    #[test]
    fn close_runtime_removes_and_closes_the_connection() {
        let runtimes = RuntimeManager::new("sh".into()).expect("runtime manager must initialize");
        let (client, _peer) = tcp_pair();
        let mut coordinator = coordinator(client, &runtimes);
        let runtime_peer = attach_runtime(&mut coordinator);

        coordinator
            .close_runtime()
            .expect("runtime connection must close");

        assert!(coordinator.runtime.is_none());
        assert_unix_closed(runtime_peer);
    }

    #[test]
    fn dropping_coordinator_closes_the_runtime_connection() {
        let runtimes = RuntimeManager::new("sh".into()).expect("runtime manager must initialize");
        let (client, _peer) = tcp_pair();
        let mut coordinator = coordinator(client, &runtimes);
        let runtime_peer = attach_runtime(&mut coordinator);

        drop(coordinator);

        assert_unix_closed(runtime_peer);
    }

    #[test]
    fn client_reader_stops_if_the_event_receiver_is_gone() {
        let (server, mut client) = tcp_pair();
        let (sender, events) = mpsc::channel();
        drop(events);
        let reader = spawn_client_reader(server, sender);

        codec::encode(&mut client, &ClientMsg::CreateTab).expect("message must encode");
        let stopped = wait_for_thread(&reader);
        if !stopped {
            let _ = client.shutdown(std::net::Shutdown::Both);
        }
        reader.join().expect("reader must not panic");

        assert!(stopped);
    }

    #[test]
    fn runtime_reader_stops_after_the_connection_ends() {
        let (stream, peer) = UnixStream::pair().expect("runtime pair must open");
        let (sender, events) = mpsc::channel();
        let reader = super::spawn_runtime_reader(stream, Arc::new(()), sender);
        drop(peer);

        let stopped = wait_for_thread(&reader);
        drop(events);
        reader.join().expect("reader must not panic");

        assert!(stopped);
    }

    #[test]
    fn join_reader_reports_a_panic() {
        let reader = thread::spawn(|| panic!("test panic"));

        assert!(join_reader(reader).is_err());
    }

    fn coordinator<'a>(client: TcpStream, runtimes: &'a RuntimeManager) -> Coordinator<'a> {
        let (event_sender, events) = mpsc::channel();
        Coordinator {
            client,
            client_reader: None,
            owner: "alice",
            users: &[],
            runtimes,
            event_sender,
            events,
            runtime: None,
            peeking: false,
        }
    }

    fn attach_runtime(coordinator: &mut Coordinator<'_>) -> UnixStream {
        let (stream, peer) = UnixStream::pair().expect("runtime pair must open");
        peer.set_read_timeout(Some(WAIT_TIMEOUT))
            .expect("read timeout must set");
        let identity = Arc::new(());
        let reader = super::spawn_runtime_reader(
            stream.try_clone().expect("runtime stream must clone"),
            Arc::clone(&identity),
            coordinator.event_sender.clone(),
        );
        coordinator.runtime = Some(RuntimeConnection {
            stream,
            identity,
            reader,
        });
        peer
    }

    fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        let address = listener.local_addr().expect("listener must have address");
        let client = TcpStream::connect(address).expect("client must connect");
        let (server, _) = listener.accept().expect("server must accept");
        client
            .set_read_timeout(Some(WAIT_TIMEOUT))
            .expect("read timeout must set");
        (server, client)
    }

    fn assert_tcp_closed(mut stream: TcpStream) {
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::UnexpectedEof
                ) => {}
            result => panic!("TCP stream must close: {result:?}"),
        }
    }

    fn assert_unix_closed(mut stream: UnixStream) {
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::UnexpectedEof
                ) => {}
            result => panic!("Unix stream must close: {result:?}"),
        }
    }

    fn wait_for_thread(thread: &thread::JoinHandle<()>) -> bool {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            if thread.is_finished() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}
