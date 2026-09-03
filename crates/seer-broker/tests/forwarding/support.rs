use std::fs;
use std::io;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::binary;

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
static NEXT_TEMPORARY_DIRECTORY: AtomicUsize = AtomicUsize::new(0);
static BROKER_BINARY: OnceLock<PathBuf> = OnceLock::new();
static RUNTIME_BINARY: OnceLock<PathBuf> = OnceLock::new();

pub struct TestFiles {
    pub root: PathBuf,
    pub state_dir: PathBuf,
    config: PathBuf,
    wrapper: PathBuf,
    broker_log: PathBuf,
    xdg_runtime_dir: PathBuf,
}

impl TestFiles {
    pub fn new() -> Self {
        let counter = NEXT_TEMPORARY_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos()
            % 1_000_000_000;
        let root = PathBuf::from(format!(
            "/tmp/sbf-{}-{counter}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("temporary directory must be created");
        Self {
            config: root.join("broker.toml"),
            state_dir: root.join("state"),
            wrapper: root.join("runtime-wrapper"),
            broker_log: root.join("broker.log"),
            xdg_runtime_dir: root.join("run"),
            root,
        }
    }

    pub fn write_config(&self, address: SocketAddr) {
        let os_user = current_os_user();
        self.write_config_with_os_users(address, &os_user, &os_user);
    }

    pub fn write_config_with_os_users(
        &self,
        address: SocketAddr,
        alice_os_user: &str,
        bob_os_user: &str,
    ) {
        fs::create_dir(&self.state_dir).expect("state directory must be created");
        let alice_hash = hash("alice-secret");
        let bob_hash = hash("bob-secret");
        let people = format!(
            "[{{\"user_id\":\"alice\",\"name\":\"alice\",\"credential_hash\":\"{alice_hash}\",\"created_at\":1,\"is_owner\":true}},{{\"user_id\":\"bob\",\"name\":\"bob\",\"credential_hash\":\"{bob_hash}\",\"created_at\":2,\"is_owner\":false}}]\n"
        );
        fs::write(self.state_dir.join("people.json"), people).expect("people registry must write");
        fs::write(self.state_dir.join("seats.json"), "[]\n").expect("seat registry must write");
        let contents = format!(
            "listen = \"{address}\"\npublished_addr = \"host:7321\"\nremote = false\nstate_dir = \"{}\"\nowner_name = \"owner\"\n\n[os_users]\nalice = \"{alice_os_user}\"\nbob = \"{bob_os_user}\"\n",
            self.state_dir.display(),
        );
        fs::write(&self.config, contents).expect("broker config must write");
    }

    pub fn write_runtime_wrapper(&self) {
        let script = "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$SEER_TEST_FILES/$2.pid\"\nprintf '%s\\n%s\\n%s\\n%s\\n' \"$1\" \"$2\" \"$3\" \"$PWD\" > \"$SEER_TEST_FILES/$2.args\"\nprintf '%s\\n%s\\n%s\\n' \"$(id -u)\" \"$HOME\" \"$SHELL\" > \"$SEER_TEST_FILES/$2.identity\"\nexec \"$SEER_TEST_RUNTIME_BIN\" \"$@\"\n";
        fs::write(&self.wrapper, script).expect("runtime wrapper must write");
        fs::set_permissions(&self.wrapper, fs::Permissions::from_mode(0o700))
            .expect("runtime wrapper mode must set");
    }

    pub fn start_broker(&self) -> Child {
        let log = fs::File::create(&self.broker_log).expect("broker log must open");
        Command::new(BROKER_BINARY.get_or_init(|| binary::build("seer-broker")))
            .arg(&self.config)
            .env("XDG_RUNTIME_DIR", &self.xdg_runtime_dir)
            .env("SEER_RUNTIME_BIN", &self.wrapper)
            .env(
                "SEER_TEST_RUNTIME_BIN",
                RUNTIME_BINARY.get_or_init(|| binary::build("seer-runtime")),
            )
            .env("SEER_TEST_FILES", &self.root)
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .expect("broker must start")
    }

    pub fn pid_file(&self, user: &str) -> PathBuf {
        self.root.join(format!("{user}.pid"))
    }

    pub fn runtime_pid(&self, user: &str) -> u32 {
        let pid_file = self.pid_file(user);
        assert!(wait_for_file(&pid_file));
        fs::read_to_string(pid_file)
            .expect("runtime PID must be readable")
            .trim()
            .parse()
            .expect("runtime PID must be valid")
    }

    pub fn pane_pid(&self, user: &str) -> u32 {
        let pid_file = self.root.join(format!("{user}-pane.pid"));
        assert!(wait_for_file(&pid_file));
        fs::read_to_string(pid_file)
            .expect("pane PID must be readable")
            .trim()
            .parse()
            .expect("pane PID must be valid")
    }

    pub fn assert_process_running(&self, pid: u32) {
        let status = fs::read_to_string(format!("/proc/{pid}/status"))
            .expect("process status must be readable");
        assert!(!status.lines().any(|line| line.starts_with("State:\tZ")));
    }

    pub fn assert_runtime_arguments(&self, user: &str) {
        let arguments_file = self.root.join(format!("{user}.args"));
        assert!(wait_for_file(&arguments_file));
        let arguments = fs::read_to_string(arguments_file).expect("runtime arguments must read");
        let expected_socket = self.xdg_runtime_dir.join(format!("seer/{user}.sock"));
        let expected_state = self.state_dir.join("users").join(user);
        let shell = current_login_shell();
        assert_eq!(
            arguments.lines().collect::<Vec<_>>(),
            [
                expected_socket.to_string_lossy().as_ref(),
                user,
                shell.as_str(),
                expected_state.to_string_lossy().as_ref()
            ]
        );
    }

    pub fn assert_socket_directory(&self) {
        let directory = self.xdg_runtime_dir.join("seer");
        let mode = fs::metadata(directory)
            .expect("socket directory metadata must load")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    pub fn terminate_runtime(&self, user: &str) -> PathBuf {
        let pid = self.runtime_pid(user);
        terminate_process(pid);
        fs::remove_file(self.pid_file(user)).expect("runtime PID file must be removed");
        self.xdg_runtime_dir.join(format!("seer/{user}.sock"))
    }

    pub fn assert_log_contains(&self, expected: &str) {
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

    pub fn assert_log_excludes(&self, unexpected: &str) {
        let contents = fs::read_to_string(&self.broker_log).expect("broker log must read");
        assert!(!contents.contains(unexpected));
    }
}

fn current_os_user() -> String {
    command_output("id", &["-un"])
}

fn current_login_shell() -> String {
    let user = current_os_user();
    let record = command_output("getent", &["passwd", &user]);
    record
        .split(':')
        .nth(6)
        .expect("password record must contain a shell")
        .to_owned()
}

fn command_output(program: &str, arguments: &[&str]) -> String {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .expect("account command must run");
    assert!(output.status.success(), "account command must succeed");
    String::from_utf8(output.stdout)
        .expect("account output must be UTF-8")
        .trim()
        .to_owned()
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
        remove_temporary_directory(&self.root);
    }
}

fn remove_temporary_directory(directory: &Path) {
    let mut last_error = None;
    for attempt in 0..20 {
        match fs::remove_dir_all(directory) {
            Ok(()) => return,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => last_error = Some(error),
        }
        if attempt < 19 {
            thread::sleep(Duration::from_millis(50));
        }
    }
    panic!("temporary directory must be removed: {last_error:?}");
}

pub struct ProcessGuard(Child);

impl ProcessGuard {
    pub fn new(child: Child) -> Self {
        Self(child)
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        terminate_child(&mut self.0);
    }
}

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
