use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientInfo, ClientMsg, ServerMsg, codec};

#[path = "support/server_io.rs"]
mod server_io;

use server_io::receive;

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

#[test]
fn detach_reports_zero_clients_and_detaches_one_client() {
    let zero = run_detach(Vec::new(), "");
    assert_eq!(zero.status.code(), Some(1));
    assert!(zero.stdout.is_empty());
    assert_eq!(zero.stderr, b"no attached client\n");

    let one = run_detach(vec![client("client-one", 4)], "");
    assert_eq!(one.status.code(), Some(0));
    assert_eq!(
        one.stdout,
        b"Detached from team.example.com. Your panes are still running.\n"
    );
    assert!(one.stderr.is_empty());
}

#[test]
fn detach_picker_selects_one_of_several_clients() {
    let output = run_detach(
        vec![client("client-one", 4), client("client-two", 9)],
        "2\n",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        b"Select a client:\n  1. client-one (connected 4 seconds)\n  2. client-two (connected 9 seconds)\nClient: Detached from team.example.com. Your panes are still running.\n"
    );
    assert!(output.stderr.is_empty());
}

fn run_detach(clients: Vec<ClientInfo>, input: &str) -> Output {
    let config = TestConfig::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let port = listener
        .local_addr()
        .expect("listener must have an address")
        .port();
    write_store(&config.root, port);
    let expected_client = match clients.as_slice() {
        [] => None,
        [client] => Some(client.client_id.clone()),
        [_, second, ..] => Some(second.client_id.clone()),
    };
    let server = thread::spawn(move || serve_detach(listener, clients, expected_client));

    let output = run(&config, input);

    server.join().expect("server must finish");
    output
}

fn serve_detach(listener: TcpListener, clients: Vec<ClientInfo>, expected_client: Option<String>) {
    let mut stream = accept(&listener);
    assert_eq!(
        receive(&mut stream),
        ClientMsg::Hello {
            user_id: "user-bob".into(),
            credential: "device-secret".into(),
        }
    );
    send(
        &mut stream,
        &ServerMsg::Welcome {
            user_id: "user-bob".into(),
            name: "bob".into(),
            client_id: "query-client".into(),
            tree: Tree::new(),
        },
    );
    assert_eq!(
        receive(&mut stream),
        ClientMsg::DetachClient {
            client_id: String::new()
        }
    );
    send(&mut stream, &ServerMsg::Clients { clients });
    if let Some(client_id) = expected_client {
        assert_eq!(receive(&mut stream), ClientMsg::DetachClient { client_id });
    }
}

fn accept(listener: &TcpListener) -> TcpStream {
    listener
        .set_nonblocking(true)
        .expect("listener must become nonblocking");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("accepted stream must become blocking");
                stream
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .expect("read timeout must set");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("write timeout must set");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "server accept timed out");
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("server accept failed: {error}"),
        }
    }
}

fn send(stream: &mut TcpStream, message: &ServerMsg) {
    codec::encode(stream, message).expect("server message must encode");
}

fn run(config: &TestConfig, input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seer"))
        .arg("detach")
        .env("XDG_CONFIG_HOME", &config.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("seer must start");
    child
        .stdin
        .take()
        .expect("stdin must be piped")
        .write_all(input.as_bytes())
        .expect("input must write");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child
            .try_wait()
            .expect("seer status must be available")
            .is_some()
        {
            return child.wait_with_output().expect("seer output must read");
        }
        if Instant::now() >= deadline {
            child.kill().expect("seer must be killed after timeout");
            let _ = child.wait();
            panic!("seer did not finish before timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn write_store(root: &Path, port: u16) {
    let directory = root.join("seer");
    fs::create_dir_all(&directory).expect("config directory must exist");
    let contents = format!(
        "[[servers]]\nendpoint = \"127.0.0.1:{port}\"\nalias = \"team.example.com\"\nuser_id = \"user-bob\"\nname = \"bob\"\ncredential = \"device-secret\"\ncurrent = true\n"
    );
    fs::write(directory.join("servers.toml"), contents).expect("store must write");
}

fn client(client_id: &str, connected_secs: u64) -> ClientInfo {
    ClientInfo {
        client_id: client_id.into(),
        connected_secs,
    }
}

struct TestConfig {
    root: PathBuf,
}

impl TestConfig {
    fn new() -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = PathBuf::from(format!("/tmp/sd-{}-{number}", std::process::id()));
        fs::create_dir_all(&root).expect("test directory must exist");
        Self { root }
    }
}

impl Drop for TestConfig {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("test directory must be removed");
    }
}
