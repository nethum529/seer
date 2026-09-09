use std::fs;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{SplitDirection, Tree};

use crate::support::*;

const SNAPSHOT_FILE: &str = "session.json";
const SNAPSHOT_DIR_VAR: &str = "SEER_SNAPSHOT_DIR";
const GENERATION: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn cold_restart_restores_topology_and_corrupt_snapshots_start_safely() {
    let temporary = TemporaryDirectory::new();
    let state = temporary.path.join("state");
    fs::create_dir(&state).expect("state directory must be created");
    let snapshot_path = state.join(SNAPSHOT_FILE);
    let socket_path = temporary.path.join("runtime.sock");

    let mut runtime = spawn_runtime(&socket_path, &state);
    let mut owner = connect_with_timeout(&socket_path);
    let initial = read_tree(&mut owner);
    assert_eq!(
        (
            initial.workspaces[0].tabs.len(),
            initial.workspaces[0].tabs[0].panes.len()
        ),
        (1, 1)
    );
    assert!(snapshot_path.exists(), "first shell must be saved");

    let workspace = initial.workspaces[0].id.clone();
    let tab = initial.workspaces[0].tabs[0].id.clone();
    send(
        &mut owner,
        &ClientMsg::Resize {
            workspace: workspace.clone(),
            tab: tab.clone(),
            cols: 81,
            rows: 25,
        },
    );
    let _resized = read_until_tree(&mut owner);
    send(
        &mut owner,
        &ClientMsg::SplitPane {
            workspace: workspace.clone(),
            tab: tab.clone(),
            direction: SplitDirection::Right,
        },
    );
    let split_tree = read_until_tree(&mut owner);
    assert_eq!(split_tree.workspaces[0].tabs[0].panes.len(), 2);
    let first_pane = split_tree.workspaces[0].tabs[0].panes[0].id.clone();
    let second_pane = split_tree.workspaces[0].tabs[0].panes[1].id.clone();
    assert_ne!(first_pane, second_pane);

    let first_pid = marked_pid(&mut owner, &first_pane).expect("first shell PID must be read");
    let second_pid = marked_pid(&mut owner, &second_pane).expect("second shell PID must be read");
    assert_ne!(first_pid, second_pid, "each pane must have its own shell");

    drop(owner);
    let mut reattached = connect_with_timeout(&socket_path);
    let live_tree = read_tree(&mut reattached);
    assert_eq!(live_tree, split_tree, "live reattach must keep the tree");
    let live_pid = marked_pid(&mut reattached, &first_pane).expect("live PID must be read");
    assert_eq!(
        live_pid, first_pid,
        "live detach and reattach must keep the original shell"
    );
    drop(reattached);
    runtime.stop();

    let mut restarted = spawn_runtime(&socket_path, &state);
    let mut restored_client = connect_with_timeout(&socket_path);
    let restored = read_tree(&mut restored_client);
    assert_eq!(
        restored, split_tree,
        "cold restart must restore the saved topology without duplicates"
    );
    let restored_first = marked_pid(&mut restored_client, &first_pane)
        .expect("restored first shell PID must be read");
    let restored_second = marked_pid(&mut restored_client, &second_pane)
        .expect("restored second shell PID must be read");
    assert_ne!(
        restored_first, first_pid,
        "restored shell must be a fresh process"
    );
    assert_ne!(
        restored_second, second_pid,
        "restored shell must be a fresh process"
    );
    drop(restored_client);
    restarted.stop();

    let valid_text = fs::read_to_string(&snapshot_path).expect("valid snapshot must be readable");
    fs::write(&snapshot_path, b"{\"version\":1,\"revision\":7,\"tree\":")
        .expect("truncated snapshot must write");
    let mut corrupt_runtime = spawn_runtime(&socket_path, &state);
    let mut corrupt_client = connect_with_timeout(&socket_path);
    let safe = read_tree(&mut corrupt_client);
    assert_eq!(
        (
            safe.workspaces[0].tabs.len(),
            safe.workspaces[0].tabs[0].panes.len()
        ),
        (1, 1),
        "corrupt snapshot must start a safe default session"
    );
    assert!(
        wait_for_cells(&mut corrupt_client),
        "default shell must run"
    );
    drop(corrupt_client);
    corrupt_runtime.stop();

    fs::write(
        &snapshot_path,
        valid_text.replacen("\"version\":1", "\"version\":99", 1),
    )
    .expect("unsupported snapshot must write");
    let mut unsupported_runtime = spawn_runtime(&socket_path, &state);
    let mut unsupported_client = connect_with_timeout(&socket_path);
    let safe_again = read_tree(&mut unsupported_client);
    assert_eq!(
        (
            safe_again.workspaces[0].tabs.len(),
            safe_again.workspaces[0].tabs[0].panes.len()
        ),
        (1, 1),
        "unsupported snapshot version must start a safe default session"
    );
    assert!(
        wait_for_cells(&mut unsupported_client),
        "default shell must run"
    );
    drop(unsupported_client);
    unsupported_runtime.stop();

    fs::write(
        &snapshot_path,
        rewrite_number(&valid_text, "next_tab_id", "1"),
    )
    .expect("colliding counter snapshot must write");
    let mut counter_runtime = spawn_runtime(&socket_path, &state);
    let mut counter_client = connect_with_timeout(&socket_path);
    let safe_counter = read_tree(&mut counter_client);
    assert_eq!(
        (
            safe_counter.workspaces[0].tabs.len(),
            safe_counter.workspaces[0].tabs[0].panes.len()
        ),
        (1, 1),
        "snapshot that reuses a tab id must start a safe default session"
    );
    assert!(
        wait_for_cells(&mut counter_client),
        "default shell must run"
    );
    drop(counter_client);
    counter_runtime.stop();

    fs::write(
        &snapshot_path,
        valid_text.replacen("\"id\":\"w1:p1\"", "\"id\":\"\"", 1),
    )
    .expect("empty pane id snapshot must write");
    let mut empty_pane_runtime = spawn_runtime(&socket_path, &state);
    let mut empty_pane_client = connect_with_timeout(&socket_path);
    let safe_empty_pane = read_tree(&mut empty_pane_client);
    assert_eq!(
        (
            safe_empty_pane.workspaces[0].tabs.len(),
            safe_empty_pane.workspaces[0].tabs[0].panes.len()
        ),
        (1, 1),
        "snapshot with an empty pane id must start a safe default session"
    );
    assert!(
        wait_for_cells(&mut empty_pane_client),
        "default shell must run"
    );
    drop(empty_pane_client);
    empty_pane_runtime.stop();
}

#[test]
fn snapshot_save_failure_stops_before_a_queued_mutation() {
    let temporary = TemporaryDirectory::new();
    let state = temporary.path.join("state");
    fs::create_dir(&state).expect("state directory must be created");
    let socket_path = temporary.path.join("runtime.sock");

    let mut runtime = spawn_runtime(&socket_path, &state);
    let mut first = connect_with_timeout(&socket_path);
    let initial = read_tree(&mut first);
    let workspace = initial.workspaces[0].id.clone();
    let tab = initial.workspaces[0].tabs[0].id.clone();
    let mut second = connect_with_timeout(&socket_path);
    assert_eq!(read_tree(&mut second), initial);

    let failed_write = state.join(".session.json.tmp");
    let status = Command::new("mkfifo")
        .arg(&failed_write)
        .status()
        .expect("mkfifo must run");
    assert!(status.success(), "snapshot failure FIFO must be created");

    send(
        &mut first,
        &ClientMsg::Resize {
            workspace: workspace.clone(),
            tab,
            cols: 81,
            rows: 25,
        },
    );
    send(&mut second, &ClientMsg::CreateTab { workspace });

    let mut fifo = fs::File::open(&failed_write).expect("snapshot writer must open the FIFO");
    io::copy(&mut fifo, &mut io::sink()).expect("snapshot bytes must drain");
    let output = runtime.wait_for_exit();
    assert!(!output.status.success(), "snapshot failure must be fatal");

    let mut restarted = spawn_runtime(&socket_path, &state);
    let mut client = connect_with_timeout(&socket_path);
    assert_eq!(
        read_tree(&mut client),
        initial,
        "a queued mutation must not replace the last durable snapshot"
    );
    drop(client);
    restarted.stop();
}

fn spawn_runtime(socket_path: &Path, state_dir: &Path) -> RuntimeProcess {
    let child = runtime_command()
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            "sh".as_ref(),
            GENERATION.as_ref(),
        ])
        .env(SNAPSHOT_DIR_VAR, state_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    RuntimeProcess::new(child)
}

fn read_tree(stream: &mut UnixStream) -> Tree {
    tree(read_message(stream))
}

fn rewrite_number(json: &str, key: &str, replacement: &str) -> String {
    let marker = format!("\"{key}\":");
    let start = json
        .find(&marker)
        .unwrap_or_else(|| panic!("json key must exist: {key}"));
    let digits_start = start + marker.len();
    let rest = &json[digits_start..];
    let digits_end = rest
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(rest.len());
    format!(
        "{}{}{}",
        &json[..digits_start],
        replacement,
        &rest[digits_end..]
    )
}

fn marked_pid(stream: &mut UnixStream, pane: &str) -> Option<u32> {
    send_input(stream, pane, "printf 'MARK:%s\\n' $$\n");
    let start = Instant::now();
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "marked shell PID was not received for {pane}"
        );
        match read_message(stream) {
            ServerMsg::Cells {
                user: _,
                pane: cell_pane,
                frame,
            } if cell_pane == pane => {
                let text = frame
                    .rows
                    .iter()
                    .flatten()
                    .map(|cell| cell.character)
                    .collect::<String>();
                if let Some(pid) = pid_after_mark(&text) {
                    return Some(pid);
                }
            }
            _ => {}
        }
    }
}

fn pid_after_mark(text: &str) -> Option<u32> {
    let mut search_from = 0;
    while let Some(mark) = text[search_from..].find("MARK:") {
        let digits_start = search_from + mark + "MARK:".len();
        let digits = text[digits_start..]
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
        search_from = digits_start + 1;
    }
    None
}
