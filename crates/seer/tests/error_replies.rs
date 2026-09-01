use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/server_io.rs"]
mod server_io;

use server_io::receive;

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

#[test]
fn join_reports_refused_and_unexpected_replies() {
    let refused = run_join(ServerMsg::Refused {
        reason: "seat expired".into(),
    });
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(refused.stderr, b"seat expired\n");

    let unexpected = run_join(ServerMsg::Tree { tree: Tree::new() });
    assert_eq!(unexpected.status.code(), Some(2));
    assert_eq!(unexpected.stderr, b"error: unexpected server reply\n");
}

#[test]
fn invite_reports_refused_and_unexpected_replies() {
    let refused = run_invite(ServerMsg::Refused {
        reason: "owner only".into(),
    });
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(refused.stderr, b"owner only\n");

    let unexpected = run_invite(ServerMsg::Bye {
        reason: "server stopped".into(),
    });
    assert_eq!(unexpected.status.code(), Some(2));
    assert_eq!(unexpected.stderr, b"error: unexpected server reply\n");
}

#[test]
#[cfg(target_os = "linux")]
fn join_hides_the_invitation_on_a_terminal() {
    let config = TestConfig::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let port = listener
        .local_addr()
        .expect("listener must have an address")
        .port();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        let join = receive(&mut stream);
        assert_eq!(
            join,
            ClientMsg::Join {
                seat_token: "private-seat".into(),
                name: "bob".into(),
            }
        );
        codec::encode(
            &mut stream,
            &ServerMsg::Joined {
                user_id: "user-bob".into(),
                credential: "secret".into(),
                name: "bob".into(),
            },
        )
        .expect("Joined must encode");
        drop(stream);

        let mut attached = accept(&listener);
        assert_eq!(
            receive(&mut attached),
            ClientMsg::Hello {
                user_id: "user-bob".into(),
                credential: "secret".into(),
            }
        );
        codec::encode(
            &mut attached,
            &ServerMsg::Welcome {
                user_id: "user-bob".into(),
                name: "bob".into(),
                client_id: "client-1".into(),
                tree: Tree::new(),
            },
        )
        .expect("Welcome must encode");
        codec::encode(
            &mut attached,
            &ServerMsg::Bye {
                reason: "test complete".into(),
            },
        )
        .expect("Bye must encode");
    });
    let input = format!("SEER1-127.0.0.1-{port}-private-seat\nbob\n");

    let output = run_terminal(&config, &input);

    assert_eq!(output.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-seat"));
    server.join().expect("server must finish");
}

fn run_join(reply: ServerMsg) -> Output {
    let config = TestConfig::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let port = listener
        .local_addr()
        .expect("listener must have an address")
        .port();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        let message = receive(&mut stream);
        assert!(matches!(message, ClientMsg::Join { .. }));
        codec::encode(&mut stream, &reply).expect("reply must encode");
    });
    let input = format!("SEER1-127.0.0.1-{port}-seat\nbob\n");
    let output = run(&config, "join", &input);
    server.join().expect("server must finish");
    output
}

fn run_invite(reply: ServerMsg) -> Output {
    let config = TestConfig::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let port = listener
        .local_addr()
        .expect("listener must have an address")
        .port();
    write_store(&config.root, port);
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        let hello = receive(&mut stream);
        assert!(matches!(hello, ClientMsg::Hello { .. }));
        codec::encode(
            &mut stream,
            &ServerMsg::Welcome {
                user_id: "user-bob".into(),
                name: "bob".into(),
                client_id: "client-1".into(),
                tree: Tree::new(),
            },
        )
        .expect("Welcome must encode");
        let invite = receive(&mut stream);
        assert_eq!(invite, ClientMsg::Invite);
        codec::encode(&mut stream, &reply).expect("reply must encode");
    });
    let output = run(&config, "invite", "");
    server.join().expect("server must finish");
    output
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

fn run(config: &TestConfig, command: &str, input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seer"))
        .arg(command)
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
            return child.wait_with_output().expect("output must be read");
        }
        if Instant::now() >= deadline {
            child.kill().expect("seer must be killed after timeout");
            let _ = child.wait();
            panic!("seer did not finish before timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
fn run_terminal(config: &TestConfig, input: &str) -> Output {
    let command = format!("{} join", env!("CARGO_BIN_EXE_seer"));
    let mut child = Command::new("script")
        .args(["-qec", command.as_str(), "/dev/null"])
        .env("XDG_CONFIG_HOME", &config.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("terminal wrapper must start");
    thread::sleep(Duration::from_millis(250));
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
            .expect("terminal wrapper status must be available")
            .is_some()
        {
            return child
                .wait_with_output()
                .expect("terminal output must be read");
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .expect("terminal wrapper must be killed after timeout");
            let _ = child.wait();
            panic!("terminal wrapper did not finish before timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn write_store(root: &Path, port: u16) {
    let directory = root.join("seer");
    fs::create_dir_all(&directory).expect("config directory must exist");
    fs::write(
        directory.join("servers.toml"),
        format!(
            "[[servers]]\nendpoint = \"127.0.0.1:{port}\"\nalias = \"team.example.com\"\nuser_id = \"user-bob\"\nname = \"bob\"\ncredential = \"secret\"\ncurrent = true\n"
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
        let root = Path::new("/tmp").join(format!("s2e-{}-{number}", std::process::id()));
        fs::create_dir_all(&root).expect("test directory must exist");
        Self { root }
    }
}

impl Drop for TestConfig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
