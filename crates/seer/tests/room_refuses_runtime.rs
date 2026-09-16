// Issue 423: a runtime that the room refuses for the version tells the windows.
#![cfg(target_os = "linux")]
use std::fs;
use std::net::TcpListener;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

const NOTICE_WITHIN: Duration = Duration::from_secs(10);

#[test]
fn a_refused_runtime_tells_every_window_and_waits_before_it_tries_again() {
    let root = Path::new("/tmp").join(format!("seer-423-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("test directory must exist");
    let (address, attempts) = fake_room();
    let runtime = start_runtime(&root, &address);
    let socket = root.join("socket");

    let (mut first_window, at_attach) = greet(&socket);
    codec::encode(&mut first_window, &ClientMsg::AttachRuntime).expect("attach must encode");
    let first_refusal = attempts
        .recv_timeout(NOTICE_WITHIN)
        .expect("the runtime must publish to the room");
    let reason = at_attach.unwrap_or_else(|| wait_for_refusal(&mut first_window));
    assert!(reason.starts_with("version mismatch: server 0.99.0, client "));
    assert!(first_refusal.elapsed() < NOTICE_WITHIN);

    let (_, later) = greet(&socket);
    assert_eq!(later.as_deref(), Some(reason.as_str()));

    let quiet_until = Duration::from_secs(3).saturating_sub(first_refusal.elapsed());
    assert!(matches!(
        attempts.recv_timeout(quiet_until),
        Err(RecvTimeoutError::Timeout)
    ));
    drop(runtime);
    let _ = fs::remove_dir_all(&root);
}

fn fake_room() -> (String, Receiver<Instant>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake room must listen");
    let address = listener
        .local_addr()
        .expect("fake room must have an address")
        .to_string();
    let (sender, attempts) = mpsc::channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Ok(ClientMsg::PublishRuntime { version, .. }) = codec::decode(&mut stream) else {
                continue;
            };
            let reason =
                format!("version mismatch: server 0.99.0, client {version}. Run: seer update");
            let _ = codec::encode(&mut stream, &ServerMsg::Refused { reason });
            let _ = sender.send(Instant::now());
        }
    });
    (address, attempts)
}

fn start_runtime(root: &Path, address: &str) -> Runtime {
    Runtime(
        Command::new(env!("CARGO_BIN_EXE_seer-runtime"))
            .arg(root.join("socket"))
            .args(["owner-id", "/bin/sh", "refusal-test"])
            .env("SEER_ROOM_ENDPOINT", address)
            .env("SEER_ROOM_CREDENTIAL", "credential")
            .env("SEER_ROOM_KEY", root.join("key"))
            .env("SEER_SNAPSHOT_DIR", root.join("snapshots"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("runtime must start"),
    )
}

fn greet(socket: &PathBuf) -> (UnixStream, Option<String>) {
    let deadline = Instant::now() + NOTICE_WITHIN;
    loop {
        if let Ok(mut stream) = UnixStream::connect(socket) {
            stream
                .set_read_timeout(Some(NOTICE_WITHIN))
                .expect("read timeout must be set");
            match codec::decode(&mut stream).expect("runtime must answer") {
                ServerMsg::RuntimeReady { room_refused, .. } => return (stream, room_refused),
                other => panic!("expected RuntimeReady, got {other:?}"),
            }
        }
        assert!(Instant::now() < deadline, "runtime must become ready");
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_refusal(window: &mut UnixStream) -> String {
    let deadline = Instant::now() + NOTICE_WITHIN;
    while Instant::now() < deadline {
        if let ServerMsg::RuntimeReady {
            room_refused: Some(reason),
            ..
        } = codec::decode(window).expect("the window link must stay open")
        {
            return reason;
        }
    }
    panic!("the window got no refusal notice");
}

struct Runtime(Child);

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
