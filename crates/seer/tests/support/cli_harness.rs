use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, PersonState, ServerMsg, codec};

use super::server_io::receive;

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct TestConfig {
    pub(crate) root: PathBuf,
}

impl TestConfig {
    pub(crate) fn new() -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = Path::new("/tmp").join(format!("s2-{}-{number}", std::process::id()));
        fs::create_dir_all(&root).expect("test directory must exist");
        Self { root }
    }
}

impl Drop for TestConfig {
    fn drop(&mut self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_seer"))
            .arg("stop")
            .env("XDG_CONFIG_HOME", &self.root)
            .env("XDG_STATE_HOME", self.root.join("state-home"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn listener() -> TcpListener {
    TcpListener::bind("127.0.0.1:0").expect("listener must bind")
}

pub(crate) fn accept(listener: &TcpListener) -> TcpStream {
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

pub(crate) fn run(config: &TestConfig, arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seer"))
        .args(arguments)
        .env("XDG_CONFIG_HOME", &config.root)
        .env("XDG_STATE_HOME", config.root.join("state-home"))
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

pub(crate) fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("output must be UTF-8")
}

pub(crate) fn assert_hello(stream: &mut impl Read) {
    assert_eq!(
        receive(stream),
        ClientMsg::Hello {
            user_id: "user-bob".into(),
            credential: "device-secret".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    );
}

pub(crate) fn send_welcome(stream: &mut impl Write, user_id: &str, name: &str) {
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

pub(crate) fn person(user_id: &str, name: &str, attached_clients: u32) -> Person {
    Person {
        online: true,
        user_id: user_id.into(),
        name: name.into(),
        attached_clients,
        peekable: true,
        host: false,
        state: PersonState::Idle,
        tabs: 2,
        foreground: "bash".into(),
        idle_secs: 90,
    }
}

pub(crate) fn send(stream: &mut impl Write, message: &ServerMsg) {
    codec::encode(stream, message).expect("server message must encode");
}
