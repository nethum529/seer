use std::fs;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use sha2::{Digest, Sha256};

#[path = "support/binary.rs"]
mod binary;

static NEXT_TEMPORARY_DIRECTORY: AtomicUsize = AtomicUsize::new(0);
static BROKER_BINARY: OnceLock<PathBuf> = OnceLock::new();

#[test]
fn requires_config_path() {
    let child = broker_command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("broker must start");
    let output = wait_for_output(child);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("usage: seer-broker <config-path>"));
}

#[test]
fn loads_config_and_listens() {
    let address = unused_address();
    let config = TemporaryConfig::new(address);
    let child = broker_command()
        .arg(&config.path)
        .env("XDG_RUNTIME_DIR", &config.directory)
        .env("SEER_RUNTIME_BIN", config.directory.join("missing-runtime"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("broker must start");
    let _broker = BrokerProcess(child);
    let mut stream = connect_when_ready(address);
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout must set");

    codec::encode(
        &mut stream,
        &ClientMsg::Hello {
            user_id: "alice".into(),
            credential: "alice-secret".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
    )
    .expect("Hello must encode");
    let response: ServerMsg = codec::decode(&mut stream).expect("Welcome must decode");

    let ServerMsg::Welcome {
        user_id,
        name,
        client_id,
        tree,
    } = response
    else {
        panic!("expected Welcome");
    };
    assert_eq!(user_id, "alice");
    assert_eq!(name, "Alice");
    assert_eq!(client_id.len(), 32);
    assert!(client_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(tree, Tree::new());
}

#[test]
fn prints_the_owner_credential_only_on_first_start() {
    let address = unused_address();
    let config = TemporaryConfig::new_empty(address);
    let first_log = config.directory.join("first.out");
    {
        let first = start_broker(&config, &first_log);
        let _first = BrokerProcess(first);
        drop(connect_when_ready(address));
        assert!(wait_for_file(&first_log));
    }
    let first_output = fs::read_to_string(&first_log).expect("first output must read");
    let credential = first_output
        .strip_prefix("owner-credential: ")
        .and_then(|value| value.strip_suffix('\n'))
        .expect("owner credential must print once");
    assert_eq!(credential.len(), 64);
    assert!(credential.bytes().all(|byte| byte.is_ascii_hexdigit()));

    let second_log = config.directory.join("second.out");
    {
        let second = start_broker(&config, &second_log);
        let _second = BrokerProcess(second);
        drop(connect_when_ready(address));
    }
    assert_eq!(
        fs::read_to_string(second_log).expect("second output must read"),
        ""
    );
}

fn start_broker(config: &TemporaryConfig, output: &PathBuf) -> Child {
    let output = fs::File::create(output).expect("broker output must open");
    broker_command()
        .arg(&config.path)
        .env("XDG_RUNTIME_DIR", &config.directory)
        .env("SEER_RUNTIME_BIN", config.directory.join("missing-runtime"))
        .stdout(Stdio::from(output))
        .stderr(Stdio::null())
        .spawn()
        .expect("broker must start")
}

fn broker_command() -> Command {
    Command::new(BROKER_BINARY.get_or_init(|| binary::build("seer-broker")))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn unused_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port probe must bind");
    listener
        .local_addr()
        .expect("port probe must have an address")
}

fn connect_when_ready(address: SocketAddr) -> TcpStream {
    let mut last_error = None;
    for _ in 0..100 {
        match TcpStream::connect(address) {
            Ok(stream) => return stream,
            Err(error) => last_error = Some(error),
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("broker did not listen: {last_error:?}");
}

struct TemporaryConfig {
    path: PathBuf,
    directory: PathBuf,
}

impl TemporaryConfig {
    fn new(address: SocketAddr) -> Self {
        Self::create(address, true)
    }

    fn new_empty(address: SocketAddr) -> Self {
        Self::create(address, false)
    }

    fn create(address: SocketAddr, seed_owner: bool) -> Self {
        let counter = NEXT_TEMPORARY_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos()
            % 1_000_000_000;
        let directory = PathBuf::from(format!(
            "/tmp/sbc-{}-{counter}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("temporary directory must be created");
        let path = directory.join("broker.toml");
        let state_dir = directory.join("state");
        let contents = format!(
            "listen = \"{address}\"\npublished_addr = \"host:7321\"\nstate_dir = \"{}\"\nowner_name = \"Owner\"\n",
            state_dir.display()
        );
        fs::write(&path, contents).expect("temporary config must write");
        if seed_owner {
            fs::create_dir(&state_dir).expect("state directory must be created");
            let hash = hash("alice-secret");
            let people = format!(
                "[{{\"user_id\":\"alice\",\"name\":\"Alice\",\"credential_hash\":\"{hash}\",\"created_at\":1,\"is_owner\":true}}]\n"
            );
            fs::write(state_dir.join("people.json"), people).expect("people registry must write");
            fs::write(state_dir.join("seats.json"), "[]\n").expect("seat registry must write");
        }
        Self { path, directory }
    }
}

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn wait_for_file(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if fs::read_to_string(path).is_ok_and(|contents| contents.ends_with('\n')) {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

impl Drop for TemporaryConfig {
    fn drop(&mut self) {
        remove_temporary_directory(&self.directory);
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

struct BrokerProcess(Child);

impl Drop for BrokerProcess {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = self.0.kill();
        let kill_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < kill_deadline {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn wait_for_output(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if child
            .try_wait()
            .expect("broker status must be available")
            .is_some()
        {
            return child
                .wait_with_output()
                .expect("broker output must be available");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("broker did not exit within 2 seconds");
        }
        thread::sleep(Duration::from_millis(10));
    }
}
