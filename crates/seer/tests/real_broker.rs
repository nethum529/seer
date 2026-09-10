use std::fs;
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const WAIT_TIMEOUT: Duration = Duration::from_secs(7);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

#[test]
fn commands_work_with_stream_messages_from_the_real_broker() {
    let files = TestFiles::new();
    let address = unused_address();
    files.write_broker_files(address);
    let broker = files.start_broker();
    let _broker = BrokerProcess(broker);
    connect_when_ready(address, &files.broker_output, &files.broker_log);
    let (owner_id, owner_credential) = files.owner_identity();
    write_store(
        &files.owner_config,
        address,
        "owner",
        &owner_id,
        &owner_credential,
    );

    let owner_invite = run_seer(&files.owner_config, &["invite"], "");
    assert!(owner_invite.status.success());
    assert!(owner_invite.stderr.is_empty());
    let invitation = invitation_from(&owner_invite.stdout);

    let join_input = format!("{invitation}\nbob\n");
    let join = run_seer(&files.guest_config, &["join"], &join_input);
    assert!(join.status.success());
    assert!(join.stderr.is_empty());
    assert_eq!(
        text(&join.stdout),
        format!("Invitation: Server: {address}\nName: Joined as bob. Attaching...\n")
    );

    thread::sleep(Duration::from_millis(100));
    let list = run_seer(&files.guest_config, &["list"], "");
    assert!(list.status.success());
    assert!(list.stderr.is_empty());
    assert_eq!(
        text(&list.stdout),
        "SERVER     YOU  STATE     PEOPLE\n127.0.0.1  bob  detached  bob active 0 - 0s, owner away 0 - 0s\n"
    );

    let guest_invite = run_seer(&files.guest_config, &["invite"], "");
    assert_eq!(guest_invite.status.code(), Some(1));
    assert!(guest_invite.stdout.is_empty());
    assert_eq!(guest_invite.stderr, b"owner access required\n");
}

fn invitation_from(output: &[u8]) -> String {
    let output = text(output);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.first(),
        Some(&"Seat ready. It works once and expires in 1 hour.")
    );
    assert_eq!(lines.get(1), Some(&"Send this to a friend:"));
    let join = lines.get(4).expect("join command must print");
    let invitation = join
        .strip_prefix("seer join ")
        .expect("join command must include the invitation");
    let install = lines.get(7).expect("install command must print");
    assert!(install.ends_with(invitation));
    assert_eq!(lines.len(), 8);
    assert!(invitation.starts_with("SEER1-127.0.0.1-"));
    invitation.to_owned()
}

fn write_store(config: &Path, address: SocketAddr, name: &str, user_id: &str, credential: &str) {
    let directory = config.join("seer");
    fs::create_dir_all(&directory).expect("config directory must be created");
    let contents = format!(
        "[[servers]]\nendpoint = \"{address}\"\nalias = \"127.0.0.1\"\nuser_id = \"{user_id}\"\nname = \"{name}\"\ncredential = \"{credential}\"\ncurrent = true\n"
    );
    fs::write(directory.join("servers.toml"), contents).expect("server store must write");
}

fn run_seer(config: &Path, arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seer"))
        .args(arguments)
        .env("XDG_CONFIG_HOME", config)
        .env("XDG_STATE_HOME", config.join("state-home"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("seer must start");
    child
        .stdin
        .take()
        .expect("stdin must be piped")
        .write_all(input.as_bytes())
        .expect("seer input must write");
    wait_for_output(child, WAIT_TIMEOUT)
}

fn wait_for_output(mut child: Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .expect("process status must be available")
            .is_some()
        {
            return child.wait_with_output().expect("process output must read");
        }
        if Instant::now() >= deadline {
            child.kill().expect("timed out process must be killed");
            let _ = child.wait();
            panic!("process did not exit before the deadline");
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn unused_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port probe must bind");
    listener
        .local_addr()
        .expect("port probe must have an address")
}

fn connect_when_ready(address: SocketAddr, output: &Path, log: &Path) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let mut last_error = None;
    while Instant::now() < deadline {
        match TcpStream::connect(address) {
            Ok(stream) => {
                drop(stream);
                return;
            }
            Err(error) => last_error = Some(error),
        }
        thread::sleep(POLL_INTERVAL);
    }
    panic!(
        "broker did not listen: {last_error:?}; output: {:?}; log: {:?}",
        fs::read_to_string(output),
        fs::read_to_string(log)
    );
}

struct TestFiles {
    root: PathBuf,
    state_dir: PathBuf,
    broker_config: PathBuf,
    broker_output: PathBuf,
    broker_log: PathBuf,
    runtime_wrapper: PathBuf,
    runtime_dir: PathBuf,
    owner_config: PathBuf,
    guest_config: PathBuf,
}

impl TestFiles {
    fn new() -> Self {
        let counter = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = PathBuf::from(format!("/tmp/s83-{}-{counter}", std::process::id()));
        fs::create_dir(&root).expect("temporary directory must be created");
        Self {
            state_dir: root.join("state"),
            broker_config: root.join("broker.toml"),
            broker_output: root.join("broker.out"),
            broker_log: root.join("broker.log"),
            runtime_wrapper: root.join("runtime-wrapper"),
            runtime_dir: root.join("run"),
            owner_config: root.join("owner-config"),
            guest_config: root.join("guest-config"),
            root,
        }
    }

    fn write_broker_files(&self, address: SocketAddr) {
        let config = format!(
            "listen = \"{address}\"\npublished_addr = \"{address}\"\nremote = false\nstate_dir = \"{}\"\nowner_name = \"owner\"\n",
            self.state_dir.display(),
        );
        fs::write(&self.broker_config, config).expect("broker config must write");
        let wrapper = "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$SEER_TEST_ROOT/runtime-$2.pid\"\nexec \"$SEER_TEST_RUNTIME_BIN\" \"$@\"\n";
        fs::write(&self.runtime_wrapper, wrapper).expect("runtime wrapper must write");
        fs::set_permissions(&self.runtime_wrapper, fs::Permissions::from_mode(0o700))
            .expect("runtime wrapper mode must set");
    }

    fn start_broker(&self) -> Child {
        let output = fs::File::create(&self.broker_output).expect("broker output must open");
        let log = fs::File::create(&self.broker_log).expect("broker log must open");
        Command::new(broker_binary())
            .arg(&self.broker_config)
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .env("SEER_RUNTIME_BIN", &self.runtime_wrapper)
            .env("SEER_TEST_RUNTIME_BIN", runtime_binary())
            .env("SEER_TEST_ROOT", &self.root)
            .stdout(Stdio::from(output))
            .stderr(Stdio::from(log))
            .spawn()
            .expect("broker must start")
    }

    fn owner_identity(&self) -> (String, String) {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(identity) = read_owner_identity(&self.broker_output) {
                return identity;
            }
            thread::sleep(POLL_INTERVAL);
        }
        panic!("owner identity did not become available");
    }
}

impl Drop for TestFiles {
    fn drop(&mut self) {
        for user in ["owner", "bob"] {
            if let Ok(pid) = fs::read_to_string(self.root.join(format!("runtime-{user}.pid")))
                .and_then(|value| value.trim().parse::<u32>().map_err(io::Error::other))
            {
                terminate_pid(pid);
            }
        }
        remove_directory(&self.root);
    }
}

struct BrokerProcess(Child);

impl Drop for BrokerProcess {
    fn drop(&mut self) {
        terminate_child(&mut self.0);
    }
}

fn broker_binary() -> PathBuf {
    sibling_binary("seer-broker")
}

fn runtime_binary() -> PathBuf {
    sibling_binary("seer-runtime")
}

fn sibling_binary(binary: &str) -> PathBuf {
    let sibling = Path::new(env!("CARGO_BIN_EXE_seer"))
        .parent()
        .expect("seer binary must have a parent")
        .join(binary);
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut build = Command::new(cargo)
        .args(["build", "-p", "seer", "--bin", binary])
        .current_dir(manifest)
        .spawn()
        .expect("sibling binary must build");
    let status = wait_for_child(&mut build, Duration::from_secs(60)).unwrap_or_else(|| {
        let _ = build.kill();
        assert!(wait_for_child(&mut build, Duration::from_secs(2)).is_some());
        panic!("sibling binary build timed out");
    });
    assert!(status.success(), "sibling binary must build");
    assert!(sibling.is_file(), "sibling binary must exist after build");
    sibling
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

fn terminate_pid(pid: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !pid_exists(pid) {
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
    let _ = Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
}

fn pid_exists(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

fn remove_directory(path: &Path) {
    for attempt in 0..20 {
        match fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) if attempt == 19 => panic!("temporary directory removal failed: {error}"),
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("command output must be UTF-8")
}

fn read_owner_identity(path: &Path) -> Option<(String, String)> {
    let output = fs::read_to_string(path).ok()?;
    let user_id = output
        .lines()
        .find_map(|line| line.strip_prefix("owner-id: "))?;
    let credential = output
        .lines()
        .find_map(|line| line.strip_prefix("owner-credential: "))?;
    Some((user_id.to_owned(), credential.to_owned()))
}
