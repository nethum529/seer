use std::io::Write;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[test]
fn terminal_names_are_shared_durable_and_owner_only() {
    let directory = std::env::temp_dir().join(format!("seer-names-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("state directory");
    let socket = directory.join("socket");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let binary = target.join("debug/seer-runtime");
    assert!(
        Command::new("cargo")
            .args(["build", "-p", "seer", "--bin", "seer-runtime"])
            .env("CARGO_TARGET_DIR", target)
            .status()
            .expect("build runtime")
            .success()
    );
    let launch = || {
        Running(
            Command::new(&binary)
                .arg(&socket)
                .args(["alice", "sh", "0123456789abcdef0123456789abcdef"])
                .env("SEER_SNAPSHOT_DIR", &directory)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("start runtime"),
        )
    };
    let connect = |observe| {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            if let Ok(stream) = UnixStream::connect(&socket) {
                break stream;
            }
            assert!(Instant::now() < deadline, "runtime socket");
            std::thread::sleep(Duration::from_millis(10));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        codec::encode(
            &mut stream,
            &if observe {
                ClientMsg::ObserveRuntime
            } else {
                ClientMsg::AttachRuntime
            },
        )
        .expect("attach");
        stream
    };
    let mut runtime = launch();
    let mut owner = connect(false);
    let initial = read_tree(&mut owner);
    let mut viewer = connect(true);
    read_tree(&mut viewer);
    rename(&mut owner, "Build logs");
    assert_eq!(
        read_tree(&mut owner).workspaces[0].tabs[0].title,
        "Build logs"
    );
    assert_eq!(
        read_tree(&mut viewer).workspaces[0].tabs[0].title,
        "Build logs"
    );
    rename(&mut viewer, "Other person's name");
    codec::encode(
        &mut viewer,
        &ClientMsg::TerminalCapabilities {
            capabilities: seer_core::TerminalCapabilities {
                protocol_version: u16::MAX,
                raw_output: false,
            },
        },
    )
    .expect("barrier after read-only rename");
    loop {
        if let ServerMsg::Refused { .. } =
            codec::decode::<_, ServerMsg>(&mut viewer).expect("viewer reply")
        {
            break;
        }
    }
    let mut current = connect(false);
    assert_eq!(
        read_tree(&mut current).workspaces[0].tabs[0].title,
        "Build logs"
    );
    drop(current);
    loop {
        if let ServerMsg::Terminals { terminals, .. } =
            codec::decode::<_, ServerMsg>(&mut owner).expect("names")
        {
            let value = serde_json::to_value(&terminals[0]).expect("metadata");
            if value["title"] == "Build logs" {
                break;
            }
        }
    }
    drop(owner);
    drop(viewer);
    runtime.stop();
    let mut restarted = launch();
    let mut owner = connect(false);
    assert_eq!(
        read_tree(&mut owner).workspaces[0].tabs[0].title,
        "Build logs"
    );
    rename(&mut owner, &"é".repeat(41));
    loop {
        if let ServerMsg::Refused { .. } =
            codec::decode::<_, ServerMsg>(&mut owner).expect("limit reply")
        {
            break;
        }
    }
    rename(&mut owner, "");
    let cleared = read_tree(&mut owner);
    assert_eq!(cleared.workspaces[0].tabs[0].title, "");
    assert_eq!(
        cleared.workspaces[0].tabs[0].id,
        initial.workspaces[0].tabs[0].id
    );
    drop(owner);
    restarted.stop();
    std::fs::remove_dir_all(directory).expect("remove state");
}

fn rename(stream: &mut UnixStream, title: &str) {
    let bytes = serde_json::to_vec(
        &serde_json::json!({"RenameTab": {"workspace": "w1", "tab": "w1:t1", "title": title}}),
    )
    .expect("rename message");
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .expect("length");
    stream.write_all(&bytes).expect("body");
}

fn read_tree(stream: &mut UnixStream) -> Tree {
    loop {
        if let ServerMsg::Tree { tree } = codec::decode(stream).expect("tree reply") {
            return tree;
        }
    }
}

struct Running(Child);

impl Running {
    fn stop(&mut self) {
        // SAFETY: kill takes a process ID and a signal, and touches no memory.
        unsafe {
            libc::kill(self.0.id() as i32, libc::SIGTERM);
        }
        self.0.wait().expect("runtime stops");
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
