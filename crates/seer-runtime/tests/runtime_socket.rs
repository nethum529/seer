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
use seer_core::{InputEvent, SplitDirection, TerminalFrame, TerminalInput};

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
    wait_for_close(&mut reattached);

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
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let mut runtime = RuntimeProcess::new(runtime);

    let mut owner = connect_with_timeout(&socket_path);
    let created = tree(read_message(&mut owner));
    assert_eq!(created.workspaces[0].tabs.len(), 1);
    assert_eq!(created.workspaces[0].tabs[0].panes.len(), 1);
    let first_tab = created.workspaces[0].tabs[0].id.clone();
    let first_pane = created.workspaces[0].tabs[0].panes[0].id.clone();
    assert!(wait_for_cells(&mut owner));

    let mut viewer = connect_with_timeout(&socket_path);
    let viewer_tree = tree(read_message(&mut viewer));
    assert_eq!(viewer_tree, created);
    assert!(wait_for_cells(&mut viewer));

    send_input(&mut owner, &first_pane, "printf 'owner-one\\n'\n");
    let _ = wait_for_cells_containing(&mut owner, "owner-one");
    let _ = wait_for_cells_containing(&mut viewer, "owner-one");

    send(
        &mut viewer,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
            tab: first_tab.clone(),
        },
    );
    assert_eq!(read_until_tree(&mut viewer), created);
    assert!(wait_for_cells(&mut viewer));
    send_input(&mut viewer, &first_pane, "printf 'viewer-input\\n'\n");

    send(
        &mut viewer,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
            tab: "w1:missing".into(),
        },
    );
    wait_for_refused(&mut viewer);

    send_input(&mut owner, &first_pane, "printf 'owner-two\\n'\n");
    let owner_cells = wait_for_cells_containing(&mut owner, "owner-two");
    let viewer_cells = wait_for_cells_containing(&mut viewer, "owner-two");
    assert!(!owner_cells.contains("viewer-input"));
    assert!(!viewer_cells.contains("viewer-input"));

    send(
        &mut owner,
        &ClientMsg::CreateTab {
            workspace: "w1".into(),
        },
    );
    let owner_with_second = read_until_tree(&mut owner);
    assert_eq!(owner_with_second.workspaces[0].tabs.len(), 2);
    let second_tab = owner_with_second.workspaces[0].tabs[1].id.clone();
    let second_pane = owner_with_second.workspaces[0].tabs[1].panes[0].id.clone();

    let viewer_selected = read_until_tree(&mut viewer);
    assert_eq!(viewer_selected.workspaces[0].name, created.workspaces[0].name);
    assert_eq!(viewer_selected.workspaces.len(), 1);
    assert_eq!(viewer_selected.workspaces[0].tabs.len(), 1);
    assert_eq!(viewer_selected.workspaces[0].tabs[0].id, first_tab);
    assert!(!viewer_selected.workspaces[0].tabs[0].panes.iter().any(|pane| {
        pane.id == second_pane
    }));

    send_input_at(
        &mut owner,
        "w1",
        &second_tab,
        &second_pane,
        "printf 'second-tab-output\\n'\n",
    );
    wait_for_pane_cells_containing(&mut owner, &second_pane, "second-tab-output");
    assert_no_message(&mut viewer, Some(&second_pane));

    for message in [
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: second_tab.clone(),
            cols: 200,
            rows: 50,
        },
        ClientMsg::FocusPane {
            workspace: "w1".into(),
            tab: second_tab.clone(),
            pane: second_pane.clone(),
        },
        ClientMsg::SplitPane {
            workspace: "w1".into(),
            tab: second_tab.clone(),
            direction: SplitDirection::Right,
        },
        ClientMsg::ClosePane {
            workspace: "w1".into(),
            tab: second_tab.clone(),
            pane: second_pane.clone(),
        },
        ClientMsg::CreateTab {
            workspace: "w1".into(),
        },
    ] {
        send(&mut viewer, &message);
    }
    send(
        &mut owner,
        &ClientMsg::FocusPane {
            workspace: "w1".into(),
            tab: first_tab.clone(),
            pane: first_pane.clone(),
        },
    );
    assert_eq!(read_until_tree(&mut owner), owner_with_second);
    assert_eq!(read_until_tree(&mut viewer), viewer_selected);

    send(
        &mut owner,
        &ClientMsg::Resize {
            workspace: "w1".into(),
            tab: first_tab.clone(),
            cols: 120,
            rows: 40,
        },
    );
    let owner_resized = read_until_tree(&mut owner);
    assert_eq!(
        owner_resized.workspaces[0].tabs[0].panes[0].size,
        seer_core::PaneSize {
            cols: 120,
            rows: 40
        }
    );
    assert_eq!(
        owner_resized.workspaces[0].tabs[1].panes[0].size,
        owner_with_second.workspaces[0].tabs[1].panes[0].size
    );
    let viewer_resized = read_until_tree(&mut viewer);
    assert_eq!(
        viewer_resized.workspaces[0].tabs[0].panes[0].size,
        seer_core::PaneSize {
            cols: 120,
            rows: 40
        }
    );

    send(
        &mut owner,
        &ClientMsg::ClosePane {
            workspace: "w1".into(),
            tab: first_tab,
            pane: first_pane,
        },
    );
    let remaining = read_until_tree(&mut owner);
    assert_eq!(remaining.workspaces[0].tabs.len(), 1);
    assert_eq!(remaining.workspaces[0].tabs[0].id, second_tab);
    wait_for_bye(&mut viewer);
    assert_no_message(&mut viewer, None);

    send_input_at(
        &mut owner,
        "w1",
        &second_tab,
        &second_pane,
        "printf 'owner-after-close\\n'\n",
    );
    wait_for_pane_cells_containing(&mut owner, &second_pane, "owner-after-close");
    assert_no_message(&mut viewer, None);

    send(&mut viewer, &ClientMsg::StopPeek);
    wait_for_close(&mut viewer);

    drop(owner);
    let _ = runtime.stop();
}

#[test]
fn removes_its_socket_on_sigterm() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let mut runtime = runtime_command()
        .args([socket_path.as_os_str(), "alice".as_ref(), "sh".as_ref()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let _connection = connect_when_ready(&socket_path);

    let pid = runtime.id() as libc::pid_t;
    // SAFETY: kill only reads the pid of a child this test started.
    let sent = unsafe { libc::kill(pid, libc::SIGTERM) };
    assert_eq!(sent, 0, "SIGTERM must be sent");

    let deadline = Instant::now() + Duration::from_secs(5);
    while socket_path.exists() && Instant::now() < deadline {
        thread::sleep(RETRY_INTERVAL);
    }
    assert!(!socket_path.exists(), "runtime must remove its socket");
    runtime.wait().expect("runtime must exit");
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

fn send_input_at(stream: &mut UnixStream, workspace: &str, tab: &str, pane: &str, input: &str) {
    send(
        stream,
        &ClientMsg::TerminalInput {
            workspace: workspace.into(),
            tab: tab.into(),
            pane: pane.into(),
            input: TerminalInput::new(InputEvent::Text(input.to_owned())),
        },
    );
}

fn wait_for_pane_cells_containing(stream: &mut UnixStream, pane: &str, expected: &str) {
    let start = Instant::now();
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "Cells did not contain {expected}"
        );
        if let ServerMsg::Cells {
            pane: message_pane,
            frame,
        } = read_message(stream)
            && message_pane == pane
            && frame_text(&frame).contains(expected)
        {
            return;
        }
    }
}

fn frame_text(frame: &TerminalFrame) -> String {
    frame.rows.iter().flatten().map(|cell| cell.character).collect()
}

fn wait_for_refused(stream: &mut UnixStream) {
    while !matches!(read_message(stream), ServerMsg::Refused { .. }) {}
}

fn wait_for_bye(stream: &mut UnixStream) {
    while !matches!(read_message(stream), ServerMsg::Bye { .. }) {}
}

fn assert_no_message(stream: &mut UnixStream, pane: Option<&str>) {
    let deadline = Instant::now() + Duration::from_millis(250);
    stream
        .set_read_timeout(Some(Duration::from_millis(25)))
        .expect("short read timeout must set");
    loop {
        match codec::decode::<_, ServerMsg>(stream) {
            Ok(message) if pane.is_none() => {
                panic!("unexpected message after peek ended: {message:?}");
            }
            Ok(ServerMsg::Cells { pane: message_pane, .. })
            | Ok(ServerMsg::Frame { pane: message_pane, .. })
                if pane.is_some_and(|expected| expected == message_pane) =>
            {
                panic!("peek received an update for pane {message_pane}");
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(error) => panic!("unexpected message read failure: {error}"),
        }
    }
    stream
        .set_read_timeout(Some(MESSAGE_TIMEOUT))
        .expect("message read timeout must restore");
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
