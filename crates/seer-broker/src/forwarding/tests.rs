use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

use super::{Action, Coordinator, RuntimeConnection, join_reader, spawn_client_reader};
use crate::server::BrokerState;

const WAIT_TIMEOUT: Duration = Duration::from_secs(1);

#[test]
fn stop_peek_does_nothing_in_normal_mode() {
    let broker = TestBroker::new();
    let (client, _peer) = tcp_pair();
    let mut coordinator = coordinator(client, &broker.state);
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
fn owner_can_invite_and_list_people() {
    let broker = TestBroker::new();
    let (client, mut peer) = tcp_pair();
    let mut coordinator = coordinator(client, &broker.state);

    coordinator
        .handle_client_message(ClientMsg::Invite)
        .expect("Invite must succeed");
    let seat: ServerMsg = codec::decode(&mut peer).expect("Seat must decode");
    match seat {
        ServerMsg::Seat {
            capsule,
            expires_in_secs,
        } => {
            assert!(capsule.starts_with("SEER1-host-7321-"));
            assert_eq!(expires_in_secs, 3_600);
        }
        other => panic!("expected Seat, got {other:?}"),
    }

    coordinator
        .handle_client_message(ClientMsg::ListPeople)
        .expect("ListPeople must succeed");
    let people: ServerMsg = codec::decode(&mut peer).expect("People must decode");
    match people {
        ServerMsg::People { people } => {
            assert_eq!(people.len(), 1);
            assert_eq!(people[0].name, "Owner");
            assert_eq!(people[0].attached_clients, 0);
            assert!(!people[0].peekable);
        }
        other => panic!("expected People, got {other:?}"),
    }
}

#[test]
fn non_owner_invite_is_refused() {
    let broker = TestBroker::new();
    let (client, mut peer) = tcp_pair();
    let mut coordinator = coordinator(client, &broker.state);
    coordinator.owner_is_admin = false;

    coordinator
        .handle_client_message(ClientMsg::Invite)
        .expect("Invite refusal must send");

    assert_eq!(
        codec::decode::<_, ServerMsg>(&mut peer).expect("Refused must decode"),
        ServerMsg::Refused {
            reason: "owner access required".into()
        }
    );
}

#[test]
fn non_owner_invite_reports_a_closed_client() {
    let broker = TestBroker::new();
    let (client, _peer) = tcp_pair();
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("client writes must close");
    let mut coordinator = coordinator(client, &broker.state);
    coordinator.owner_is_admin = false;

    assert!(
        coordinator
            .handle_client_message(ClientMsg::Invite)
            .is_err()
    );
}

#[test]
fn coordinator_new_uses_the_authenticated_owner() {
    let broker = TestBroker::new();
    let owner = broker.state.registry().people().expect("people must load")[0].clone();
    let socket = crate::runtime::runtime_socket_path_for_test(&owner.user_id)
        .expect("runtime socket path must resolve");
    let _ = fs::remove_file(&socket);
    let listener =
        std::os::unix::net::UnixListener::bind(&socket).expect("runtime listener must bind");
    let (client, peer) = tcp_pair();

    let mut coordinator =
        Coordinator::new(client, &owner, &broker.state).expect("coordinator must initialize");
    let (runtime_peer, _) = listener.accept().expect("runtime must connect");

    assert_eq!(coordinator.owner, owner.user_id);
    assert!(coordinator.owner_is_admin);
    coordinator.close().expect("coordinator must close");
    drop(peer);
    drop(runtime_peer);
    drop(listener);
    fs::remove_file(socket).expect("runtime socket must be removed");
}

#[test]
fn peek_reports_registry_and_runtime_errors() {
    let broker = TestBroker::new();
    let (client, _peer) = tcp_pair();
    let mut coordinator = coordinator(client, &broker.state);
    let long_user = "u".repeat(100);
    assert!(coordinator.switch_runtime(&long_user, None).is_err());

    let poison = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        broker.state.registry().poison_for_test();
    }));
    assert!(poison.is_err());
    assert!(
        coordinator
            .handle_client_message(ClientMsg::Peek {
                user: "missing".into(),
                workspace: String::new(),
            })
            .is_err()
    );
}

#[test]
fn coordinator_resolves_registered_user_ids() {
    let broker = TestBroker::new();
    let owner_id = broker.state.registry().people().expect("people must load")[0]
        .user_id
        .clone();
    let (client, _peer) = tcp_pair();
    let coordinator = coordinator(client, &broker.state);

    assert!(
        coordinator
            .user_exists(&owner_id)
            .expect("lookup must finish")
    );
    assert!(
        !coordinator
            .user_exists("missing")
            .expect("lookup must finish")
    );
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
    let broker = TestBroker::new();
    let (client, peer) = tcp_pair();
    let mut coordinator = coordinator(client, &broker.state);
    let runtime_peer = attach_runtime(&mut coordinator);

    coordinator.close().expect("coordinator must close");

    assert_tcp_closed(peer);
    assert_unix_closed(runtime_peer);
}

#[test]
fn close_runtime_removes_and_closes_the_connection() {
    let broker = TestBroker::new();
    let (client, _peer) = tcp_pair();
    let mut coordinator = coordinator(client, &broker.state);
    let runtime_peer = attach_runtime(&mut coordinator);

    coordinator
        .close_runtime()
        .expect("runtime connection must close");

    assert!(coordinator.runtime.is_none());
    assert_unix_closed(runtime_peer);
}

#[test]
fn dropping_coordinator_closes_the_runtime_connection() {
    let broker = TestBroker::new();
    let (client, _peer) = tcp_pair();
    let mut coordinator = coordinator(client, &broker.state);
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

fn coordinator<'a>(client: TcpStream, broker: &'a BrokerState) -> Coordinator<'a> {
    let (event_sender, events) = mpsc::channel();
    Coordinator {
        client,
        client_reader: None,
        owner: "alice",
        owner_is_admin: true,
        broker,
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

struct TestBroker {
    state: BrokerState,
    directory: PathBuf,
}

impl TestBroker {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be valid")
            .as_nanos()
            % 1_000_000_000;
        let directory = PathBuf::from(format!("/tmp/sf-{}-{timestamp}", std::process::id()));
        let config = crate::Config {
            listen: "127.0.0.1:0".parse().expect("address must parse"),
            published_addr: "host:7321".into(),
            state_dir: directory.clone(),
            owner_name: "Owner".into(),
            shell: "sh".into(),
        };
        let (state, _) = BrokerState::new(&config).expect("broker state must initialize");
        Self { state, directory }
    }
}

impl Drop for TestBroker {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).expect("broker state must be removed");
    }
}
