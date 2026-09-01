#![cfg(target_os = "linux")]

use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "forwarding/support.rs"]
mod support;

use support::{ProcessGuard, TestFiles};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[test]
fn forwards_to_a_lazy_runtime_and_preserves_its_tree() {
    let temporary = TestFiles::new();
    let address = unused_address();
    temporary.write_config(address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);
    let mut first = connect_when_ready(address);

    assert!(!temporary.pid_file("alice").is_file());
    send_hello(&mut first, "alice", "alice-secret");
    assert_welcome(read_message(&mut first), "alice");
    codec::encode(&mut first, &ClientMsg::CreateTab).expect("CreateTab must encode");
    wait_for_tree_with_tab(&mut first);
    assert!(wait_for_cells(&mut first));
    let runtime_pid = temporary.runtime_pid("alice");
    temporary.assert_runtime_arguments("alice");
    temporary.assert_socket_directory();
    send(&mut first, &ClientMsg::Invite);
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
    assert_welcome(read_message(&mut second), "alice");
    assert_tree_has_one_tab(read_message(&mut second));
    assert_eq!(temporary.runtime_pid("alice"), runtime_pid);

    drop(second);
    temporary.terminate_runtime("alice");
}

#[test]
fn routes_peek_and_restores_the_owners_runtime() {
    let temporary = TestFiles::new();
    let address = unused_address();
    temporary.write_config(address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    assert_welcome(read_message(&mut alice), "alice");
    send(&mut alice, &ClientMsg::CreateTab);
    let alice_tree = wait_for_tree_with_tab(&mut alice);
    assert!(wait_for_cells(&mut alice));
    let workspace = alice_tree.workspaces[0].id.clone();
    let pane = alice_tree.workspaces[0].tabs[0].panes[0].id.clone();

    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    assert_welcome(read_message(&mut bob), "bob");
    wait_for_empty_tree(&mut bob);
    send(&mut bob, &ClientMsg::Invite);
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
        },
    );
    send(&mut bob, &ClientMsg::Resize { cols: 90, rows: 30 });
    wait_for_empty_tree(&mut bob);
    assert!(!temporary.pid_file("charlie").is_file());

    send(
        &mut bob,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace,
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
    wait_for_empty_tree(&mut bob);
    temporary.assert_log_contains("broker dropped Peek for unknown user: charlie");
    temporary.assert_log_contains("broker dropped Input while user bob peeks");
    temporary.assert_log_excludes("runtime dropped read-only message");

    send(
        &mut bob,
        &ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
        },
    );
    wait_for_tree_with_tab(&mut bob);
    wait_for_tree_with_tab(&mut bob);
    temporary.terminate_runtime("alice");
    wait_for_empty_tree(&mut bob);
    wait_for_disconnect(&mut alice);

    drop(alice);
    drop(bob);
    temporary.terminate_runtime("bob");
}

fn send_hello(stream: &mut TcpStream, user: &str, token: &str) {
    codec::encode(
        stream,
        &ClientMsg::Hello {
            user_id: user.into(),
            credential: token.into(),
        },
    )
    .expect("Hello must encode");
}

fn send(stream: &mut TcpStream, message: &ClientMsg) {
    codec::encode(stream, message).expect("client message must encode");
}

fn send_input(stream: &mut TcpStream, pane: &str, input: &str) {
    send(
        stream,
        &ClientMsg::Input {
            pane: pane.into(),
            bytes: input.as_bytes().into(),
        },
    );
}

fn assert_welcome(message: ServerMsg, expected_user: &str) {
    match message {
        ServerMsg::Welcome {
            user_id,
            name,
            client_id,
            tree,
        } => {
            assert_eq!(user_id, expected_user);
            assert_eq!(name, expected_user);
            assert!(client_id.is_empty());
            assert!(tree.workspaces.is_empty());
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
}

fn wait_for_tree_with_tab(stream: &mut TcpStream) -> seer_core::Tree {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if let ServerMsg::Tree { tree } = read_message(stream)
            && tree
                .workspaces
                .first()
                .is_some_and(|workspace| workspace.tabs.len() == 1)
        {
            return tree;
        }
    }
    panic!("Tree with a tab was not received");
}

fn wait_for_cells(stream: &mut TcpStream) -> bool {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(read_message(stream), ServerMsg::Cells { .. }) {
            return true;
        }
    }
    false
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

fn wait_for_empty_tree(stream: &mut TcpStream) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if let ServerMsg::Tree { tree } = read_message(stream)
            && tree.workspaces.is_empty()
        {
            return;
        }
    }
    panic!("empty Tree was not received");
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
        if let ServerMsg::Cells { rows, .. } = read_message(stream) {
            let text = rows
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

fn read_message(stream: &mut TcpStream) -> ServerMsg {
    stream
        .set_read_timeout(Some(WAIT_TIMEOUT))
        .expect("read timeout must set");
    codec::decode(stream).expect("server message must decode")
}

fn wait_for_disconnect(stream: &mut TcpStream) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let mut bytes = [0; 1_024];
    while Instant::now() < deadline {
        match stream.read(&mut bytes) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::UnexpectedEof
                ) =>
            {
                return;
            }
            Err(error) => panic!("Alice must disconnect: {error}"),
        }
    }
    panic!("Alice did not disconnect");
}

fn unused_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port probe must bind");
    listener
        .local_addr()
        .expect("port probe must have an address")
}

fn connect_when_ready(address: SocketAddr) -> TcpStream {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let mut last_error = None;
    while Instant::now() < deadline {
        match TcpStream::connect(address) {
            Ok(stream) => return stream,
            Err(error) => last_error = Some(error),
        }
        thread::sleep(POLL_INTERVAL);
    }
    panic!("broker did not listen: {last_error:?}");
}
