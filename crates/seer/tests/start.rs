#[path = "support/process.rs"]
mod process;
use process::{process_exists, wait_for_process_end};

#[path = "start/cases.rs"]
mod cases;

use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
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
    if std::env::var_os("SEER_FAKE_REMOTE_FAILURE").is_some() {
        panic!("fake remote listener failed");
    }
    fs::create_dir_all(&config.state_dir).expect("fake broker state must be created");
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
        println!("broker-output");
        println!("owner-id: owner-id");
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
            Ok((stream, _)) => serve_fake_client(stream),
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

fn serve_fake_client(mut stream: TcpStream) {
    stream
        .set_nonblocking(false)
        .expect("fake client stream must be blocking");
    let Ok(ClientMsg::Hello { .. }) = codec::decode(&mut stream) else {
        return;
    };
    codec::encode(
        &mut stream,
        &ServerMsg::Welcome {
            user_id: "owner-id".into(),
            name: "alice".into(),
            client_id: "client-id".into(),
            tree: Tree::new(),
        },
    )
    .expect("fake welcome must write");
    let request: ClientMsg = codec::decode(&mut stream).expect("fake invite request must read");
    assert_eq!(request, ClientMsg::Invite { hours: None });
    codec::encode(
        &mut stream,
        &ServerMsg::Seat {
            capsule: "SEER1-127.0.0.1-7321-seat-token".into(),
            expires_in_secs: 3_600,
        },
    )
    .expect("fake seat must write");
}

fn install_binaries(directory: &TestDirectory) -> PathBuf {
    let bin_dir = directory.path.join("b");
    fs::create_dir(&bin_dir).expect("binary directory must be created");
    let executable = bin_dir.join("seer");
    fs::copy(env!("CARGO_BIN_EXE_seer"), &executable).expect("client binary must be copied");
    let test_binary = std::env::current_exe().expect("test binary path must be available");
    assert!(!test_binary.to_string_lossy().contains('\''));
    let script = format!(
        "#!/bin/sh\nif [ \"$2\" = --remint-owner ]; then\n  echo 'owner-id: owner-id'\n  echo 'owner-credential: owner-secret'\n  exit 0\nfi\nif [ \"$SEER_FAKE_FAIL\" = 1 ]; then\n  i=1\n  while [ $i -le 25 ]; do echo failure-line-$i; i=$((i + 1)); done\n  exit 7\nfi\nexport SEER_FAKE_CONFIG=\"$1\"\nexec '{}' fake_broker_process --exact --nocapture\n",
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
        "[[servers]]\nendpoint = \"127.0.0.1:7321\"\nalias = \"old\"\nuser_id = \"old-id\"\nname = \"old\"\ncredential = \"old-secret\"\ncurrent = true\n\n[[servers]]\nendpoint = \"other.test:8000\"\nalias = \"other.test\"\nuser_id = \"other-id\"\nname = \"bob\"\ncredential = \"other-secret\"\ncurrent = false\n",
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
    wait_for_process_end(pid);
}

fn stop_process(pid: i32) {
    // Safety: kill receives a PID created by the fake broker.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    wait_for_process_end(pid);
}
