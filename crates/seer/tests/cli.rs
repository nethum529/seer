use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, ServerMsg, codec};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

#[test]
fn help_detach_and_missing_attach_have_exact_results() {
    let config = TestConfig::new();

    let help = run(&config, &["--help"], "");
    assert_eq!(help.status.code(), Some(0));
    let help_text = text(&help.stdout);
    for command in [
        "start", "invite", "join", "list", "attach", "detach", "peek",
    ] {
        assert!(help_text.contains(command));
    }

    let detach = run(&config, &["detach"], "");
    assert_eq!(detach.status.code(), Some(1));
    assert_eq!(detach.stderr, b"no attached client in this shell\n");

    let attach = run(&config, &["attach"], "");
    assert_eq!(attach.status.code(), Some(1));
    assert_eq!(attach.stderr, b"run seer join first\n");

    let invalid = run(&config, &["unknown"], "");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert!(text(&invalid.stderr).starts_with("Usage: seer <command>\n"));
    for command in ["start", "invite", "join", "list", "attach", "detach"] {
        let extra = run(&config, &[command, "extra"], "");
        assert_eq!(extra.status.code(), Some(2));
        assert!(text(&extra.stderr).starts_with("Usage: seer <command>\n"));
    }
}

#[test]
fn join_retries_the_name_once_saves_private_store_and_attaches() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    let server = thread::spawn(move || {
        let mut first = accept(&listener);
        assert_eq!(
            receive(&mut first),
            ClientMsg::Join {
                seat_token: "seat-token".into(),
                name: "alice".into(),
            }
        );
        send(
            &mut first,
            &ServerMsg::Refused {
                reason: "name is in use".into(),
            },
        );

        let mut second = accept(&listener);
        assert_eq!(
            receive(&mut second),
            ClientMsg::Join {
                seat_token: "seat-token".into(),
                name: "bob".into(),
            }
        );
        send(
            &mut second,
            &ServerMsg::Joined {
                user_id: "user-bob".into(),
                credential: "device-secret".into(),
                name: "bob".into(),
            },
        );
        send_welcome(&mut second, "user-bob", "bob");
    });
    let capsule = format!(
        "SEER1-127.0.0.1-{}-seat-token\nalice\nbob\n",
        address.port()
    );

    let output = run(&config, &["join"], &capsule);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(
        text(&output.stdout),
        format!(
            "Invitation: Server: 127.0.0.1:{}\nName: That name is in use.\nName: Joined as bob. Attaching...\n",
            address.port()
        )
    );
    server.join().expect("server must finish");
    let store_path = config.root.join("seer/servers.toml");
    let store = fs::read_to_string(&store_path).expect("store must be readable");
    assert!(store.contains("name = \"bob\""));
    assert!(store.contains("credential = \"device-secret\""));
    assert_eq!(
        fs::metadata(&store_path)
            .expect("store metadata must load")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(config.root.join("seer"))
            .expect("directory metadata must load")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn attach_uses_the_saved_identity() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    write_store(&config, &[saved(address.port(), "team.example.com", true)]);
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let mut stream = accept(&listener);
            assert_hello(&mut stream);
            send_welcome(&mut stream, "user-bob", "bob");
        }
    });

    let output = run(&config, &["attach"], "");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"Attached to team.example.com as bob.\n");
    assert!(output.stderr.is_empty());

    let first = SavedServer {
        endpoint: "127.0.0.1:1".into(),
        alias: "first".into(),
        current: false,
    };
    write_store(&config, &[first, saved(address.port(), "second", true)]);
    let current = run(&config, &["attach"], "");
    assert_eq!(current.status.code(), Some(0));
    assert_eq!(current.stdout, b"Attached to second as bob.\n");
    server.join().expect("server must finish");
}

#[test]
fn picker_selects_one_of_several_servers() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    let first = SavedServer {
        endpoint: "127.0.0.1:1".into(),
        alias: "first".into(),
        current: false,
    };
    write_store(&config, &[first, saved(address.port(), "second", false)]);
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
    });

    let output = run(&config, &["attach"], "2\n");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        b"Select a server:\n  1. first (bob)\n  2. second (bob)\nServer: Attached to second as bob.\n"
    );
    assert!(output.stderr.is_empty());
    server.join().expect("server must finish");
}

#[test]
fn list_prints_people_and_marks_an_unreachable_server() {
    let config = TestConfig::new();
    let first_listener = listener();
    let address = first_listener
        .local_addr()
        .expect("listener must have an address");
    let second_listener = listener();
    let second_address = second_listener
        .local_addr()
        .expect("second listener must have an address");
    let offline = SavedServer {
        endpoint: "127.0.0.1:1".into(),
        alias: "offline".into(),
        current: false,
    };
    write_store(
        &config,
        &[
            offline,
            saved(address.port(), "team.example.com", true),
            saved(second_address.port(), "second", false),
        ],
    );
    let server = thread::spawn(move || {
        let mut stream = accept(&first_listener);
        assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
        assert_eq!(receive(&mut stream), ClientMsg::ListPeople);
        send(
            &mut stream,
            &ServerMsg::People {
                people: vec![
                    person("user-bob", "bob", 0),
                    person("user-alice", "alice", 2),
                ],
            },
        );
    });
    let second_server = thread::spawn(move || {
        let mut stream = accept(&second_listener);
        assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
        assert_eq!(receive(&mut stream), ClientMsg::ListPeople);
        send(
            &mut stream,
            &ServerMsg::People {
                people: vec![person("user-bob", "bob", 1)],
            },
        );
    });

    let output = run(&config, &["list"], "");

    assert_eq!(output.status.code(), Some(0));
    let stdout = text(&output.stdout);
    assert!(stdout.contains("SERVER            YOU  STATE        PEOPLE\n"));
    assert!(stdout.contains("team.example.com  bob  detached     alice, bob\n"));
    assert!(stdout.contains("offline           bob  unreachable"));
    assert!(stdout.contains("second            bob  detached"));
    assert!(
        stdout
            .find("team.example.com")
            .expect("current row must exist")
            < stdout.find("offline").expect("offline row must exist")
    );
    assert!(!stdout.contains("device-secret"));
    assert!(output.stderr.is_empty());
    server.join().expect("server must finish");
    second_server.join().expect("second server must finish");
}

#[test]
fn invite_prints_the_worked_example_block() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    write_store(&config, &[saved(address.port(), "team.example.com", true)]);
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
        assert_eq!(receive(&mut stream), ClientMsg::Invite);
        send(
            &mut stream,
            &ServerMsg::Seat {
                capsule: "SEER1-team.example.com-7321-A7K4Q9P2".into(),
                expires_in_secs: 3_600,
            },
        );
    });

    let output = run(&config, &["invite"], "");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        b"Seat ready. It works once and expires in 1 hour.\nSend this invitation through a private channel:\n\nSEER1-team.example.com-7321-A7K4Q9P2\n"
    );
    assert!(output.stderr.is_empty());
    server.join().expect("server must finish");
}

#[test]
fn peek_requires_an_exact_name_and_sends_the_user_id() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    write_store(&config, &[saved(address.port(), "team.example.com", true)]);
    let server = thread::spawn(move || {
        for exact in [false, true] {
            let mut stream = accept(&listener);
            assert_hello(&mut stream);
            send_welcome(&mut stream, "user-bob", "bob");
            assert_eq!(receive(&mut stream), ClientMsg::ListPeople);
            send(
                &mut stream,
                &ServerMsg::People {
                    people: vec![person("user-alice", "alice", 1)],
                },
            );
            if exact {
                assert_eq!(
                    receive(&mut stream),
                    ClientMsg::Peek {
                        user: "user-alice".into(),
                        workspace: "w1".into(),
                    }
                );
            }
        }
    });

    let close = run(&config, &["peek", "alic"], "");
    assert_eq!(close.status.code(), Some(1));
    assert!(close.stdout.is_empty());
    assert_eq!(close.stderr, b"Close names: alice\nno person named alic\n");

    let exact = run(&config, &["peek", "alice"], "");
    assert_eq!(exact.status.code(), Some(0));
    assert_eq!(
        exact.stdout,
        b"PEEK: alice - READ ONLY\nWorkspace: alice/current\n"
    );
    assert!(exact.stderr.is_empty());
    server.join().expect("server must finish");
}

fn assert_hello(stream: &mut TcpStream) {
    assert_eq!(
        receive(stream),
        ClientMsg::Hello {
            user_id: "user-bob".into(),
            credential: "device-secret".into(),
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

fn person(user_id: &str, name: &str, attached_clients: u32) -> Person {
    Person {
        user_id: user_id.into(),
        name: name.into(),
        attached_clients,
        peekable: true,
    }
}

fn send(stream: &mut TcpStream, message: &ServerMsg) {
    codec::encode(stream, message).expect("server message must encode");
}

fn receive(stream: &mut TcpStream) -> ClientMsg {
    codec::decode(stream).expect("client message must decode")
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
                    .set_read_timeout(Some(Duration::from_secs(5)))
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

fn write_store(config: &TestConfig, servers: &[SavedServer]) {
    let directory = config.root.join("seer");
    fs::create_dir_all(&directory).expect("config directory must exist");
    let mut contents = String::new();
    for server in servers {
        contents.push_str(&format!(
            "[[servers]]\nendpoint = \"{}\"\nalias = \"{}\"\nuser_id = \"user-bob\"\nname = \"bob\"\ncredential = \"device-secret\"\ncurrent = {}\n",
            server.endpoint, server.alias, server.current
        ));
    }
    fs::write(directory.join("servers.toml"), contents).expect("store must be written");
}

fn saved(port: u16, alias: &str, current: bool) -> SavedServer {
    SavedServer {
        endpoint: format!("127.0.0.1:{port}"),
        alias: alias.into(),
        current,
    }
}

struct SavedServer {
    endpoint: String,
    alias: String,
    current: bool,
}

struct TestConfig {
    root: PathBuf,
}

impl TestConfig {
    fn new() -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = Path::new("/tmp").join(format!("s2-{}-{number}", std::process::id()));
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
