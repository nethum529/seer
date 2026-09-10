#![cfg(target_os = "linux")]

use std::fs;
use std::io;
use std::net::TcpListener;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TerminalInput};

const USER: &str = "alice-user-id";
const WAIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_secs(1);

// Issue 359: the client whose keyboard owns the terminal is the one that
// leaves. A second client that only watches the same terminal stays.
#[test]
fn exit_inside_a_seer_terminal_makes_only_the_typing_client_leave() {
    let root = Root::new("inside");
    let directory = root.0.join("state-home/seer/runtimes").join(USER);
    fs::create_dir_all(&directory).expect("runtime directory must exist");
    let socket = directory.join("socket");
    let _runtime = Runtime::start(&socket);

    let mut watcher = attach(&socket);
    let pane = first_pane(&mut watcher);
    let mut typist = attach(&socket);
    first_pane(&mut typist);
    send_text(&mut typist, &pane, "printf 'typist-here\\n'\n");
    wait_for_cells_containing(&mut typist, "typist-here");

    let output = run_exit(&root.0, Some(&pane));
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());

    assert_eq!(wait_for_bye(&mut typist), "detached");
    assert!(
        !receives_bye(&mut watcher),
        "the watching client must stay attached"
    );
}

#[test]
fn exit_outside_seer_fails_and_reaches_no_client() {
    let root = Root::new("outside");
    let room = TcpListener::bind("127.0.0.1:0").expect("room must listen");
    let endpoint = room.local_addr().expect("room address").to_string();
    write_store(&root.0, &endpoint);

    let output = run_exit(&root.0, None);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "seer exit works only inside a Seer terminal.\n"
    );

    room.set_nonblocking(true)
        .expect("room listener must become nonblocking");
    assert!(
        matches!(room.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock),
        "seer exit must not reach the room"
    );
    assert!(!root.0.join("state-home/seer/runtimes").exists());
}

struct Root(PathBuf);

impl Root {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(format!("/tmp/seer-exit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test directory must exist");
        Self(root)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_exit(root: &Path, inside: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_seer"));
    command
        .arg("exit")
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_STATE_HOME", root.join("state-home"))
        .env_remove("SEER_USER_ID")
        .env_remove("SEER_PANE")
        .stdin(Stdio::null());
    if let Some(pane) = inside {
        command.env("SEER_USER_ID", USER).env("SEER_PANE", pane);
    }
    command.output().expect("seer exit must run")
}

struct Runtime(Child);

impl Runtime {
    fn start(socket: &Path) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_seer-runtime"))
            .args([
                socket.as_os_str(),
                USER.as_ref(),
                "sh".as_ref(),
                "1".as_ref(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("runtime must start");
        Self(child)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn attach(socket: &Path) -> UnixStream {
    let deadline = Instant::now() + WAIT;
    let mut stream = loop {
        match UnixStream::connect(socket) {
            Ok(stream) => break stream,
            Err(error) => assert!(Instant::now() < deadline, "runtime did not listen: {error}"),
        }
        thread::sleep(Duration::from_millis(20));
    };
    stream
        .set_read_timeout(Some(WAIT))
        .expect("read timeout must set");
    match codec::decode(&mut stream).expect("runtime ready must decode") {
        ServerMsg::RuntimeReady { .. } => {}
        other => panic!("expected RuntimeReady, got {other:?}"),
    }
    codec::encode(&mut stream, &ClientMsg::AttachRuntime).expect("attach must encode");
    stream
}

fn first_pane(stream: &mut UnixStream) -> String {
    loop {
        if let ServerMsg::Tree { tree } = read(stream) {
            return tree.workspaces[0].tabs[0].panes[0].id.clone();
        }
    }
}

fn send_text(stream: &mut UnixStream, pane: &str, input: &str) {
    codec::encode(
        stream,
        &ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: pane.into(),
            input: TerminalInput::new(InputEvent::Text(input.to_owned())),
        },
    )
    .expect("input must encode");
}

fn wait_for_cells_containing(stream: &mut UnixStream, expected: &str) {
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(
            Instant::now() < deadline,
            "Cells did not contain {expected}"
        );
        if let ServerMsg::Cells { frame, .. } = read(stream) {
            let text = frame
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.character)
                .collect::<String>();
            if text.contains(expected) {
                return;
            }
        }
    }
}

fn wait_for_bye(stream: &mut UnixStream) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "Bye was not received");
        if let ServerMsg::Bye { reason } = read(stream) {
            return reason;
        }
    }
}

fn receives_bye(stream: &mut UnixStream) -> bool {
    stream
        .set_read_timeout(Some(QUIET))
        .expect("read timeout must set");
    loop {
        match codec::decode::<_, ServerMsg>(stream) {
            Ok(ServerMsg::Bye { .. }) => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
}

fn read(stream: &mut UnixStream) -> ServerMsg {
    codec::decode(stream).expect("server message must decode")
}

fn write_store(root: &Path, endpoint: &str) {
    fs::create_dir_all(root.join("seer")).expect("config directory must be created");
    fs::write(
        root.join("seer/servers.toml"),
        format!(
            "[[servers]]\nendpoint = \"{endpoint}\"\nalias = \"room\"\nuser_id = \"{USER}\"\nname = \"alice\"\ncredential = \"secret\"\ncurrent = true\n"
        ),
    )
    .expect("server store must be written");
}
