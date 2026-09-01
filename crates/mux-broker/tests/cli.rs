use std::fs;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mux_core::Tree;
use mux_core::proto::{ClientMsg, ServerMsg, codec};

#[test]
fn requires_config_path() {
    let child = broker_command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("broker must start");
    let output = wait_for_output(child);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("usage: mux-broker <config-path>"));
}

#[test]
fn loads_config_and_listens() {
    let address = unused_address();
    let config = TemporaryConfig::new(address);
    let child = broker_command()
        .arg(&config.path)
        .env("XDG_RUNTIME_DIR", &config.directory)
        .env("MUX_RUNTIME_BIN", config.directory.join("missing-runtime"))
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
            user: "alice".into(),
            token: "alice-secret".into(),
        },
    )
    .expect("Hello must encode");
    let response: ServerMsg = codec::decode(&mut stream).expect("Welcome must decode");

    assert_eq!(
        response,
        ServerMsg::Welcome {
            user: "alice".into(),
            tree: Tree::new(),
        }
    );
}

fn broker_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mux-broker"))
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
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("mux-broker-cli-{}-{timestamp}", std::process::id()));
        fs::create_dir(&directory).expect("temporary directory must be created");
        let path = directory.join("broker.toml");
        let contents = format!(
            "listen = \"{address}\"\n\n[[users]]\nuser = \"alice\"\ntoken = \"alice-secret\"\n"
        );
        fs::write(&path, contents).expect("temporary config must write");
        Self { path, directory }
    }
}

impl Drop for TemporaryConfig {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.directory)
            && error.kind() != io::ErrorKind::NotFound
        {
            panic!("temporary directory must be removed: {error}");
        }
    }
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
