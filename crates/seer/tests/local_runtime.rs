#![cfg(target_os = "linux")]

use std::fs;
use std::net::TcpListener;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

const WAIT: Duration = Duration::from_secs(20);
const USER: &str = "alice-user-id";

// Issue 338: several windows on one computer share that person's runtime, and
// a window opens even when the room is unreachable. A race between two windows
// must not leave two runtimes or a stale PID.
#[test]
fn windows_opening_together_share_one_runtime_while_the_room_is_offline() {
    let root = PathBuf::from(format!("/tmp/seer-local-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("test directory must be created");
    write_store(&root);

    let _windows = Windows {
        root: root.clone(),
        children: (0..4).map(|_| open_window(&root)).collect(),
    };
    let socket = root
        .join("state-home/seer/runtimes")
        .join(USER)
        .join("socket");
    let first = read_tree(&socket);
    for _ in 0..3 {
        assert_eq!(
            read_tree(&socket),
            first,
            "every window on this computer must attach to the same runtime"
        );
    }

    let directory = root.join("state-home/seer/runtimes").join(USER);
    let pid = wait_for_pid(&directory);
    assert!(
        Path::new(&format!("/proc/{pid}")).exists(),
        "the recorded PID must be the runtime that is running, not one that lost the race"
    );
}

struct Windows {
    root: PathBuf,
    children: Vec<Child>,
}

impl Drop for Windows {
    fn drop(&mut self) {
        for child in &mut self.children {
            // SAFETY: each child starts a private process group owned by this test.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
        }
        let _ = Command::new(env!("CARGO_BIN_EXE_seer"))
            .arg("stop")
            .env("XDG_CONFIG_HOME", &self.root)
            .env("XDG_STATE_HOME", self.root.join("state-home"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn wait_for_pid(directory: &Path) -> i32 {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(text) = fs::read_to_string(directory.join("runtime.pid"))
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "the runtime PID must be recorded"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

// A window needs a real terminal, which is what starts the local runtime.
fn open_window(root: &Path) -> std::process::Child {
    Command::new("script")
        .process_group(0)
        .args([
            "-qec",
            &format!("{} attach", env!("CARGO_BIN_EXE_seer")),
            "/dev/null",
        ])
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_STATE_HOME", root.join("state-home"))
        .env("TERM", "xterm")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("a window must start")
}

fn read_tree(socket: &Path) -> String {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Ok(mut stream) = UnixStream::connect(socket) {
            stream
                .set_read_timeout(Some(WAIT))
                .expect("read timeout must set");
            if let Ok(ServerMsg::RuntimeReady { .. }) = codec::decode::<_, ServerMsg>(&mut stream) {
                codec::encode(&mut stream, &ClientMsg::AttachRuntime).expect("attach must send");
                while let Ok(message) = codec::decode::<_, ServerMsg>(&mut stream) {
                    if let ServerMsg::Tree { tree } = message
                        && let Some(workspace) = tree.workspaces.first()
                        && !workspace.tabs.is_empty()
                    {
                        return format!("{:?}", workspace.tabs);
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("the local runtime never answered");
}

fn write_store(root: &Path) {
    write_store_for(root, "127.0.0.1:1");
}

fn write_store_for(root: &Path, endpoint: &str) {
    fs::create_dir_all(root.join("seer")).expect("config directory must be created");
    fs::write(
        root.join("seer/servers.toml"),
        format!(
            "[[servers]]\nendpoint = \"{endpoint}\"\nalias = \"room\"\nuser_id = \"{USER}\"\nname = \"alice\"\ncredential = \"secret\"\ncurrent = true\n"
        ),
    )
    .expect("server store must be written");
}

// Issue 338: a room that accepts the connection and never answers must not
// hold up the local terminals. A refused port fails at once and would hide it.
#[test]
fn a_room_that_never_answers_does_not_hold_up_this_computer() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a silent room must listen");
    let endpoint = listener.local_addr().expect("the room address").to_string();
    let held = thread::spawn(move || {
        let mut accepted = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            accepted.push(stream);
        }
    });

    let root = PathBuf::from(format!("/tmp/seer-silent-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("test directory must be created");
    write_store_for(&root, &endpoint);

    let _windows = Windows {
        root: root.clone(),
        children: vec![open_window(&root)],
    };
    let socket = root
        .join("state-home/seer/runtimes")
        .join(USER)
        .join("socket");
    let start = Instant::now();
    read_tree(&socket);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "the local runtime must answer while the room stays silent"
    );

    let directory = root.join("state-home/seer/runtimes").join(USER);
    wait_for_pid(&directory);
    drop(held);
}
