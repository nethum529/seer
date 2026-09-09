use std::fs;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TerminalInput};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
pub const RETRY_INTERVAL: Duration = Duration::from_millis(10);
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(2);
// Full workspace runs can starve runtime startup and PTY polling.
pub const MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);

static RUNTIME_BINARY: OnceLock<PathBuf> = OnceLock::new();
static NEXT_TEMPORARY_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

pub fn runtime_command() -> Command {
    let mut command = Command::new(runtime_binary());
    command.stdin(Stdio::piped());
    command
}

pub fn runtime_binary() -> &'static Path {
    RUNTIME_BINARY.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let target = target_directory(
            manifest,
            std::env::var_os("CARGO_TARGET_DIR")
                .as_deref()
                .map(Path::new),
        );
        let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["build", "-p", "seer", "--bin", "seer-runtime"])
            .current_dir(manifest)
            .env("CARGO_TARGET_DIR", &target)
            .status()
            .expect("runtime binary must build");
        assert!(status.success(), "runtime binary must build");
        target.join("debug/seer-runtime")
    })
}

fn target_directory(manifest: &Path, target_var: Option<&Path>) -> PathBuf {
    let workspace_root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("crate manifest must sit inside the workspace root");
    match target_var {
        Some(target) if target.is_absolute() => target.to_owned(),
        Some(target) => workspace_root.join(target),
        None => workspace_root.join("target"),
    }
}

pub fn connect_when_ready(path: &Path) -> UnixStream {
    let mut last_error;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match UnixStream::connect(path) {
            Ok(mut stream) => {
                codec::encode(&mut stream, &ClientMsg::AttachRuntime)
                    .expect("runtime attach must encode");
                match codec::decode::<_, ServerMsg>(&mut stream).expect("runtime ready must decode")
                {
                    ServerMsg::RuntimeReady { generation } => {
                        assert!(!generation.is_empty());
                    }
                    other => panic!("expected RuntimeReady, got {other:?}"),
                }
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

pub fn connect_with_timeout(path: &Path) -> UnixStream {
    let stream = connect_when_ready(path);
    stream
        .set_read_timeout(Some(MESSAGE_TIMEOUT))
        .expect("read timeout must set");
    stream
}

pub fn send(stream: &mut UnixStream, message: &ClientMsg) {
    codec::encode(stream, message).expect("client message must encode");
}

pub fn send_input(stream: &mut UnixStream, pane: &str, input: &str) {
    send(
        stream,
        &ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: pane.into(),
            input: TerminalInput::new(InputEvent::Text(input.to_owned())),
        },
    );
}

pub fn read_message(stream: &mut UnixStream) -> ServerMsg {
    codec::decode(stream).expect("server message must decode")
}

pub fn tree(message: ServerMsg) -> seer_core::Tree {
    match message {
        ServerMsg::Tree { tree } => tree,
        other => panic!("expected Tree, got {other:?}"),
    }
}

pub fn read_until_tree(stream: &mut UnixStream) -> seer_core::Tree {
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < MESSAGE_TIMEOUT, "Tree was not received");
        if let ServerMsg::Tree { tree } = read_message(stream) {
            return tree;
        }
    }
}

pub fn wait_for_cells(stream: &mut UnixStream) -> bool {
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < MESSAGE_TIMEOUT, "Cells were not received");
        if matches!(read_message(stream), ServerMsg::Cells { .. }) {
            return true;
        }
    }
}

pub struct TemporaryDirectory {
    pub path: PathBuf,
}

impl TemporaryDirectory {
    #[must_use]
    pub fn new() -> Self {
        let counter = NEXT_TEMPORARY_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "seer-runtime-{}-{timestamp}-{counter}",
            std::process::id()
        ));
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

pub struct RuntimeProcess(Option<Child>);

impl RuntimeProcess {
    #[must_use]
    pub fn new(child: Child) -> Self {
        Self(Some(child))
    }

    pub fn stop(&mut self) -> Output {
        let child = self.0.as_mut().expect("runtime process must exist");
        signal(child.id(), libc::SIGTERM);
        self.wait_for_exit()
    }

    pub fn is_running(&mut self) -> bool {
        self.0
            .as_mut()
            .expect("runtime process must exist")
            .try_wait()
            .expect("runtime status must be available")
            .is_none()
    }

    pub fn close_parent_pipe(&mut self) {
        let child = self.0.as_mut().expect("runtime process must exist");
        drop(child.stdin.take());
    }

    pub fn wait_for_exit(&mut self) -> Output {
        let mut child = self.0.take().expect("runtime process must exist");
        wait_until_exit(&mut child, "runtime did not exit");
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

pub fn signal(pid: u32, number: i32) {
    let pid = i32::try_from(pid).expect("process ID must fit");
    // SAFETY: kill takes a process ID and a signal number and touches no memory.
    unsafe {
        libc::kill(pid, number);
    }
}

pub fn wait_until_exit(child: &mut Child, timeout_message: &str) {
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
