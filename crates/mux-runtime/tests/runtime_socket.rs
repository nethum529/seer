#![cfg(target_os = "linux")]

use std::fs;
use std::io::{self, Read};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mux_core::proto::{ClientMsg, ServerMsg, codec};

const CONNECT_ATTEMPTS: usize = 100;
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn serves_cells_and_preserves_the_tree_after_disconnect() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    UnixListener::bind(&socket_path).expect("stale socket must bind");

    let runtime = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let _runtime = RuntimeProcess(runtime);
    let mut stream = connect_when_ready(&socket_path);
    stream
        .set_read_timeout(Some(MESSAGE_TIMEOUT))
        .expect("read timeout must set");

    assert!(tree(read_message(&mut stream)).workspaces.is_empty());
    codec::encode(&mut stream, &ClientMsg::CreateTab).expect("CreateTab must encode");
    let created_tree = tree(read_message(&mut stream));
    assert_eq!(created_tree.workspaces[0].tabs.len(), 1);
    assert!(wait_for_cells(&mut stream));

    drop(stream);

    let mut reattached = connect_when_ready(&socket_path);
    reattached
        .set_read_timeout(Some(MESSAGE_TIMEOUT))
        .expect("read timeout must set");
    let reattached_tree = tree(read_message(&mut reattached));
    assert_eq!(reattached_tree.workspaces[0].tabs.len(), 1);

    codec::encode(&mut reattached, &ClientMsg::Detach).expect("Detach must encode");
    let mut byte = [0];
    assert_eq!(
        reattached
            .read(&mut byte)
            .expect("runtime must close Detach"),
        0
    );

    let duplicate = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .output()
        .expect("duplicate runtime must run");
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already in use"));
}

#[test]
fn rejects_wrong_argument_counts() {
    let cases: &[&[&str]] = &[
        &[],
        &["socket"],
        &["socket", "alice"],
        &["socket", "alice", "sh", "extra"],
    ];

    for arguments in cases {
        let output = runtime_command()
            .args(*arguments)
            .output()
            .expect("runtime must run");
        assert_usage_error(&output);
    }
}

fn runtime_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mux-runtime"))
}

fn connect_when_ready(path: &Path) -> UnixStream {
    let mut last_error = None;
    for _ in 0..CONNECT_ATTEMPTS {
        match UnixStream::connect(path) {
            Ok(stream) => return stream,
            Err(error) => last_error = Some(error),
        }
        thread::sleep(RETRY_INTERVAL);
    }
    panic!("runtime did not listen: {last_error:?}");
}

fn read_message(stream: &mut UnixStream) -> ServerMsg {
    codec::decode(stream).expect("server message must decode")
}

fn tree(message: ServerMsg) -> mux_core::Tree {
    match message {
        ServerMsg::Tree { tree } => tree,
        other => panic!("expected Tree, got {other:?}"),
    }
}

fn wait_for_cells(stream: &mut UnixStream) -> bool {
    loop {
        if matches!(read_message(stream), ServerMsg::Cells { .. }) {
            return true;
        }
    }
}

fn assert_usage_error(output: &Output) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("usage: mux-runtime <socket-path> <user> <shell>")
    );
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("mux-runtime-{}-{timestamp}", std::process::id()));
        fs::create_dir(&path).expect("temporary directory must be created");
        Self { path }
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            panic!("temporary directory must be removed: {error}");
        }
    }
}

struct RuntimeProcess(Child);

impl Drop for RuntimeProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
