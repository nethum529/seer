#![cfg(target_os = "linux")]
use std::fs;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/cli_run.rs"]
mod cli_run;
#[path = "support/process.rs"]
mod process;

use cli_run::{TestConfig, run, text};
use process::{process_exists, wait_for_process_end};

const OWNER: &str = "owner-id";
const WAIT: Duration = Duration::from_secs(5);

// The test binary runs this as its own process, so seer stop finds a room
// server that owns the listen socket and leads its own process group.
#[test]
fn fake_room_process() {
    let Some(address) = std::env::var_os("SEER_FAKE_ROOM_ADDRESS") else {
        return;
    };
    let address: SocketAddr = address
        .to_str()
        .and_then(|address| address.parse().ok())
        .expect("fake room address must parse");
    let version = std::env::var("SEER_FAKE_ROOM_VERSION").expect("fake room version must be set");
    let listener = TcpListener::bind(address).expect("fake room must listen");
    for stream in listener.incoming() {
        let mut stream = stream.expect("fake room accept must work");
        let Ok(ClientMsg::Hello {
            version: client, ..
        }) = codec::decode(&mut stream)
        else {
            continue;
        };
        let reason =
            format!("version mismatch: server {version}, client {client}. Run: seer update");
        let _ = codec::encode(&mut stream, &ServerMsg::Refused { reason });
    }
}

#[test]
fn stop_ends_a_room_server_of_another_minor_version_on_this_computer() {
    let config = TestConfig::new();
    let state_dir = config.root.join("state");
    fs::create_dir_all(&state_dir).expect("state directory must exist");
    let older = older_minor_version();
    let hosted = unused_address();
    let hosted_room = spawn_fake_room(hosted, &older, Some(&state_dir.join("broker.pid")));
    let elsewhere = unused_address();
    let other_room = spawn_fake_room(elsewhere, &older, None);
    write_broker_config(&config, hosted, &state_dir);
    let mut runtime = start_runtime(&config);

    write_identity(&config, elsewhere);
    let refused = run(&config, &["stop"], "");
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    assert_eq!(
        text(&refused.stderr),
        format!(
            "The room runs Seer {older}. You run Seer {}. The room owner must update.\n",
            env!("CARGO_PKG_VERSION")
        )
    );
    assert!(process_exists(hosted_room.pid()));

    write_identity(&config, hosted);
    let stopped = run(&config, &["stop"], "");

    assert_eq!(stopped.status.code(), Some(0), "{}", text(&stopped.stderr));
    assert_eq!(
        text(&stopped.stdout),
        "Your terminals on this computer stopped.\nRoom server stopped.\n"
    );
    assert!(stopped.stderr.is_empty());
    assert!(!state_dir.join("broker.pid").exists());
    wait_for_process_end(hosted_room.pid());
    assert!(process_exists(other_room.pid()));
    let deadline = Instant::now() + WAIT;
    while runtime.0.try_wait().expect("runtime status").is_none() {
        assert!(Instant::now() < deadline, "the host runtime must stop");
        thread::sleep(Duration::from_millis(25));
    }
}

fn older_minor_version() -> String {
    let minor: u32 = env!("CARGO_PKG_VERSION_MINOR")
        .parse()
        .expect("minor version must be a number");
    let older = minor.checked_sub(1).expect("minor version must not be 0");
    format!("{}.{older}.0", env!("CARGO_PKG_VERSION_MAJOR"))
}

fn unused_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port probe must bind");
    listener.local_addr().expect("port probe must have address")
}

fn spawn_fake_room(address: SocketAddr, version: &str, pid_path: Option<&Path>) -> FakeRoom {
    let test_binary = std::env::current_exe().expect("test binary path must be available");
    let mut command = Command::new(test_binary);
    command
        .args(["fake_room_process", "--exact", "--nocapture"])
        .env("SEER_FAKE_ROOM_ADDRESS", address.to_string())
        .env("SEER_FAKE_ROOM_VERSION", version)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid touches no memory and runs before exec. It gives the fake
    // room its own process group, the one that seer stop signals.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() >= 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    let mut child = command.spawn().expect("fake room must start");
    let pid = i32::try_from(child.id()).expect("pid must fit");
    // seer stop waits until the room server is gone. A child of this test
    // stays a zombie until it is reaped, so a thread reaps it at once.
    let reaper = thread::spawn(move || {
        let _ = child.wait();
    });
    let room = FakeRoom {
        pid,
        reaper: Some(reaper),
    };
    if let Some(pid_path) = pid_path {
        fs::write(pid_path, format!("{}\n", room.pid())).expect("fake room pid must be written");
    }
    let deadline = Instant::now() + WAIT;
    while TcpStream::connect(address).is_err() {
        assert!(Instant::now() < deadline, "fake room must listen");
        thread::sleep(Duration::from_millis(10));
    }
    room
}

struct FakeRoom {
    pid: i32,
    reaper: Option<thread::JoinHandle<()>>,
}

impl FakeRoom {
    fn pid(&self) -> i32 {
        self.pid
    }
}

impl Drop for FakeRoom {
    fn drop(&mut self) {
        // SAFETY: kill receives the process group ID of the fake room this test started with setsid.
        unsafe {
            libc::kill(-self.pid, libc::SIGKILL);
        }
        if let Some(reaper) = self.reaper.take() {
            let _ = reaper.join();
        }
    }
}

fn write_broker_config(config: &TestConfig, address: SocketAddr, state_dir: &Path) {
    let config_dir = config.root.join("seer");
    fs::create_dir_all(&config_dir).expect("config directory must exist");
    fs::write(
        config_dir.join("broker.toml"),
        format!(
            "listen = \"{address}\"\npublished_addr = \"{address}\"\nremote = false\nowner_name = \"alice\"\nstate_dir = {state_dir:?}\n"
        ),
    )
    .expect("broker config must be written");
}

fn write_identity(config: &TestConfig, address: SocketAddr) {
    fs::write(
        config.root.join("seer/servers.toml"),
        format!(
            "[[servers]]\nendpoint = \"{address}\"\nalias = \"local\"\nuser_id = \"{OWNER}\"\nname = \"alice\"\ncredential = \"owner-secret\"\ncurrent = true\n"
        ),
    )
    .expect("owner identity must be written");
}

fn start_runtime(config: &TestConfig) -> Runtime {
    let directory = config.root.join("state-home/seer/runtimes").join(OWNER);
    fs::create_dir_all(&directory).expect("runtime directory must exist");
    let socket = directory.join("socket");
    let runtime = Runtime(
        Command::new(env!("CARGO_BIN_EXE_seer-runtime"))
            .arg(&socket)
            .arg(OWNER)
            .args(["/bin/sh", "stop-test"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("runtime must start"),
    );
    fs::write(directory.join("runtime.pid"), runtime.0.id().to_string())
        .expect("runtime PID must be saved");
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(mut stream) = UnixStream::connect(&socket) {
            stream
                .set_read_timeout(Some(WAIT))
                .expect("read timeout must be set");
            let ready: ServerMsg = codec::decode(&mut stream).expect("runtime must answer");
            assert!(matches!(ready, ServerMsg::RuntimeReady { .. }));
            return runtime;
        }
        assert!(Instant::now() < deadline, "runtime must become ready");
        thread::sleep(Duration::from_millis(25));
    }
}

struct Runtime(Child);

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
