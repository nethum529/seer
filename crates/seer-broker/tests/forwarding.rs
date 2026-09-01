#![cfg(target_os = "linux")]

use std::fs;
use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

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

    assert!(!temporary.pid_file("alice").is_file());
    send_hello(&mut first, "alice", "alice-secret");
    assert_welcome(read_message(&mut first), "alice");
    codec::encode(&mut first, &ClientMsg::CreateTab).expect("CreateTab must encode");
    wait_for_tree_with_tab(&mut first);
    assert!(wait_for_cells(&mut first));
    let runtime_pid = temporary.runtime_pid("alice");
    temporary.assert_runtime_arguments("alice");
    temporary.assert_socket_directory();

    drop(first);

    let mut second = connect_when_ready(address);
    send_hello(&mut second, "alice", "alice-secret");
    assert_welcome(read_message(&mut second), "alice");
    assert_tree_has_one_tab(read_message(&mut second));
    assert_eq!(temporary.runtime_pid("alice"), runtime_pid);

    drop(second);
    temporary.terminate_runtime("alice");
}

#[test]
fn routes_peek_and_restores_the_owners_runtime() {
    let temporary = TestFiles::new();
    let address = unused_address();
    temporary.write_config(address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    assert_welcome(read_message(&mut alice), "alice");
    send(&mut alice, &ClientMsg::CreateTab);
    let alice_tree = wait_for_tree_with_tab(&mut alice);
    assert!(wait_for_cells(&mut alice));
    let workspace = alice_tree.workspaces[0].id.clone();
    let pane = alice_tree.workspaces[0].tabs[0].panes[0].id.clone();

    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    assert_welcome(read_message(&mut bob), "bob");
    assert_tree_is_empty(read_message(&mut bob));

    send(
        &mut bob,
        &ClientMsg::Peek {
            user: "charlie".into(),
            workspace: workspace.clone(),
        },
    );
    send(&mut bob, &ClientMsg::Resize { cols: 90, rows: 30 });
    assert_tree_is_empty(read_message(&mut bob));
    assert!(!temporary.pid_file("charlie").is_file());

    send(
        &mut bob,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace,
        },
    );
    assert_eq!(wait_for_tree_with_tab(&mut bob), alice_tree);
    send_input(&mut alice, &pane, "printf 'alice-before\\n'\n");
    assert!(wait_for_cells_containing(&mut alice, "alice-before").contains("alice-before"));
    assert!(wait_for_cells_containing(&mut bob, "alice-before").contains("alice-before"));

    send_input(&mut bob, &pane, "printf 'bob-write\\n'\n");
    send_input(&mut alice, &pane, "printf 'alice-after\\n'\n");
    let alice_cells = wait_for_cells_containing(&mut alice, "alice-after");
    let bob_cells = wait_for_cells_containing(&mut bob, "alice-after");
    assert!(!alice_cells.contains("bob-write"));
    assert!(!bob_cells.contains("bob-write"));

    send(&mut bob, &ClientMsg::StopPeek);
    assert_tree_is_empty(read_message(&mut bob));
    temporary.assert_log_contains("broker dropped Peek for unknown user: charlie");
    temporary.assert_log_contains("broker dropped Input while user bob peeks");
    temporary.assert_log_excludes("runtime dropped read-only message");

    send(
        &mut bob,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
        },
    );
    wait_for_tree_with_tab(&mut bob);
    wait_for_tree_with_tab(&mut bob);
    temporary.terminate_runtime("alice");
    assert_tree_is_empty(read_message(&mut bob));
    let mut byte = [0];
    assert_eq!(alice.read(&mut byte).expect("Alice must disconnect"), 0);

    drop(alice);
    drop(bob);
    temporary.terminate_runtime("bob");
}

fn send_hello(stream: &mut TcpStream, user: &str, token: &str) {
    codec::encode(
        stream,
        &ClientMsg::Hello {
            user_id: user.into(),
            credential: token.into(),
        },
    )
    .expect("Hello must encode");
}

fn send(stream: &mut TcpStream, message: &ClientMsg) {
    codec::encode(stream, message).expect("client message must encode");
}

fn send_input(stream: &mut TcpStream, pane: &str, input: &str) {
    send(
        stream,
        &ClientMsg::Input {
            pane: pane.into(),
            bytes: input.as_bytes().into(),
        },
    );
}

fn assert_welcome(message: ServerMsg, expected_user: &str) {
    match message {
        ServerMsg::Welcome {
            user_id,
            name,
            client_id,
            tree,
        } => {
            assert_eq!(user_id, expected_user);
            assert_eq!(name, expected_user);
            assert!(client_id.is_empty());
            assert!(tree.workspaces.is_empty());
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
}

fn wait_for_tree_with_tab(stream: &mut TcpStream) -> seer_core::Tree {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if let ServerMsg::Tree { tree } = read_message(stream)
            && tree
                .workspaces
                .first()
                .is_some_and(|workspace| workspace.tabs.len() == 1)
        {
            return tree;
        }
    }
    panic!("Tree with a tab was not received");
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

fn assert_tree_is_empty(message: ServerMsg) {
    match message {
        ServerMsg::Tree { tree } => assert!(tree.workspaces.is_empty()),
        other => panic!("expected Tree, got {other:?}"),
    }
}

fn wait_for_cells_containing(stream: &mut TcpStream, expected: &str) -> String {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
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
    panic!("Cells did not contain {expected}");
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
    broker_log: PathBuf,
    xdg_runtime_dir: PathBuf,
}

impl TestFiles {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "seer-broker-forwarding-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("temporary directory must be created");
        Self {
            config: root.join("broker.toml"),
            wrapper: root.join("runtime-wrapper"),
            broker_log: root.join("broker.log"),
            xdg_runtime_dir: root.join("run"),
            root,
        }
    }

    fn write_config(&self, address: SocketAddr) {
        let contents = format!(
            "listen = \"{address}\"\nshell = \"sh\"\n\n[[users]]\nuser = \"alice\"\ntoken = \"alice-secret\"\n\n[[users]]\nuser = \"bob\"\ntoken = \"bob-secret\"\n"
        );
        fs::write(&self.config, contents).expect("broker config must write");
    }

    fn write_runtime_wrapper(&self) {
        let script = "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$SEER_TEST_FILES/$2.pid\"\nprintf '%s\\n%s\\n%s\\n' \"$1\" \"$2\" \"$3\" > \"$SEER_TEST_FILES/$2.args\"\nexec \"$SEER_TEST_RUNTIME_BIN\" \"$@\"\n";
        fs::write(&self.wrapper, script).expect("runtime wrapper must write");
        fs::set_permissions(&self.wrapper, fs::Permissions::from_mode(0o700))
            .expect("runtime wrapper mode must set");
    }

    fn start_broker(&self) -> Child {
        let log = fs::File::create(&self.broker_log).expect("broker log must open");
        Command::new(env!("CARGO_BIN_EXE_seer-broker"))
            .arg(&self.config)
            .env("XDG_RUNTIME_DIR", &self.xdg_runtime_dir)
            .env("SEER_RUNTIME_BIN", &self.wrapper)
            .env("SEER_TEST_RUNTIME_BIN", runtime_binary())
            .env("SEER_TEST_FILES", &self.root)
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .expect("broker must start")
    }

    fn pid_file(&self, user: &str) -> PathBuf {
        self.root.join(format!("{user}.pid"))
    }

    fn runtime_pid(&self, user: &str) -> u32 {
        let pid_file = self.pid_file(user);
        assert!(wait_for_file(&pid_file));
        fs::read_to_string(pid_file)
            .expect("runtime PID must be readable")
            .trim()
            .parse()
            .expect("runtime PID must be valid")
    }

    fn assert_runtime_arguments(&self, user: &str) {
        let arguments_file = self.root.join(format!("{user}.args"));
        assert!(wait_for_file(&arguments_file));
        let arguments = fs::read_to_string(arguments_file).expect("runtime arguments must read");
        let expected_socket = self.xdg_runtime_dir.join(format!("seer/{user}.sock"));
        assert_eq!(
            arguments.lines().collect::<Vec<_>>(),
            [expected_socket.to_string_lossy().as_ref(), user, "sh"]
        );
    }

    fn assert_socket_directory(&self) {
        let directory = self.xdg_runtime_dir.join("seer");
        let mode = fs::metadata(directory)
            .expect("socket directory metadata must load")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    fn terminate_runtime(&self, user: &str) {
        let pid = self.runtime_pid(user);
        terminate_process(pid);
        fs::remove_file(self.pid_file(user)).expect("runtime PID file must be removed");
    }

    fn assert_log_contains(&self, expected: &str) {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            if fs::read_to_string(&self.broker_log)
                .is_ok_and(|contents| contents.contains(expected))
            {
                return;
            }
            thread::sleep(POLL_INTERVAL);
        }
        panic!("broker log did not contain {expected}");
    }

    fn assert_log_excludes(&self, unexpected: &str) {
        let contents = fs::read_to_string(&self.broker_log).expect("broker log must read");
        assert!(!contents.contains(unexpected));
    }
}

impl Drop for TestFiles {
    fn drop(&mut self) {
        for user in ["alice", "bob"] {
            if let Ok(contents) = fs::read_to_string(self.pid_file(user))
                && let Ok(pid) = contents.trim().parse()
            {
                try_terminate_process(pid);
            }
        }
        if let Err(error) = fs::remove_dir_all(&self.root)
            && error.kind() != io::ErrorKind::NotFound
        {
            panic!("temporary directory must be removed: {error}");
        }
    }
}

fn runtime_binary() -> PathBuf {
    let sibling = Path::new(env!("CARGO_BIN_EXE_seer-broker"))
        .parent()
        .expect("broker binary must have a parent")
        .join("seer-runtime");
    if sibling.is_file() {
        return sibling;
    }

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest_directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(cargo)
        .args(["build", "-p", "seer-runtime", "--bin", "seer-runtime"])
        .current_dir(manifest_directory)
        .spawn()
        .expect("runtime binary must build");
    let status = wait_for_child(&mut child, Duration::from_secs(60)).unwrap_or_else(|| {
        let _ = child.kill();
        assert!(wait_for_child(&mut child, Duration::from_secs(2)).is_some());
        panic!("runtime binary build timed out");
    });
    assert!(status.success(), "runtime binary must build");

    manifest_directory.join("../../target/debug/seer-runtime")
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
    assert!(wait_for_child(child, Duration::from_secs(2)).is_some());
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        thread::sleep(POLL_INTERVAL);
    }
    None
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
    let _ = run_kill(pid, "TERM");
    if !wait_for_process_stop(pid) {
        let _ = run_kill(pid, "KILL");
    }
}

fn send_signal(pid: u32, signal: &str) {
    let status = run_kill(pid, signal).expect("kill command must finish");
    assert!(status.success());
}

fn run_kill(pid: u32, signal: &str) -> Option<ExitStatus> {
    let mut child = Command::new("kill")
        .args([format!("-{signal}"), pid.to_string()])
        .spawn()
        .expect("kill command must run");
    let status = wait_for_child(&mut child, Duration::from_secs(2));
    if status.is_none() {
        let _ = child.kill();
        assert!(wait_for_child(&mut child, Duration::from_secs(2)).is_some());
    }
    status
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
