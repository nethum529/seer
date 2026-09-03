#![cfg(target_os = "linux")]

use std::net::TcpStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{InputEvent, TerminalInput};

#[path = "support/binary.rs"]
mod binary;
#[path = "forwarding/support.rs"]
mod support;
#[path = "forwarding/extras.rs"]
mod extras;

use support::{
    ProcessGuard, TestFiles, connect_when_ready, read_message, send_hello, unused_address,
    wait_for_disconnect, wait_for_tree_with_tab, welcome_client_id,
};
use extras::{
    assert_log_contains, assert_log_excludes, assert_process_running, assert_runtime_arguments,
    assert_socket_directory, pane_pid, send, wait_for_cells, write_config,
};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[test]
fn forwards_to_a_lazy_runtime_and_preserves_its_tree() {
    let temporary = TestFiles::new();
    let address = unused_address();
    write_config(&temporary, address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);
    let mut first = connect_when_ready(address);

    assert!(!temporary.pid_file("alice").is_file());
    send_hello(&mut first, "alice", "alice-secret");
    drop(welcome_client_id(read_message(&mut first), "alice"));
    let tree = wait_for_tree_with_tab(&mut first);
    assert!(wait_for_cells(&mut first));
    let pane = &tree.workspaces[0].tabs[0].panes[0].id;
    send_input(
        &mut first,
        pane,
        "sh -c 'printf \"%s\\n\" \"$PPID\" > \"$SEER_TEST_FILES/alice-pane.pid\"'\n",
    );
    assert_process_running(pane_pid(&temporary, "alice"));
    let runtime_pid = temporary.runtime_pid("alice");
    assert_runtime_arguments(&temporary, "alice");
    assert_socket_directory(&temporary);
    send(&mut first, &ClientMsg::Invite { hours: None });
    match wait_for_seat(&mut first) {
        ServerMsg::Seat {
            capsule,
            expires_in_secs,
        } => {
            assert!(capsule.starts_with("SEER1-host-7321-"));
            assert_eq!(expires_in_secs, 3_600);
        }
        other => panic!("expected Seat, got {other:?}"),
    }
    send(&mut first, &ClientMsg::ListPeople);
    match wait_for_people(&mut first) {
        ServerMsg::People { people } => {
            let alice = people
                .iter()
                .find(|person| person.user_id == "alice")
                .expect("Alice must be listed");
            let bob = people
                .iter()
                .find(|person| person.user_id == "bob")
                .expect("Bob must be listed");
            assert_eq!(alice.attached_clients, 1);
            assert!(alice.peekable);
            assert_eq!(bob.attached_clients, 0);
            assert!(!bob.peekable);
        }
        other => panic!("expected People, got {other:?}"),
    }

    drop(first);

    let mut second = connect_when_ready(address);
    send_hello(&mut second, "alice", "alice-secret");
    drop(welcome_client_id(read_message(&mut second), "alice"));
    assert_tree_has_one_tab(read_message(&mut second));
    assert_eq!(temporary.runtime_pid("alice"), runtime_pid);

    drop(second);
    temporary.terminate_runtime("alice");
}

#[test]
fn routes_peek_and_restores_the_owners_runtime() {
    let temporary = TestFiles::new();
    let address = unused_address();
    write_config(&temporary, address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    drop(welcome_client_id(read_message(&mut alice), "alice"));
    let alice_tree = wait_for_tree_with_tab(&mut alice);
    assert!(wait_for_cells(&mut alice));
    let workspace = alice_tree.workspaces[0].id.clone();
    let pane = alice_tree.workspaces[0].tabs[0].panes[0].id.clone();

    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    drop(welcome_client_id(read_message(&mut bob), "bob"));
    wait_for_tree_with_tab(&mut bob);
    send(&mut bob, &ClientMsg::Invite { hours: None });
    assert_eq!(
        wait_for_refused(&mut bob),
        ServerMsg::Refused {
            reason: "owner access required".into()
        }
    );

    send(
        &mut bob,
        &ClientMsg::Peek {
            user: "charlie".into(),
            workspace: workspace.clone(),
            tab: "w1:t1".into(),
        },
    );
    send(
        &mut bob,
        &ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 90,
            rows: 30,
        },
    );
    wait_for_tree_with_tab(&mut bob);
    assert!(!temporary.pid_file("charlie").is_file());

    send(
        &mut bob,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace,
            tab: "w1:t1".into(),
        },
    );
    assert_eq!(wait_for_tree_with_tab(&mut bob), alice_tree);
    send_input(&mut alice, &pane, "printf 'alice-before\\n'\n");
    assert!(wait_for_cells_containing(&mut alice, "alice-before").contains("alice-before"));
    assert!(wait_for_cells_containing(&mut bob, "alice-before").contains("alice-before"));

    send_input(&mut bob, &pane, "printf 'bob-write\\n'\n");
    send_input(&mut alice, &pane, "printf 'alice-after\\n'\n");
    let alice_cells = wait_for_cells_containing(&mut alice, "alice-after");
    let bob_cells = wait_for_cells_containing(&mut bob, "alice-after");
    assert!(!alice_cells.contains("bob-write"));
    assert!(!bob_cells.contains("bob-write"));

    send(&mut bob, &ClientMsg::StopPeek);
    wait_for_tree_with_tab(&mut bob);
    assert_log_contains(&temporary, "broker dropped Peek for unknown user: charlie");
    assert_log_contains(&temporary, "broker dropped Input while user bob peeks");
    assert_log_excludes(&temporary, "runtime dropped read-only message");

    send(
        &mut bob,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
            tab: "w1:t1".into(),
        },
    );
    wait_for_tree_with_tab(&mut bob);
    wait_for_tree_with_tab(&mut bob);
    temporary.terminate_runtime("alice");
    wait_for_tree_with_tab(&mut bob);
    wait_for_disconnect(&mut alice);

    drop(alice);
    drop(bob);
    temporary.terminate_runtime("bob");
}

#[test]
fn supervises_an_exited_runtime_and_starts_a_replacement() {
    let temporary = TestFiles::new();
    let address = unused_address();
    write_config(&temporary, address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    drop(welcome_client_id(read_message(&mut alice), "alice"));
    wait_for_tree_with_tab(&mut alice);
    let alice_pid = temporary.runtime_pid("alice");

    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    drop(welcome_client_id(read_message(&mut bob), "bob"));
    wait_for_tree_with_tab(&mut bob);
    let bob_pid = temporary.runtime_pid("bob");

    let alice_socket = temporary.terminate_runtime("alice");
    assert_path_removed(format!("/proc/{alice_pid}/status"));
    assert_path_removed(&alice_socket);
    assert_process_running(bob_pid);

    let mut replacement = connect_when_ready(address);
    send_hello(&mut replacement, "alice", "alice-secret");
    drop(welcome_client_id(read_message(&mut replacement), "alice"));
    wait_for_tree_with_tab(&mut replacement);
    assert_ne!(temporary.runtime_pid("alice"), alice_pid);
    assert_process_running(bob_pid);

    drop(alice);
    drop(bob);
    drop(replacement);
    temporary.terminate_runtime("alice");
    temporary.terminate_runtime("bob");
}

fn send_input(stream: &mut TcpStream, pane: &str, input: &str) {
    send(
        stream,
        &ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: pane.into(),
            input: TerminalInput::new(InputEvent::Text(input.into())),
        },
    );
}

fn assert_tree_has_one_tab(message: ServerMsg) {
    match message {
        ServerMsg::Tree { tree } => {
            assert_eq!(tree.workspaces.len(), 1);
            assert_eq!(tree.workspaces[0].tabs.len(), 1);
        }
        other => panic!("expected Tree, got {other:?}"),
    }
}

fn wait_for_seat(stream: &mut TcpStream) -> ServerMsg {
    wait_for_broker_message(stream, |message| matches!(message, ServerMsg::Seat { .. }))
}

fn wait_for_people(stream: &mut TcpStream) -> ServerMsg {
    wait_for_broker_message(stream, |message| {
        matches!(message, ServerMsg::People { .. })
    })
}

fn wait_for_refused(stream: &mut TcpStream) -> ServerMsg {
    wait_for_broker_message(stream, |message| {
        matches!(message, ServerMsg::Refused { .. })
    })
}

fn wait_for_broker_message(
    stream: &mut TcpStream,
    expected: impl Fn(&ServerMsg) -> bool,
) -> ServerMsg {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        let message = read_message(stream);
        if expected(&message) {
            return message;
        }
    }
    panic!("broker response was not received");
}

fn wait_for_cells_containing(stream: &mut TcpStream, expected: &str) -> String {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
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
    panic!("Cells did not contain {expected}");
}

fn assert_path_removed(path: impl AsRef<Path>) {
    let path = path.as_ref();
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if !path.exists() {
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
    panic!("path was not removed: {}", path.display());
}
