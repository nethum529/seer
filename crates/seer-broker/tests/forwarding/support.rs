use std::fs;
use std::io;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(super) struct TestFiles {
    root: PathBuf,
    state_dir: PathBuf,
    config: PathBuf,
    wrapper: PathBuf,
    broker_log: PathBuf,
    xdg_runtime_dir: PathBuf,
}

impl TestFiles {
    pub(super) fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos()
            % 1_000_000_000;
        let root = PathBuf::from(format!("/tmp/sbf-{}-{timestamp}", std::process::id()));
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

    pub(super) fn write_config(&self, address: SocketAddr) {
        fs::create_dir(&self.state_dir).expect("state directory must be created");
        let alice_hash = hash("alice-secret");
        let bob_hash = hash("bob-secret");
        let people = format!(
            "[{{\"user_id\":\"alice\",\"name\":\"alice\",\"credential_hash\":\"{alice_hash}\",\"created_at\":1,\"is_owner\":true}},{{\"user_id\":\"bob\",\"name\":\"bob\",\"credential_hash\":\"{bob_hash}\",\"created_at\":2,\"is_owner\":false}}]\n"
        );
        fs::write(self.state_dir.join("people.json"), people).expect("people registry must write");
        fs::write(self.state_dir.join("seats.json"), "[]\n").expect("seat registry must write");
        let contents = format!(
            "listen = \"{address}\"\npublished_addr = \"host:7321\"\nstate_dir = \"{}\"\nowner_name = \"owner\"\nshell = \"sh\"\n",
            self.state_dir.display()
        );
        fs::write(&self.config, contents).expect("broker config must write");
    }

    pub(super) fn write_runtime_wrapper(&self) {
        let script = "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$SEER_TEST_FILES/$2.pid\"\nprintf '%s\\n%s\\n%s\\n%s\\n' \"$1\" \"$2\" \"$3\" \"$PWD\" > \"$SEER_TEST_FILES/$2.args\"\nexec \"$SEER_TEST_RUNTIME_BIN\" \"$@\"\n";
        fs::write(&self.wrapper, script).expect("runtime wrapper must write");
        fs::set_permissions(&self.wrapper, fs::Permissions::from_mode(0o700))
            .expect("runtime wrapper mode must set");
    }

    pub(super) fn start_broker(&self) -> Child {
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

    pub(super) fn pid_file(&self, user: &str) -> PathBuf {
        self.root.join(format!("{user}.pid"))
    }

    pub(super) fn runtime_pid(&self, user: &str) -> u32 {
        let pid_file = self.pid_file(user);
        assert!(wait_for_file(&pid_file));
        fs::read_to_string(pid_file)
            .expect("runtime PID must be readable")
            .trim()
            .parse()
            .expect("runtime PID must be valid")
    }

    pub(super) fn assert_runtime_arguments(&self, user: &str) {
        let arguments_file = self.root.join(format!("{user}.args"));
        assert!(wait_for_file(&arguments_file));
        let arguments = fs::read_to_string(arguments_file).expect("runtime arguments must read");
        let expected_socket = self.xdg_runtime_dir.join(format!("seer/{user}.sock"));
        let expected_state = self.state_dir.join("users").join(user);
        assert_eq!(
            arguments.lines().collect::<Vec<_>>(),
            [
                expected_socket.to_string_lossy().as_ref(),
                user,
                "sh",
                expected_state.to_string_lossy().as_ref()
            ]
        );
    }

    pub(super) fn assert_socket_directory(&self) {
        let directory = self.xdg_runtime_dir.join("seer");
        let mode = fs::metadata(directory)
            .expect("socket directory metadata must load")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    pub(super) fn terminate_runtime(&self, user: &str) {
        let pid = self.runtime_pid(user);
        terminate_process(pid);
        fs::remove_file(self.pid_file(user)).expect("runtime PID file must be removed");
    }

    pub(super) fn assert_log_contains(&self, expected: &str) {
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

    pub(super) fn assert_log_excludes(&self, unexpected: &str) {
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

pub(super) struct ProcessGuard(Child);

impl ProcessGuard {
    pub(super) fn new(child: Child) -> Self {
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
