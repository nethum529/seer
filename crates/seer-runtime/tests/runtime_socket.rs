#![cfg(target_os = "linux")]

use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Child, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

mod support;
use support::*;

#[test]
fn serves_cells_and_preserves_the_tree_after_disconnect() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let stale_listener = UnixListener::bind(&socket_path).expect("stale socket must bind");
    let stale_inode = fs::metadata(&socket_path)
        .expect("stale socket metadata must load")
        .ino();
    drop(stale_listener);

    let runtime = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let _runtime = RuntimeProcess::new(runtime);
    wait_for_socket_replacement(&socket_path, stale_inode);
    let mut stream = connect_when_ready(&socket_path);
    stream
        .set_read_timeout(Some(MESSAGE_TIMEOUT))
        .expect("read timeout must set");

    let created_tree = tree(read_message(&mut stream));
    assert_eq!(created_tree.workspaces[0].tabs.len(), 1);
    assert_eq!(created_tree.workspaces[0].tabs[0].panes.len(), 1);
    assert!(wait_for_cells(&mut stream));

    drop(stream);

    let mut reattached = connect_when_ready(&socket_path);
    reattached
        .set_read_timeout(Some(MESSAGE_TIMEOUT))
        .expect("read timeout must set");
    let reattached_tree = tree(read_message(&mut reattached));
    assert_eq!(reattached_tree.workspaces[0].tabs.len(), 1);
    assert!(wait_for_cells(&mut reattached));

    codec::encode(&mut reattached, &ClientMsg::Detach).expect("Detach must encode");
    let mut byte = [0];
    assert_eq!(
        reattached
            .read(&mut byte)
            .expect("runtime must close Detach"),
        0
    );

    let duplicate = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("duplicate runtime must start");
    let duplicate = wait_for_output(duplicate);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already in use"));
}

#[test]
fn restores_idle_cells_after_reattach() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let _runtime = RuntimeProcess::new(runtime);

    let mut attached = connect_with_timeout(&socket_path);
    let created = tree(read_message(&mut attached));
    let pane = created.workspaces[0].tabs[0].panes[0].id.clone();
    send_input(
        &mut attached,
        &pane,
        "printf '\\033[2J\\033[Hidle-reattach'; sleep 60\n",
    );
    let before_detach = wait_for_cells_containing(&mut attached, "idle-reattach");
    thread::sleep(Duration::from_millis(100));
    drop(attached);

    let mut reattached = connect_with_timeout(&socket_path);
    assert_eq!(tree(read_message(&mut reattached)), created);
    let after_reattach = wait_for_cells_containing(&mut reattached, "idle-reattach");
    assert_eq!(after_reattach, before_detach);
}

#[test]
fn broadcasts_to_concurrent_connections_and_blocks_peek_input() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("runtime must start");
    let mut runtime = RuntimeProcess::new(runtime);

    let mut owner = connect_with_timeout(&socket_path);
    let created = tree(read_message(&mut owner));
    assert_eq!(created.workspaces[0].tabs.len(), 1);
    assert_eq!(created.workspaces[0].tabs[0].panes.len(), 1);
    let pane = created.workspaces[0].tabs[0].panes[0].id.clone();
    assert!(wait_for_cells(&mut owner));

    let mut viewer = connect_with_timeout(&socket_path);
    let viewer_tree = tree(read_message(&mut viewer));
    assert_eq!(viewer_tree, created);
    assert!(wait_for_cells(&mut viewer));

    send_input(&mut owner, &pane, "printf 'owner-one\\n'\n");
    assert_cells_contain(&mut owner, "owner-one");
    assert_cells_contain(&mut viewer, "owner-one");

    send(
        &mut viewer,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
            tab: "w1:t1".into(),
        },
    );
    assert_eq!(read_until_tree(&mut viewer), created);
    assert!(wait_for_cells(&mut viewer));
    send_input(&mut viewer, &pane, "printf 'viewer-input\\n'\n");
    send(
        &mut viewer,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
            tab: "w1:t1".into(),
        },
    );
    assert_eq!(read_until_tree(&mut viewer), created);
    assert!(wait_for_cells(&mut viewer));

    send_input(&mut owner, &pane, "printf 'owner-two\\n'\n");
    let owner_cells = wait_for_cells_containing(&mut owner, "owner-two");
    let viewer_cells = wait_for_cells_containing(&mut viewer, "owner-two");
    assert!(!owner_cells.contains("viewer-input"));
    assert!(!viewer_cells.contains("viewer-input"));

    send(&mut viewer, &ClientMsg::StopPeek);
    wait_for_close(&mut viewer);
    send_input(&mut owner, &pane, "printf 'owner-three\\n'\n");
    assert_cells_contain(&mut owner, "owner-three");

    drop(owner);
    let output = runtime.stop();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.contains("runtime dropped read-only message"))
            .count(),
        1
    );
}

#[test]
fn rejects_wrong_argument_counts() {
    let cases: &[&[&str]] = &[
        &[],
        &["socket"],
        &["socket", "alice"],
        &["socket", "alice", "sh", "extra"],
    ];

    for arguments in cases {
        let child = runtime_command()
            .args(*arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("runtime must start");
        let output = wait_for_output(child);
        assert_usage_error(&output);
    }
}

fn wait_for_socket_replacement(path: &Path, stale_inode: u64) {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match fs::metadata(path) {
            Ok(metadata) if metadata.ino() != stale_inode => return,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => panic!("runtime socket metadata must load: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "runtime did not replace stale socket"
        );
        thread::sleep(RETRY_INTERVAL);
    }
}

fn assert_cells_contain(stream: &mut UnixStream, expected: &str) {
    let cells = wait_for_cells_containing(stream, expected);
    assert!(cells.contains(expected));
}

fn wait_for_cells_containing(stream: &mut UnixStream, expected: &str) -> String {
    let start = Instant::now();
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "Cells did not contain {expected}"
        );
        if let ServerMsg::Cells { frame, .. } = read_message(stream) {
            let text = frame
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.character)
                .collect::<String>();
            if text.contains(expected) {
                return text;
            }
        }
    }
}

fn wait_for_close(stream: &mut UnixStream) {
    let start = Instant::now();
    let mut bytes = [0; 1024];
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "connection did not close"
        );
        if stream.read(&mut bytes).expect("connection close must read") == 0 {
            return;
        }
    }
}

fn assert_usage_error(output: &Output) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("usage: seer-runtime <socket-path> <user> <shell>")
    );
}

fn wait_for_output(mut child: Child) -> Output {
    let start = Instant::now();
    loop {
        if child
            .try_wait()
            .expect("runtime status must be available")
            .is_some()
        {
            return child
                .wait_with_output()
                .expect("runtime output must be available");
        }
        if start.elapsed() >= PROCESS_TIMEOUT {
            let _ = child.kill();
            wait_until_exit(&mut child, "runtime did not stop after timeout");
            panic!("runtime did not exit within 2 seconds");
        }
        thread::sleep(RETRY_INTERVAL);
    }
}
