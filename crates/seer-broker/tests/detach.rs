#![cfg(target_os = "linux")]

use std::net::TcpStream;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{InputEvent, TerminalInput};

#[path = "support/binary.rs"]
mod binary;
#[path = "forwarding/extras.rs"]
mod extras;
#[path = "forwarding/support.rs"]
mod support;

use extras::{
    assert_log_contains, assert_log_excludes, assert_process_running, assert_runtime_arguments,
    assert_socket_directory, pane_pid, send, wait_for_cells, write_config,
};
use support::{
    ProcessGuard, TestFiles, connect_when_ready, read_message, send_hello, unused_address,
    wait_for_disconnect, wait_for_tree_with_tab, welcome_client_id,
};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn detaches_own_client_refuses_another_person_and_keeps_the_pane() {
    let temporary = TestFiles::new();
    let address = unused_address();
    write_config(&temporary, address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    let alice_client = welcome_client_id(read_message(&mut alice), "alice");
    drop(wait_for_tree_with_tab(&mut alice));
    assert!(wait_for_cells(&mut alice));
    send(
        &mut alice,
        &ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
            input: TerminalInput::new(InputEvent::Text(
                "sh -c 'printf \"%s\\n\" \"$PPID\" > \"$SEER_TEST_FILES/alice-pane.pid\"'\n".into(),
            )),
        },
    );
    let pane_pid = pane_pid(&temporary, "alice");
    let runtime_pid = temporary.runtime_pid("alice");

    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    let bob_client = welcome_client_id(read_message(&mut bob), "bob");
    assert_ne!(alice_client, bob_client);
    send(
        &mut bob,
        &ClientMsg::DetachClient {
            client_id: alice_client.clone(),
        },
    );
    assert_eq!(
        wait_for(&mut bob, |message| matches!(
            message,
            ServerMsg::Refused { .. }
        )),
        ServerMsg::Refused {
            reason: "client does not belong to this person".into()
        }
    );
    assert_log_contains(&temporary, "broker refused DetachClient for user bob");
    assert_log_excludes(&temporary, "runtime dropped read-only message");
    assert_runtime_arguments(&temporary, "alice");
    assert_socket_directory(&temporary);

    let mut controller = connect_when_ready(address);
    send_hello(&mut controller, "alice", "alice-secret");
    let controller_id = welcome_client_id(read_message(&mut controller), "alice");
    assert_ne!(alice_client, controller_id);
    send(
        &mut controller,
        &ClientMsg::DetachClient {
            client_id: String::new(),
        },
    );
    let clients = wait_for(&mut controller, |message| {
        matches!(message, ServerMsg::Clients { .. })
    });
    let ServerMsg::Clients { clients } = clients else {
        panic!("expected Clients");
    };
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].client_id, alice_client);
    send(
        &mut controller,
        &ClientMsg::DetachClient {
            client_id: alice_client,
        },
    );
    assert_eq!(
        wait_for(&mut alice, |message| matches!(
            message,
            ServerMsg::Bye { .. }
        )),
        ServerMsg::Bye {
            reason: "detached".into()
        }
    );
    wait_for_disconnect(&mut alice);

    assert_eq!(temporary.runtime_pid("alice"), runtime_pid);
    assert_process_running(pane_pid);
    let mut reattached = connect_when_ready(address);
    send_hello(&mut reattached, "alice", "alice-secret");
    welcome_client_id(read_message(&mut reattached), "alice");
    drop(wait_for_tree_with_tab(&mut reattached));
    assert_eq!(temporary.runtime_pid("alice"), runtime_pid);
    assert_process_running(pane_pid);

    drop(reattached);
    drop(controller);
    drop(bob);
    temporary.terminate_runtime("alice");
    temporary.terminate_runtime("bob");
}

fn wait_for(stream: &mut TcpStream, expected: impl Fn(&ServerMsg) -> bool) -> ServerMsg {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        let message = read_message(stream);
        if expected(&message) {
            return message;
        }
    }
    panic!("expected server message was not received");
}
