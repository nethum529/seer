#![cfg(target_os = "linux")]

use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mux_core::proto::{ClientMsg, ServerMsg, codec};

const CONNECT_ATTEMPTS: usize = 100;
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(2);
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn serves_cells_and_preserves_the_tree_after_disconnect() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let stale_listener = UnixListener::bind(&socket_path).expect("stale socket must bind");
    let stale_inode = fs::metadata(&socket_path)
        .expect("stale socket metadata must exist")
        .ino();
    drop(stale_listener);

    let runtime = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let _runtime = RuntimeProcess::new(runtime);
    let mut stream = connect_after_replacement(&socket_path, stale_inode);
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("duplicate runtime must start");
    let duplicate = wait_for_output(duplicate);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already in use"));
}

#[test]
fn broadcasts_to_concurrent_connections_and_blocks_peek_input() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("runtime must start");
    let mut runtime = RuntimeProcess::new(runtime);

    let mut owner = connect_with_timeout(&socket_path);
    assert!(tree(read_message(&mut owner)).workspaces.is_empty());
    send(&mut owner, &ClientMsg::CreateTab);
    let created = tree(read_message(&mut owner));
    let pane = created.workspaces[0].tabs[0].panes[0].id.clone();

    let mut viewer = connect_with_timeout(&socket_path);
    let viewer_tree = tree(read_message(&mut viewer));
    assert_eq!(viewer_tree, created);

    send_input(&mut owner, &pane, "printf 'owner-one\\n'\n");
    assert_cells_contain(&mut owner, "owner-one");
    assert_cells_contain(&mut viewer, "owner-one");

    send(
        &mut viewer,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
        },
    );
    assert_eq!(read_until_tree(&mut viewer), created);
    send_input(&mut viewer, &pane, "printf 'viewer-input\\n'\n");
    send(
        &mut viewer,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
        },
    );
    assert_eq!(read_until_tree(&mut viewer), created);

    send_input(&mut owner, &pane, "printf 'owner-two\\n'\n");
    let owner_cells = wait_for_cells_containing(&mut owner, "owner-two");
    let viewer_cells = wait_for_cells_containing(&mut viewer, "owner-two");
    assert!(!owner_cells.contains("viewer-input"));
    assert!(!viewer_cells.contains("viewer-input"));

    send(&mut viewer, &ClientMsg::StopPeek);
    wait_for_close(&mut viewer);
    send_input(&mut owner, &pane, "printf 'owner-three\\n'\n");
    assert_cells_contain(&mut owner, "owner-three");

    drop(owner);
    let output = runtime.stop();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.contains("runtime dropped read-only message"))
            .count(),
        1
    );
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
        let child = runtime_command()
            .args(*arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("runtime must start");
        let output = wait_for_output(child);
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

fn connect_after_replacement(path: &Path, stale_inode: u64) -> UnixStream {
    let mut last_error = None;
    for _ in 0..CONNECT_ATTEMPTS {
        match fs::metadata(path) {
            Ok(metadata) if metadata.ino() != stale_inode => match UnixStream::connect(path) {
                Ok(stream) => return stream,
                Err(error) => last_error = Some(error),
            },
            Ok(_) => {}
            Err(error) => last_error = Some(error),
        }
        thread::sleep(RETRY_INTERVAL);
    }
    panic!("runtime did not replace stale socket: {last_error:?}");
}

fn connect_with_timeout(path: &Path) -> UnixStream {
    let stream = connect_when_ready(path);
    stream
        .set_read_timeout(Some(MESSAGE_TIMEOUT))
        .expect("read timeout must set");
    stream
}

fn send(stream: &mut UnixStream, message: &ClientMsg) {
    codec::encode(stream, message).expect("client message must encode");
}

fn send_input(stream: &mut UnixStream, pane: &str, input: &str) {
    send(
        stream,
        &ClientMsg::Input {
            pane: pane.into(),
            bytes: input.as_bytes().into(),
        },
    );
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

fn read_until_tree(stream: &mut UnixStream) -> mux_core::Tree {
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < MESSAGE_TIMEOUT, "Tree was not received");
        if let ServerMsg::Tree { tree } = read_message(stream) {
            return tree;
        }
    }
}

fn assert_cells_contain(stream: &mut UnixStream, expected: &str) {
    let cells = wait_for_cells_containing(stream, expected);
    assert!(cells.contains(expected));
}

fn wait_for_cells_containing(stream: &mut UnixStream, expected: &str) -> String {
    let start = Instant::now();
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "Cells did not contain {expected}"
        );
        if let ServerMsg::Cells { rows, .. } = read_message(stream) {
            let text = rows
                .iter()
                .flatten()
                .map(|cell| cell.character)
                .collect::<String>();
            if text.contains(expected) {
                return text;
            }
        }
    }
}

fn wait_for_cells(stream: &mut UnixStream) -> bool {
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < MESSAGE_TIMEOUT, "Cells were not received");
        if matches!(read_message(stream), ServerMsg::Cells { .. }) {
            return true;
        }
    }
}

fn wait_for_close(stream: &mut UnixStream) {
    let start = Instant::now();
    let mut bytes = [0; 1024];
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "connection did not close"
        );
        if stream.read(&mut bytes).expect("connection close must read") == 0 {
            return;
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

fn wait_for_output(mut child: Child) -> Output {
    let start = Instant::now();
    loop {
        if child
            .try_wait()
            .expect("runtime status must be available")
            .is_some()
        {
            return child
                .wait_with_output()
                .expect("runtime output must be available");
        }
        if start.elapsed() >= PROCESS_TIMEOUT {
            let _ = child.kill();
            wait_until_exit(&mut child, "runtime did not stop after timeout");
            panic!("runtime did not exit within 2 seconds");
        }
        thread::sleep(RETRY_INTERVAL);
    }
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

struct RuntimeProcess(Option<Child>);

impl RuntimeProcess {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn stop(&mut self) -> Output {
        let mut child = self.0.take().expect("runtime process must exist");
        let _ = child.kill();
        wait_until_exit(&mut child, "runtime did not stop after kill");
        child
            .wait_with_output()
            .expect("runtime output must be available")
    }
}

impl Drop for RuntimeProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            wait_until_exit(&mut child, "runtime did not stop during cleanup");
        }
    }
}

fn wait_until_exit(child: &mut Child, timeout_message: &str) {
    let start = Instant::now();
    loop {
        if child
            .try_wait()
            .expect("runtime status must be available")
            .is_some()
        {
            return;
        }
        assert!(start.elapsed() < PROCESS_TIMEOUT, "{timeout_message}");
        thread::sleep(RETRY_INTERVAL);
    }
}
