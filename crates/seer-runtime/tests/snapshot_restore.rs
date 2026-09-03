//! End-to-end snapshot persistence for the runtime binary.
//!
//! The runtime persists a versioned session snapshot when it is started with
//! the SEER_SNAPSHOT_DIR environment variable. This file covers one scenario:
//! live detach and reattach keep the original shell processes, a cold restart
//! restores the same topology with replacement shells, and a corrupt or
//! unsupported snapshot file makes the runtime start a safe default session.

#![cfg(target_os = "linux")]

use std::fs;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{SplitDirection, Tree};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(10);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
const SNAPSHOT_FILE: &str = "session.json";
const SNAPSHOT_DIR_VAR: &str = "SEER_SNAPSHOT_DIR";

static RUNTIME_BINARY: OnceLock<PathBuf> = OnceLock::new();

#[test]
fn cold_restart_restores_topology_and_corrupt_snapshots_start_safely() {
    let temporary = TemporaryDirectory::new();
    let state = temporary.path.join("state");
    fs::create_dir(&state).expect("state directory must be created");
    let snapshot_path = state.join(SNAPSHOT_FILE);
    let socket_path = temporary.path.join("runtime.sock");

    let mut runtime = spawn_runtime(&socket_path, &state);
    let mut owner = connect_with_timeout(&socket_path);
    let initial = read_tree(&mut owner);
    assert_eq!(initial.workspaces[0].tabs.len(), 1);
    assert_eq!(initial.workspaces[0].tabs[0].panes.len(), 1);
    assert!(snapshot_path.exists(), "first shell must be saved");

    let workspace = initial.workspaces[0].id.clone();
    let tab = initial.workspaces[0].tabs[0].id.clone();
    send(
        &mut owner,
        &ClientMsg::Resize {
            workspace: workspace.clone(),
            tab: tab.clone(),
            cols: 81,
            rows: 25,
        },
    );
    let _resized = read_until_tree(&mut owner);
    send(
        &mut owner,
        &ClientMsg::SplitPane {
            workspace: workspace.clone(),
            tab: tab.clone(),
            direction: SplitDirection::Right,
        },
    );
    let split_tree = read_until_tree(&mut owner);
    assert_eq!(split_tree.workspaces[0].tabs[0].panes.len(), 2);
    let first_pane = split_tree.workspaces[0].tabs[0].panes[0].id.clone();
    let second_pane = split_tree.workspaces[0].tabs[0].panes[1].id.clone();
    assert_ne!(first_pane, second_pane);

    let first_pid =
        marked_pid(&mut owner, &workspace, &tab, &first_pane).expect("first shell PID must be read");
    let second_pid = marked_pid(&mut owner, &workspace, &tab, &second_pane)
        .expect("second shell PID must be read");
    assert_ne!(first_pid, second_pid, "each pane must have its own shell");

    drop(owner);
    let mut reattached = connect_with_timeout(&socket_path);
    let live_tree = read_tree(&mut reattached);
    assert_eq!(live_tree, split_tree, "live reattach must keep the tree");
    let live_pid =
        marked_pid(&mut reattached, &workspace, &tab, &first_pane).expect("live PID must be read");
    assert_eq!(
        live_pid, first_pid,
        "live detach and reattach must keep the original shell"
    );
    drop(reattached);
    runtime.stop();

    let mut restarted = spawn_runtime(&socket_path, &state);
    let mut restored_client = connect_with_timeout(&socket_path);
    let restored = read_tree(&mut restored_client);
    assert_eq!(
        restored, split_tree,
        "cold restart must restore the saved topology without duplicates"
    );
    let restored_first = marked_pid(&mut restored_client, &workspace, &tab, &first_pane)
        .expect("restored first shell PID must be read");
    let restored_second = marked_pid(&mut restored_client, &workspace, &tab, &second_pane)
        .expect("restored second shell PID must be read");
    assert_ne!(restored_first, first_pid, "restored shell must be a fresh process");
    assert_ne!(restored_second, second_pid, "restored shell must be a fresh process");
    drop(restored_client);
    restarted.stop();

    let valid_text =
        fs::read_to_string(&snapshot_path).expect("valid snapshot must be readable");
    fs::write(&snapshot_path, b"{\"version\":1,\"revision\":7,\"tree\":")
        .expect("truncated snapshot must write");
    let mut corrupt_runtime = spawn_runtime(&socket_path, &state);
    let mut corrupt_client = connect_with_timeout(&socket_path);
    let safe = read_tree(&mut corrupt_client);
    assert_eq!(
        safe.workspaces[0].tabs.len(),
        1,
        "corrupt snapshot must start a safe default session"
    );
    assert_eq!(safe.workspaces[0].tabs[0].panes.len(), 1);
    assert!(wait_for_cells(&mut corrupt_client), "default shell must run");
    drop(corrupt_client);
    corrupt_runtime.stop();

    fs::write(
        &snapshot_path,
        valid_text.replacen("\"version\":1", "\"version\":99", 1),
    )
    .expect("unsupported snapshot must write");
    let mut unsupported_runtime = spawn_runtime(&socket_path, &state);
    let mut unsupported_client = connect_with_timeout(&socket_path);
    let safe_again = read_tree(&mut unsupported_client);
    assert_eq!(
        safe_again.workspaces[0].tabs.len(),
        1,
        "unsupported snapshot version must start a safe default session"
    );
    assert_eq!(safe_again.workspaces[0].tabs[0].panes.len(), 1);
    assert!(wait_for_cells(&mut unsupported_client), "default shell must run");
    drop(unsupported_client);
    unsupported_runtime.stop();
}

fn spawn_runtime(socket_path: &Path, state_dir: &Path) -> RuntimeProcess {
    let child = Command::new(runtime_binary())
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .env(SNAPSHOT_DIR_VAR, state_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    RuntimeProcess::new(child)
}

fn runtime_binary() -> &'static Path {
    RUNTIME_BINARY.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["build", "-p", "seer", "--bin", "seer-runtime"])
            .current_dir(manifest)
            .status()
            .expect("runtime binary must build");
        assert!(status.success(), "runtime binary must build");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| manifest.join("../../target"));
        target.join("debug/seer-runtime")
    })
}

fn connect_with_timeout(path: &Path) -> UnixStream {
    let mut last_error;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match UnixStream::connect(path) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(MESSAGE_TIMEOUT))
                    .expect("read timeout must set");
                return stream;
            }
            Err(error) => last_error = Some(error),
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(RETRY_INTERVAL);
    }
    panic!("runtime did not listen: {last_error:?}");
}

fn send(stream: &mut UnixStream, message: &ClientMsg) {
    codec::encode(stream, message).expect("client message must encode");
}

fn read_message(stream: &mut UnixStream) -> ServerMsg {
    codec::decode(stream).expect("server message must decode")
}

fn tree(message: ServerMsg) -> Tree {
    match message {
        ServerMsg::Tree { tree } => tree,
        other => panic!("expected Tree, got {other:?}"),
    }
}

fn read_tree(stream: &mut UnixStream) -> Tree {
    tree(read_message(stream))
}

fn read_until_tree(stream: &mut UnixStream) -> Tree {
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < MESSAGE_TIMEOUT, "Tree was not received");
        if let ServerMsg::Tree { tree } = read_message(stream) {
            return tree;
        }
    }
}

fn wait_for_cells(stream: &mut UnixStream) -> bool {
    let start = Instant::now();
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "Cells were not received"
        );
        if matches!(read_message(stream), ServerMsg::Cells { .. }) {
            return true;
        }
    }
}

fn marked_pid(stream: &mut UnixStream, workspace: &str, tab: &str, pane: &str) -> Option<u32> {
    send(
        stream,
        &ClientMsg::Input {
            workspace: workspace.into(),
            tab: tab.into(),
            pane: pane.into(),
            bytes: b"printf 'MARK:%s\\n' $$\n".to_vec(),
        },
    );
    let start = Instant::now();
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "marked shell PID was not received for {pane}"
        );
        match read_message(stream) {
            ServerMsg::Cells {
                pane: cell_pane,
                rows,
            } if cell_pane == pane => {
                let text = rows
                    .iter()
                    .flatten()
                    .map(|cell| cell.character)
                    .collect::<String>();
                if let Some(pid) = pid_after_mark(&text) {
                    return Some(pid);
                }
            }
            _ => {}
        }
    }
}

fn pid_after_mark(text: &str) -> Option<u32> {
    let mut search_from = 0;
    while let Some(mark) = text[search_from..].find("MARK:") {
        let digits_start = search_from + mark + "MARK:".len();
        let digits = text[digits_start..]
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
        search_from = digits_start + 1;
    }
    None
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
            std::env::temp_dir().join(format!("seer-snapshot-{}-{timestamp}", std::process::id()));
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

    fn stop(&mut self) {
        let mut child = self.0.take().expect("runtime process must exist");
        let _ = child.kill();
        wait_until_exit(&mut child, "runtime did not stop after kill");
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
