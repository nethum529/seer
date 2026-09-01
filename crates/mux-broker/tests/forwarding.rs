#![cfg(target_os = "linux")]

use std::fs;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mux_core::proto::{ClientMsg, ServerMsg, codec};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[test]
fn forwards_to_a_lazy_runtime_and_preserves_its_tree() {
    let temporary = TestFiles::new();
    let address = unused_address();
    temporary.write_config(address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);
    let mut first = connect_when_ready(address);

    assert!(!temporary.pid_file.is_file());
    send_hello(&mut first);
    assert_welcome(read_message(&mut first));
    codec::encode(&mut first, &ClientMsg::CreateTab).expect("CreateTab must encode");
    assert!(wait_for_tree_with_tab(&mut first));
    assert!(wait_for_cells(&mut first));
    let runtime_pid = temporary.runtime_pid();
    temporary.assert_runtime_arguments();
    temporary.assert_socket_directory();

    drop(first);

    let mut second = connect_when_ready(address);
    send_hello(&mut second);
    assert_welcome(read_message(&mut second));
    assert_tree_has_one_tab(read_message(&mut second));
    assert_eq!(temporary.runtime_pid(), runtime_pid);

    drop(second);
    temporary.terminate_runtime(runtime_pid);
}

fn send_hello(stream: &mut TcpStream) {
    codec::encode(
        stream,
        &ClientMsg::Hello {
            user: "alice".into(),
            token: "alice-secret".into(),
        },
    )
    .expect("Hello must encode");
}

fn assert_welcome(message: ServerMsg) {
    match message {
        ServerMsg::Welcome { user, tree } => {
            assert_eq!(user, "alice");
            assert!(tree.workspaces.is_empty());
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
}

fn wait_for_tree_with_tab(stream: &mut TcpStream) -> bool {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if let ServerMsg::Tree { tree } = read_message(stream)
            && tree
                .workspaces
                .first()
                .is_some_and(|workspace| workspace.tabs.len() == 1)
        {
            return true;
        }
    }
    false
}

fn wait_for_cells(stream: &mut TcpStream) -> bool {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(read_message(stream), ServerMsg::Cells { .. }) {
            return true;
        }
    }
    false
}

fn assert_tree_has_one_tab(message: ServerMsg) {
    match message {
        ServerMsg::Tree { tree } => {
            assert_eq!(tree.workspaces.len(), 1);
            assert_eq!(tree.workspaces[0].tabs.len(), 1);
        }
        other => panic!("expected Tree, got {other:?}"),
    }
}

fn read_message(stream: &mut TcpStream) -> ServerMsg {
    stream
        .set_read_timeout(Some(WAIT_TIMEOUT))
        .expect("read timeout must set");
    codec::decode(stream).expect("server message must decode")
}

fn unused_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port probe must bind");
    listener
        .local_addr()
        .expect("port probe must have an address")
}

fn connect_when_ready(address: SocketAddr) -> TcpStream {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let mut last_error = None;
    while Instant::now() < deadline {
        match TcpStream::connect(address) {
            Ok(stream) => return stream,
            Err(error) => last_error = Some(error),
        }
        thread::sleep(POLL_INTERVAL);
    }
    panic!("broker did not listen: {last_error:?}");
}

struct TestFiles {
    root: PathBuf,
    config: PathBuf,
    wrapper: PathBuf,
    pid_file: PathBuf,
    arguments_file: PathBuf,
    xdg_runtime_dir: PathBuf,
}

impl TestFiles {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mux-broker-forwarding-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("temporary directory must be created");
        Self {
            config: root.join("broker.toml"),
            wrapper: root.join("runtime-wrapper"),
            pid_file: root.join("runtime.pid"),
            arguments_file: root.join("runtime.args"),
            xdg_runtime_dir: root.join("run"),
            root,
        }
    }

    fn write_config(&self, address: SocketAddr) {
        let contents = format!(
            "listen = \"{address}\"\nshell = \"sh\"\n\n[[users]]\nuser = \"alice\"\ntoken = \"alice-secret\"\n"
        );
        fs::write(&self.config, contents).expect("broker config must write");
    }

    fn write_runtime_wrapper(&self) {
        let script = "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$MUX_TEST_PID_FILE\"\nprintf '%s\\n%s\\n%s\\n' \"$1\" \"$2\" \"$3\" > \"$MUX_TEST_ARGS_FILE\"\nexec \"$MUX_TEST_RUNTIME_BIN\" \"$@\"\n";
        fs::write(&self.wrapper, script).expect("runtime wrapper must write");
        fs::set_permissions(&self.wrapper, fs::Permissions::from_mode(0o700))
            .expect("runtime wrapper mode must set");
    }

    fn start_broker(&self) -> Child {
        Command::new(env!("CARGO_BIN_EXE_mux-broker"))
            .arg(&self.config)
            .env("XDG_RUNTIME_DIR", &self.xdg_runtime_dir)
            .env("MUX_RUNTIME_BIN", &self.wrapper)
            .env("MUX_TEST_RUNTIME_BIN", runtime_binary())
            .env("MUX_TEST_PID_FILE", &self.pid_file)
            .env("MUX_TEST_ARGS_FILE", &self.arguments_file)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("broker must start")
    }

    fn runtime_pid(&self) -> u32 {
        assert!(wait_for_file(&self.pid_file));
        fs::read_to_string(&self.pid_file)
            .expect("runtime PID must be readable")
            .trim()
            .parse()
            .expect("runtime PID must be valid")
    }

    fn assert_runtime_arguments(&self) {
        assert!(wait_for_file(&self.arguments_file));
        let arguments =
            fs::read_to_string(&self.arguments_file).expect("runtime arguments must be readable");
        let expected_socket = self.xdg_runtime_dir.join("mux/alice.sock");
        assert_eq!(
            arguments.lines().collect::<Vec<_>>(),
            [expected_socket.to_string_lossy().as_ref(), "alice", "sh"]
        );
    }

    fn assert_socket_directory(&self) {
        let directory = self.xdg_runtime_dir.join("mux");
        let mode = fs::metadata(directory)
            .expect("socket directory metadata must load")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    fn terminate_runtime(&self, pid: u32) {
        terminate_process(pid);
        fs::remove_file(&self.pid_file).expect("runtime PID file must be removed");
    }
}

impl Drop for TestFiles {
    fn drop(&mut self) {
        if let Ok(contents) = fs::read_to_string(&self.pid_file)
            && let Ok(pid) = contents.trim().parse()
        {
            try_terminate_process(pid);
        }
        if let Err(error) = fs::remove_dir_all(&self.root)
            && error.kind() != io::ErrorKind::NotFound
        {
            panic!("temporary directory must be removed: {error}");
        }
    }
}

fn runtime_binary() -> PathBuf {
    let sibling = Path::new(env!("CARGO_BIN_EXE_mux-broker"))
        .parent()
        .expect("broker binary must have a parent")
        .join("mux-runtime");
    if sibling.is_file() {
        return sibling;
    }

    runtime_from_parent_working_directories().expect("workspace runtime binary must exist")
}

fn runtime_from_parent_working_directories() -> Option<PathBuf> {
    let mut pid = std::process::id();
    for _ in 0..8 {
        pid = parent_pid(pid)?;
        let working_directory = fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
        let candidate = working_directory.join("target/debug/mux-runtime");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn parent_pid(pid: u32) -> Option<u32> {
    fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("PPid:\t"))?
        .parse()
        .ok()
}

fn wait_for_file(path: &Path) -> bool {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if path.is_file() {
            return true;
        }
        thread::sleep(POLL_INTERVAL);
    }
    false
}

struct ProcessGuard(Child);

impl ProcessGuard {
    fn new(child: Child) -> Self {
        Self(child)
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        terminate_child(&mut self.0);
    }
}

fn terminate_child(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn terminate_process(pid: u32) {
    send_signal(pid, "TERM");
    if wait_for_process_stop(pid) {
        return;
    }
    send_signal(pid, "KILL");
    assert!(wait_for_process_stop(pid));
}

fn try_terminate_process(pid: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    if !wait_for_process_stop(pid) {
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
}

fn send_signal(pid: u32, signal: &str) {
    let status = Command::new("kill")
        .args([format!("-{signal}"), pid.to_string()])
        .status()
        .expect("kill command must run");
    assert!(status.success());
}

fn wait_for_process_stop(pid: u32) -> bool {
    let status_path = PathBuf::from(format!("/proc/{pid}/status"));
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        match fs::read_to_string(&status_path) {
            Ok(contents) if contents.lines().any(|line| line.starts_with("State:\tZ")) => {
                return true;
            }
            Ok(_) => thread::sleep(POLL_INTERVAL),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return true,
            Err(_) => return false,
        }
    }
    false
}
