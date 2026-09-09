use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixListener;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::SplitDirection;
use seer_core::proto::{ClientMsg, codec};

use crate::runtime_socket_helpers::*;
use crate::support::*;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";

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
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            "sh".as_ref(),
            GENERATION.as_ref(),
        ])
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
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            "sh".as_ref(),
            GENERATION.as_ref(),
        ])
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
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            "sh".as_ref(),
            GENERATION.as_ref(),
        ])
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
    wait_for_cells_containing(&mut attached, "idle-reattach");
    drop(attached);

    let mut reattached = connect_with_timeout(&socket_path);
    assert_eq!(tree(read_message(&mut reattached)), created);
    wait_for_cells_containing(&mut reattached, "idle-reattach");
}

#[test]
fn broadcasts_to_concurrent_connections() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            "sh".as_ref(),
            GENERATION.as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let mut runtime = RuntimeProcess::new(runtime);

    let mut owner = connect_with_timeout(&socket_path);
    let created = read_until_tree(&mut owner);
    assert_eq!(created.workspaces[0].tabs.len(), 1);
    assert_eq!(created.workspaces[0].tabs[0].panes.len(), 1);
    let first_tab = created.workspaces[0].tabs[0].id.clone();
    let first_pane = created.workspaces[0].tabs[0].panes[0].id.clone();
    assert!(wait_for_cells(&mut owner));

    let mut viewer = connect_viewer(&socket_path);
    let viewer_tree = tree(read_message(&mut viewer));
    assert_eq!(viewer_tree, created);
    assert!(wait_for_cells(&mut viewer));

    send_input(&mut owner, &first_pane, "printf 'owner-one\\n'\n");
    let _ = wait_for_cells_containing(&mut owner, "owner-one");
    let _ = wait_for_cells_containing(&mut viewer, "owner-one");

    send(
        &mut viewer,
        &ClientMsg::Watch {
            user: "alice".into(),
            pane: first_pane.clone(),
            cols: 80,
            rows: 24,
        },
    );
    send_input(&mut viewer, &first_pane, "printf 'viewer-input\\n'\n");

    send(
        &mut viewer,
        &ClientMsg::Watch {
            user: "alice".into(),
            pane: "w1:missing".into(),
            cols: 80,
            rows: 24,
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
    assert_eq!(
        viewer_selected.workspaces[0].name,
        created.workspaces[0].name
    );
    assert_eq!(viewer_selected.workspaces.len(), 1);
    assert_eq!(viewer_selected.workspaces[0].tabs.len(), 1);
    assert_eq!(viewer_selected.workspaces[0].tabs[0].id, first_tab);
    assert!(
        !viewer_selected.workspaces[0].tabs[0]
            .panes
            .iter()
            .any(|pane| { pane.id == second_pane })
    );

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
    // The owner does not watch this pane, so the viewer that watches it keeps the geometry.
    let owner_resized = read_until_tree(&mut owner);
    assert_eq!(
        owner_resized.workspaces[0].tabs[0].panes[0].size,
        seer_core::PaneSize { cols: 80, rows: 24 }
    );
    assert_eq!(
        owner_resized.workspaces[0].tabs[1].panes[0].size,
        owner_with_second.workspaces[0].tabs[1].panes[0].size
    );
    let viewer_resized = read_until_tree(&mut viewer);
    assert_eq!(
        viewer_resized.workspaces[0].tabs[0].panes[0].size,
        seer_core::PaneSize { cols: 80, rows: 24 }
    );

    send(
        &mut owner,
        &ClientMsg::ClosePane {
            workspace: "w1".into(),
            tab: first_tab,
            pane: first_pane,
        },
    );
    let remaining = read_until_tree_with_tab_count(&mut owner, 1);
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

    send(&mut viewer, &ClientMsg::Detach);
    wait_for_close(&mut viewer);

    drop(owner);
    let _ = runtime.stop();
}

#[test]
fn removes_its_socket_on_sigterm() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let mut runtime = runtime_command()
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            "sh".as_ref(),
            GENERATION.as_ref(),
        ])
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
        &["socket", "alice", "sh"],
        &["socket", "alice", "sh", "generation", "extra"],
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
