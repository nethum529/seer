#![cfg(target_os = "linux")]

use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/binary.rs"]
mod binary;
#[path = "forwarding/support.rs"]
mod support;

use support::{ProcessGuard, TestFiles};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[test]
fn detaches_own_client_refuses_another_person_and_keeps_the_pane() {
    let temporary = TestFiles::new();
    let address = unused_address();
    temporary.write_config(address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    let alice_client = welcome_client_id(read_message(&mut alice), "alice");
    wait_for_tree_with_tab(&mut alice);
    wait_for_cells(&mut alice);
    send(
        &mut alice,
        &ClientMsg::Input {
            pane: "w1:p1".into(),
            bytes: b"printf '%s\\n' \"$$\" > \"$SEER_TEST_FILES/alice-pane.pid\"\n".to_vec(),
        },
    );
    let pane_pid = temporary.pane_pid("alice");
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
    temporary.assert_log_contains("broker refused DetachClient for user bob");
    temporary.assert_log_excludes("runtime dropped read-only message");
    temporary.assert_runtime_arguments("alice");
    temporary.assert_socket_directory();

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
    temporary.assert_process_running(pane_pid);
    let mut reattached = connect_when_ready(address);
    send_hello(&mut reattached, "alice", "alice-secret");
    welcome_client_id(read_message(&mut reattached), "alice");
    wait_for_tree_with_tab(&mut reattached);
    assert_eq!(temporary.runtime_pid("alice"), runtime_pid);
    temporary.assert_process_running(pane_pid);

    drop(reattached);
    drop(controller);
    drop(bob);
    temporary.terminate_runtime("alice");
    temporary.terminate_runtime("bob");
}

fn send_hello(stream: &mut TcpStream, user: &str, credential: &str) {
    send(
        stream,
        &ClientMsg::Hello {
            user_id: user.into(),
            credential: credential.into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
    );
}

fn send(stream: &mut TcpStream, message: &ClientMsg) {
    codec::encode(stream, message).expect("client message must encode");
}

fn welcome_client_id(message: ServerMsg, expected_user: &str) -> String {
    let ServerMsg::Welcome {
        user_id, client_id, ..
    } = message
    else {
        panic!("expected Welcome");
    };
    assert_eq!(user_id, expected_user);
    assert_eq!(client_id.len(), 32);
    assert!(client_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    client_id
}

fn wait_for_tree_with_tab(stream: &mut TcpStream) {
    wait_for(stream, |message| {
        matches!(
            message,
            ServerMsg::Tree { tree }
                if tree.workspaces.first().is_some_and(|workspace| workspace.tabs.len() == 1)
        )
    });
}

fn wait_for_cells(stream: &mut TcpStream) {
    wait_for(stream, |message| matches!(message, ServerMsg::Cells { .. }));
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
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => panic!("client disconnect failed: {error}"),
        }
    }
    panic!("client did not disconnect");
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
