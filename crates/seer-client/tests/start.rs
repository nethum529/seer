#![cfg(target_os = "linux")]

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(7);
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
static PROCESS_TEST: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
struct FakeConfig {
    listen: SocketAddr,
    state_dir: PathBuf,
}

struct TestDirectory {
    path: PathBuf,
}

struct CommandOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl TestDirectory {
    fn new() -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(format!("/tmp/s73-{}-{number}", std::process::id()));
        fs::create_dir(&path).expect("test directory must be created");
        Self { path }
    }

    fn config_home(&self) -> PathBuf {
        self.path.join("c")
    }

    fn state_home(&self) -> PathBuf {
        self.path.join("s")
    }

    fn state_dir(&self) -> PathBuf {
        self.state_home().join("seer")
    }

    fn pid_path(&self) -> PathBuf {
        self.state_dir().join("broker.pid")
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        for path in [self.pid_path(), self.state_dir().join("fake.pid")] {
            if let Ok(contents) = fs::read_to_string(path)
                && let Ok(pid) = contents.trim().parse::<i32>()
            {
                stop_process_group(pid);
            }
        }
        if let Ok(contents) = fs::read_to_string(self.state_dir().join("worker.pid"))
            && let Ok(pid) = contents.trim().parse::<i32>()
            && process_exists(pid)
        {
            stop_process(pid);
        }
        fs::remove_dir_all(&self.path).expect("test directory must be removed");
    }
}

#[test]
fn fake_broker_process() {
    let Some(config_path) = std::env::var_os("SEER_FAKE_CONFIG") else {
        return;
    };
    let contents = fs::read_to_string(config_path).expect("fake broker config must be read");
    let config: FakeConfig = toml::from_str(&contents).expect("fake broker config must parse");
    fs::create_dir_all(&config.state_dir).expect("fake broker state must be created");
    fs::write(
        config.state_dir.join("people.json"),
        r#"[{"user_id":"owner-id","name":"alice","is_owner":true}]"#,
    )
    .expect("fake people file must be written");
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(config.state_dir.join("launches"))
        .and_then(|mut file| writeln!(file, "launch"))
        .expect("launch must be recorded");
    fs::write(
        config.state_dir.join("fake.pid"),
        format!("{}\n", std::process::id()),
    )
    .expect("fake pid must be written");
    let listener = TcpListener::bind(config.listen).expect("fake broker must listen");
    let mut worker = if std::env::var_os("SEER_FAKE_DESCENDANT").is_some() {
        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("fake worker must spawn");
        fs::write(
            config.state_dir.join("worker.pid"),
            format!("{}\n", child.id()),
        )
        .expect("worker pid must be written");
        Some(child)
    } else {
        None
    };
    if std::env::var_os("SEER_FAKE_NO_CREDENTIAL").is_none() {
        thread::sleep(Duration::from_millis(250));
        println!("owner-credential: owner-secret");
        std::io::stdout()
            .flush()
            .expect("fake credential must flush");
    }
    listener
        .set_nonblocking(true)
        .expect("fake listener must be nonblocking");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("fake broker accept failed: {error}"),
        }
    }
    if let Some(child) = &mut worker {
        child
            .kill()
            .expect("fake worker must be killed on deadline");
        child.wait().expect("fake worker must be reaped");
    }
}

#[test]
fn prompt_defaults_create_config_and_owner_store() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    ensure_default_port_is_free();
    let executable = install_binaries(&directory);
    write_existing_servers(&directory);

    let output = run_start(&executable, &directory, "\n\n", &[]);

    assert!(output.status.success(), "{}", output.stderr);
    assert!(output.stdout.contains("Your name [alice]: "));
    assert!(
        output
            .stdout
            .contains("Published address [host.test:7321]: ")
    );
    assert!(
        output
            .stdout
            .contains("Server started at host.test:7321.\nYou are alice.\n")
    );
    let broker: toml::Value = read_toml(directory.config_home().join("seer/broker.toml"));
    assert_eq!(broker["listen"].as_str(), Some("0.0.0.0:7321"));
    assert_eq!(broker["published_addr"].as_str(), Some("host.test:7321"));
    assert_eq!(broker["owner_name"].as_str(), Some("alice"));
    assert_eq!(broker["state_dir"].as_str(), directory.state_dir().to_str());
    let servers: toml::Value = read_toml(directory.config_home().join("seer/servers.toml"));
    let owner = &servers["servers"][0];
    assert_eq!(owner["endpoint"].as_str(), Some("other.test:8000"));
    assert_eq!(owner["current"].as_bool(), Some(false));
    let local = &servers["servers"][1];
    assert_eq!(local["endpoint"].as_str(), Some("host.test:7321"));
    assert_eq!(local["alias"].as_str(), Some("host.test"));
    assert_eq!(local["user_id"].as_str(), Some("owner-id"));
    assert_eq!(local["name"].as_str(), Some("alice"));
    assert_eq!(local["credential"].as_str(), Some("owner-secret"));
    assert_eq!(local["current"].as_bool(), Some(true));
    assert_eq!(servers["servers"].as_array().map(Vec::len), Some(2));
    assert_eq!(mode(directory.config_home().join("seer")), 0o700);
    assert_eq!(
        mode(directory.config_home().join("seer/servers.toml")),
        0o600
    );
}

#[test]
fn second_start_uses_the_live_broker() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    let address = unused_address();
    write_config(&directory, address);
    let executable = install_binaries(&directory);
    let first = run_start(&executable, &directory, "", &[]);
    assert!(first.status.success(), "{}", first.stderr);
    let first_pid = fs::read_to_string(directory.pid_path()).expect("pid must exist");

    let second = run_start(&executable, &directory, "", &[]);

    assert!(second.status.success(), "{}", second.stderr);
    assert_eq!(
        second.stdout,
        format!("Server already running at {address}.\n")
    );
    assert_eq!(
        fs::read_to_string(directory.pid_path()).expect("pid must still exist"),
        first_pid
    );
    assert_eq!(launch_count(&directory), 1);
}

#[test]
fn dead_pid_is_replaced() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    let address = unused_address();
    write_config(&directory, address);
    fs::write(directory.pid_path(), "999999\n").expect("stale pid must be written");
    let executable = install_binaries(&directory);

    let output = run_start(&executable, &directory, "", &[]);

    assert!(output.status.success(), "{}", output.stderr);
    assert_ne!(
        fs::read_to_string(directory.pid_path()).expect("new pid must exist"),
        "999999\n"
    );
    assert_eq!(launch_count(&directory), 1);
}

#[test]
fn failed_broker_prints_only_the_last_twenty_log_lines() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    let address = unused_address();
    write_config(&directory, address);
    let executable = install_binaries(&directory);

    let output = run_start(&executable, &directory, "", &[("SEER_FAKE_FAIL", "1")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stderr.contains("failure-line-1\n"));
    assert!(output.stderr.contains("failure-line-6\n"));
    assert!(output.stderr.contains("failure-line-25\n"));
}

#[test]
fn invalid_config_is_not_replaced() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    let config_dir = directory.config_home().join("seer");
    fs::create_dir_all(&config_dir).expect("config directory must be created");
    let config_path = config_dir.join("broker.toml");
    fs::write(&config_path, [0xff]).expect("invalid config must be written");
    let executable = install_binaries(&directory);

    let output = run_start(&executable, &directory, "\n\n", &[]);

    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stdout.contains("Your name"));
    assert_eq!(fs::read(config_path).expect("config must remain"), [0xff]);
}

#[test]
fn unreadable_server_store_is_not_replaced() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    ensure_default_port_is_free();
    let config_dir = directory.config_home().join("seer");
    fs::create_dir_all(&config_dir).expect("config directory must be created");
    let store_path = config_dir.join("servers.toml");
    fs::write(&store_path, [0xff]).expect("invalid store must be written");
    let executable = install_binaries(&directory);

    let output = run_start(
        &executable,
        &directory,
        "\n\n",
        &[("SEER_FAKE_DESCENDANT", "1")],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(store_path).expect("store must remain"), [0xff]);
    for path in ["fake.pid", "worker.pid"] {
        let pid = fs::read_to_string(directory.state_dir().join(path))
            .expect("fake process pid must exist")
            .trim()
            .parse::<i32>()
            .expect("fake process pid must parse");
        assert!(!process_exists(pid), "broker process group must stop");
    }
}

fn install_binaries(directory: &TestDirectory) -> PathBuf {
    let bin_dir = directory.path.join("b");
    fs::create_dir(&bin_dir).expect("binary directory must be created");
    let executable = bin_dir.join("seer-client");
    fs::copy(env!("CARGO_BIN_EXE_seer-client"), &executable).expect("client binary must be copied");
    let test_binary = std::env::current_exe().expect("test binary path must be available");
    assert!(!test_binary.to_string_lossy().contains('\''));
    let script = format!(
        "#!/bin/sh\nif [ \"$SEER_FAKE_FAIL\" = 1 ]; then\n  i=1\n  while [ $i -le 25 ]; do echo failure-line-$i; i=$((i + 1)); done\n  exit 7\nfi\nexport SEER_FAKE_CONFIG=\"$1\"\nexec '{}' fake_broker_process --exact --nocapture\n",
        test_binary.display()
    );
    let broker = bin_dir.join("seer-broker");
    fs::write(&broker, script).expect("fake broker must be written");
    fs::set_permissions(&broker, fs::Permissions::from_mode(0o755))
        .expect("fake broker must be executable");
    executable
}

fn run_start(
    executable: &Path,
    directory: &TestDirectory,
    input: &str,
    environment: &[(&str, &str)],
) -> CommandOutput {
    let mut command = Command::new(executable);
    command
        .arg("start")
        .env("XDG_CONFIG_HOME", directory.config_home())
        .env("XDG_STATE_HOME", directory.state_home())
        .env("HOME", &directory.path)
        .env("USER", "alice")
        .env("HOSTNAME", "host.test")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("seer start must spawn");
    child
        .stdin
        .take()
        .expect("stdin must be piped")
        .write_all(input.as_bytes())
        .expect("prompt input must be written");
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().expect("seer start wait must work") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("timed out seer start must be killed");
            child.wait().expect("killed seer start must be reaped");
            panic!("seer start exceeded its test deadline");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .expect("stdout must be piped")
        .read_to_string(&mut stdout)
        .expect("stdout must be read");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr must be piped")
        .read_to_string(&mut stderr)
        .expect("stderr must be read");
    CommandOutput {
        status,
        stdout,
        stderr,
    }
}

fn write_config(directory: &TestDirectory, address: SocketAddr) {
    let config_dir = directory.config_home().join("seer");
    fs::create_dir_all(&config_dir).expect("config directory must be created");
    fs::create_dir_all(directory.state_dir()).expect("state directory must be created");
    fs::write(
        config_dir.join("broker.toml"),
        format!(
            "listen = \"{address}\"\npublished_addr = \"{address}\"\nowner_name = \"alice\"\nstate_dir = {:?}\n",
            directory.state_dir()
        ),
    )
    .expect("broker config must be written");
}

fn write_existing_servers(directory: &TestDirectory) {
    let config_dir = directory.config_home().join("seer");
    fs::create_dir_all(&config_dir).expect("config directory must be created");
    fs::write(
        config_dir.join("servers.toml"),
        "[[servers]]\nendpoint = \"host.test:7321\"\nalias = \"old\"\nuser_id = \"old-id\"\nname = \"old\"\ncredential = \"old-secret\"\ncurrent = true\n\n[[servers]]\nendpoint = \"other.test:8000\"\nalias = \"other.test\"\nuser_id = \"other-id\"\nname = \"bob\"\ncredential = \"other-secret\"\ncurrent = false\n",
    )
    .expect("existing servers must be written");
}

fn unused_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port probe must bind");
    listener.local_addr().expect("port probe must have address")
}

fn ensure_default_port_is_free() {
    TcpListener::bind("127.0.0.1:7321").expect("default port must be free");
}

fn read_toml(path: PathBuf) -> toml::Value {
    toml::from_str(&fs::read_to_string(path).expect("TOML file must be read"))
        .expect("TOML file must parse")
}

fn mode(path: PathBuf) -> u32 {
    fs::metadata(path)
        .expect("path metadata must exist")
        .permissions()
        .mode()
        & 0o777
}

fn launch_count(directory: &TestDirectory) -> usize {
    fs::read_to_string(directory.state_dir().join("launches"))
        .expect("launch file must exist")
        .lines()
        .count()
}

fn stop_process_group(pid: i32) {
    // Safety: kill receives the process group ID created by the command under test.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_exists(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_exists(pid),
        "fake broker must stop by the deadline"
    );
}

fn stop_process(pid: i32) {
    // Safety: kill receives a PID created by the fake broker.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_exists(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_exists(pid),
        "fake worker must stop by the deadline"
    );
}

fn process_exists(pid: i32) -> bool {
    if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat"))
        && stat.split_whitespace().nth(2) == Some("Z")
    {
        return false;
    }
    // Safety: signal zero checks a process without changing it.
    unsafe { libc::kill(pid, 0) == 0 }
}
