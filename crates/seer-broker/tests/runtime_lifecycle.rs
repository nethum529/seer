#![cfg(target_os = "linux")]

use std::fs;
use std::io::{self, Read};
use std::net::TcpStream;
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/binary.rs"]
pub mod binary;
#[path = "forwarding/extras.rs"]
pub mod extras;
#[path = "forwarding/lifecycle.rs"]
mod lifecycle;
#[path = "forwarding/support.rs"]
pub mod support;

use extras::send;
use support::{
    ProcessGuard, TestFiles, connect_when_ready, read_message, send_hello, wait_for_tree_with_tab,
};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn concurrent_first_attaches_share_one_ready_generation() {
    let temporary = TestFiles::new();
    let address = support::unused_address();
    extras::write_config(&temporary, address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);
    let barrier = Arc::new(Barrier::new(2));

    let clients = (0..2)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let mut stream = connect_when_ready(address);
                barrier.wait();
                send_hello(&mut stream, "alice", "alice-secret");
                assert!(matches!(
                    read_message(&mut stream),
                    ServerMsg::Welcome { .. }
                ));
                wait_for_tree_with_tab(&mut stream);
                stream
            })
        })
        .collect::<Vec<_>>();
    let clients = clients
        .into_iter()
        .map(|client| client.join().expect("concurrent attach must finish"))
        .collect::<Vec<_>>();

    assert_eq!(
        fs::read_to_string(temporary.root.join("alice.launches"))
            .expect("runtime launch log must be readable")
            .lines()
            .count(),
        1
    );
    let record = temporary.wait_for_runtime_state("alice", "running");
    assert_eq!(record["generation"].as_str().map(str::len), Some(32));
    assert_eq!(record["state"].as_str(), Some("running"));

    drop(clients);
    temporary.terminate_runtime("alice");
}

#[test]
fn failed_start_is_recorded_and_replacement_uses_a_new_generation() {
    let temporary = TestFiles::new();
    let address = support::unused_address();
    extras::write_config(&temporary, address);
    temporary.write_runtime_wrapper();
    fs::remove_file(&temporary.wrapper).expect("runtime wrapper must be removable");
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut failed = connect_when_ready(address);
    send_hello(&mut failed, "alice", "alice-secret");
    assert!(matches!(
        read_message(&mut failed),
        ServerMsg::Welcome { .. }
    ));
    assert!(matches!(
        read_message(&mut failed),
        ServerMsg::Grants { .. }
    ));
    assert!(matches!(
        read_message(&mut failed),
        ServerMsg::Refused { .. }
    ));
    let failed_record = temporary.wait_for_runtime_state("alice", "failed");
    let failed_generation = failed_record["generation"]
        .as_str()
        .expect("failed generation must be present")
        .to_owned();
    let reason = failed_record["last_reason"]
        .as_str()
        .expect("failed reason must be present");
    assert!(reason.len() <= 512);

    temporary.write_runtime_wrapper();
    let mut replacement = connect_when_ready(address);
    send_hello(&mut replacement, "alice", "alice-secret");
    assert!(matches!(
        read_message(&mut replacement),
        ServerMsg::Welcome { .. }
    ));
    wait_for_tree_with_tab(&mut replacement);
    let running_record = temporary.wait_for_runtime_state("alice", "running");
    assert_ne!(
        running_record["generation"].as_str(),
        Some(failed_generation.as_str())
    );

    drop(failed);
    drop(replacement);
    temporary.terminate_runtime("alice");
}

#[test]
fn stale_runtime_readiness_is_rejected_without_forwarding_input() {
    let temporary = TestFiles::new();
    let address = support::unused_address();
    extras::write_config(&temporary, address);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    assert!(matches!(
        read_message(&mut alice),
        ServerMsg::Welcome { .. }
    ));
    wait_for_tree_with_tab(&mut alice);
    let old_generation = temporary.runtime_record("alice")["generation"]
        .as_str()
        .expect("old generation must be present")
        .to_owned();
    let socket_path = temporary.terminate_runtime("alice");
    temporary.wait_for_runtime_state("alice", "failed");
    wait_for_path_removed(&socket_path);
    drop(alice);

    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    assert!(matches!(read_message(&mut bob), ServerMsg::Welcome { .. }));
    wait_for_tree_with_tab(&mut bob);

    let new_generation = "f".repeat(32);
    let record_path = temporary.state_dir.join("runtime-records/alice.json");
    let mut record = temporary.runtime_record("alice");
    record["generation"] = serde_json::Value::String(new_generation.clone());
    record["state"] = serde_json::Value::String("running".into());
    record["last_reason"] = serde_json::Value::Null;
    fs::write(
        record_path,
        serde_json::to_vec(&record).expect("runtime record must encode"),
    )
    .expect("runtime record must write");

    let listener = UnixListener::bind(&socket_path).expect("stale endpoint must bind");
    let (sender, receiver) = mpsc::channel();
    let fake = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("stale endpoint must accept");
        codec::encode(
            &mut stream,
            &ServerMsg::RuntimeReady {
                generation: old_generation,
            },
        )
        .expect("stale readiness must encode");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("stale endpoint timeout must set");
        let mut bytes = [0; 4];
        let no_client_input = match stream.read(&mut bytes) {
            Ok(0) => true,
            Ok(_) => false,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                true
            }
            Err(_) => false,
        };
        sender
            .send(no_client_input)
            .expect("stale endpoint result must send");
    });

    send(
        &mut bob,
        &ClientMsg::QueryTargets {
            user: "alice".into(),
        },
    );
    match wait_for_refused(&mut bob) {
        ServerMsg::Refused { reason } => assert!(reason.contains("generation")),
        other => panic!("expected refusal, got {other:?}"),
    }
    assert!(
        receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("stale endpoint must report")
    );
    fake.join().expect("stale endpoint thread must finish");

    drop(bob);
    temporary.terminate_runtime("bob");
}

fn wait_for_refused(stream: &mut TcpStream) -> ServerMsg {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        let message = read_message(stream);
        if matches!(message, ServerMsg::Refused { .. }) {
            return message;
        }
    }
    panic!("broker refusal was not received");
}

fn wait_for_path_removed(path: &std::path::Path) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if !path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("path was not removed: {}", path.display());
}
