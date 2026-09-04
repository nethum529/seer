use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, PeekTarget, ServerMsg};

#[path = "support/server_io.rs"]
mod server_io;

use server_io::receive;

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

#[test]
fn peek_resolves_an_active_target_and_handles_selection_failures() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    write_store(&config, address.port());
    let server = thread::spawn(move || {
        let mut close = accept(&listener);
        prepare_people(&mut close);

        let mut active = prepare_query(&listener);
        send(
            &mut active,
            &ServerMsg::Targets {
                targets: targets(true),
            },
        );
        assert_peek(&mut active, "w4", "w4:t9");
        send(&mut active, &ServerMsg::Tree { tree: Tree::new() });

        let mut picker = prepare_query(&listener);
        send(
            &mut picker,
            &ServerMsg::Targets {
                targets: targets(false),
            },
        );
        assert_peek(&mut picker, "w4", "w4:t9");
        send(&mut picker, &ServerMsg::Tree { tree: Tree::new() });

        let mut empty = prepare_query(&listener);
        send(
            &mut empty,
            &ServerMsg::Targets {
                targets: Vec::new(),
            },
        );

        let mut stale = prepare_query(&listener);
        send(
            &mut stale,
            &ServerMsg::Targets {
                targets: vec![target("w9", "w9:t7", "old", true)],
            },
        );
        assert_peek(&mut stale, "w9", "w9:t7");
        send(
            &mut stale,
            &ServerMsg::Refused {
                reason: "tab not found".into(),
            },
        );
    });

    let close = run(&config, &["peek", "alic"], "");
    assert_eq!(close.status.code(), Some(1));
    assert!(close.stdout.is_empty());
    assert_eq!(close.stderr, b"Close names: alice\nno person named alic\n");

    let active = run(&config, &["peek", "alice"], "");
    assert_eq!(active.status.code(), Some(0));
    assert_eq!(
        active.stdout,
        b"PEEK: alice - READ ONLY\nWorkspace: alice/w4\n"
    );
    assert!(active.stderr.is_empty());

    let picker = run(&config, &["peek", "alice"], "2\n");
    assert_eq!(picker.status.code(), Some(0));
    assert_eq!(
        text(&picker.stdout),
        "Select a target:\n  1. main/shell\n  2. work/tests\nTarget: PEEK: alice - READ ONLY\nWorkspace: alice/w4\n"
    );
    assert!(picker.stderr.is_empty());

    let empty = run(&config, &["peek", "alice"], "");
    assert_eq!(empty.status.code(), Some(1));
    assert!(empty.stdout.is_empty());
    assert_eq!(empty.stderr, b"no active target\n");

    let stale = run(&config, &["peek", "alice"], "");
    assert_eq!(stale.status.code(), Some(1));
    assert!(stale.stdout.is_empty());
    assert_eq!(stale.stderr, b"tab not found\n");
    server.join().expect("server must finish");
}

fn prepare_people(stream: &mut TcpStream) {
    assert_hello(stream);
    send_welcome(stream, "user-bob", "bob");
    assert_eq!(receive(stream), ClientMsg::ListPeople);
    send(
        stream,
        &ServerMsg::People {
            people: vec![person("user-alice", "alice", 1)],
        },
    );
}

fn prepare_query(listener: &TcpListener) -> TcpStream {
    let mut stream = accept(listener);
    prepare_people(&mut stream);
    assert_eq!(
        receive(&mut stream),
        ClientMsg::QueryTargets {
            user: "user-alice".into(),
        }
    );
    stream
}

fn assert_peek(stream: &mut TcpStream, workspace: &str, tab: &str) {
    assert_eq!(
        receive(stream),
        ClientMsg::Peek {
            user: "user-alice".into(),
            workspace: workspace.into(),
            tab: tab.into(),
        }
    );
}

fn targets(second_active: bool) -> Vec<PeekTarget> {
    vec![
        target("w1", "w1:t8", "main", false),
        target("w4", "w4:t9", "work", second_active),
    ]
}

fn target(workspace: &str, tab: &str, name: &str, active: bool) -> PeekTarget {
    PeekTarget {
        workspace: workspace.into(),
        workspace_name: name.into(),
        tab: tab.into(),
        tab_title: if workspace == "w4" {
            "tests".into()
        } else {
            "shell".into()
        },
        active,
    }
}

fn assert_hello(stream: &mut TcpStream) {
    assert_eq!(
        receive(stream),
        ClientMsg::Hello {
            user_id: "user-bob".into(),
            credential: "device-secret".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    );
}

fn send_welcome(stream: &mut TcpStream, user_id: &str, name: &str) {
    send(
        stream,
        &ServerMsg::Welcome {
            user_id: user_id.into(),
            name: name.into(),
            client_id: "client-1".into(),
            tree: Tree::new(),
        },
    );
}

fn person(user_id: &str, name: &str, attached_clients: u32) -> seer_core::proto::Person {
    seer_core::proto::Person {
        user_id: user_id.into(),
        name: name.into(),
        attached_clients,
        peekable: true,
    }
}

fn send(stream: &mut TcpStream, message: &ServerMsg) {
    seer_core::proto::codec::encode(stream, message).expect("server message must encode");
}

fn listener() -> TcpListener {
    TcpListener::bind("127.0.0.1:0").expect("listener must bind")
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
                    .expect("read timeout must be set");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("write timeout must be set");
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

fn run(config: &TestConfig, arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seer"))
        .args(arguments)
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
        .expect("input must be written");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child
            .try_wait()
            .expect("seer status must be available")
            .is_some()
        {
            return child.wait_with_output().expect("seer output must be read");
        }
        if Instant::now() >= deadline {
            child.kill().expect("seer must be killed after timeout");
            let _ = child.wait();
            panic!("seer did not finish before timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn write_store(config: &TestConfig, port: u16) {
    let directory = config.root.join("seer");
    fs::create_dir_all(&directory).expect("config directory must exist");
    fs::write(
        directory.join("servers.toml"),
        format!(
            "[[servers]]\nendpoint = \"127.0.0.1:{port}\"\nalias = \"team.example.com\"\nuser_id = \"user-bob\"\nname = \"bob\"\ncredential = \"device-secret\"\ncurrent = true\n"
        ),
    )
    .expect("store must be written");
}

struct TestConfig {
    root: PathBuf,
}

impl TestConfig {
    fn new() -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = Path::new("/tmp").join(format!("s2-peek-{}-{number}", std::process::id()));
        fs::create_dir_all(&root).expect("test directory must exist");
        Self { root }
    }
}

impl Drop for TestConfig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("output must be UTF-8")
}
