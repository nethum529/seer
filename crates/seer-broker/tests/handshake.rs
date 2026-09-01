use std::fs;
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use seer_broker::{Config, serve};
use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use sha2::{Digest, Sha256};

#[test]
fn handles_required_handshake_outcomes() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    let (config, state_dir) = test_config(address);
    let _server = thread::spawn(move || serve(listener, &config));

    let (_, welcome) = exchange(
        address,
        &ClientMsg::Hello {
            user_id: "u-alice".into(),
            credential: "alice-secret".into(),
        },
    );
    assert_eq!(
        welcome,
        ServerMsg::Welcome {
            user_id: "u-alice".into(),
            name: "Alice".into(),
            client_id: String::new(),
            tree: Tree::new(),
        }
    );

    let (bad_token_stream, bad_token) = exchange(
        address,
        &ClientMsg::Hello {
            user_id: "u-alice".into(),
            credential: "wrong".into(),
        },
    );
    assert_refused_and_closed(bad_token_stream, bad_token, "invalid credentials");

    let (non_hello_stream, non_hello) = exchange(address, &ClientMsg::Detach);
    assert_refused_and_closed(non_hello_stream, non_hello, "expected Hello");
    fs::remove_dir_all(state_dir).expect("state directory must be removed");
}

#[test]
fn handles_a_second_connection_while_the_first_is_silent() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    let (config, state_dir) = test_config(address);
    let _server = thread::spawn(move || serve(listener, &config));
    let _silent = TcpStream::connect(address).expect("silent client must connect");

    let (stream, response) = exchange(
        address,
        &ClientMsg::Hello {
            user_id: "u-alice".into(),
            credential: "wrong".into(),
        },
    );

    assert_refused_and_closed(stream, response, "invalid credentials");
    fs::remove_dir_all(state_dir).expect("state directory must be removed");
}

#[test]
fn joins_with_single_use_seats_and_preserves_a_colliding_seat() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    let (config, state_dir) = test_config(address);
    let _server = thread::spawn(move || serve(listener, &config));

    let (_, joined) = exchange(
        address,
        &ClientMsg::Join {
            seat_token: "seat-one".into(),
            name: "Guest_1".into(),
        },
    );
    let joined_user = match joined {
        ServerMsg::Joined {
            user_id,
            credential,
            name,
        } => {
            assert_eq!(user_id.len(), 32);
            assert_eq!(credential.len(), 64);
            assert_eq!(name, "Guest_1");
            user_id
        }
        other => panic!("expected Joined, got {other:?}"),
    };

    let (used_stream, used) = exchange(
        address,
        &ClientMsg::Join {
            seat_token: "seat-one".into(),
            name: "Other".into(),
        },
    );
    assert_refused_and_closed(used_stream, used, "invalid seat");

    let (expired_stream, expired) = exchange(
        address,
        &ClientMsg::Join {
            seat_token: "expired-seat".into(),
            name: "Late".into(),
        },
    );
    assert_refused_and_closed(expired_stream, expired, "invalid seat");

    let (collision_stream, collision) = exchange(
        address,
        &ClientMsg::Join {
            seat_token: "seat-two".into(),
            name: "aLiCe".into(),
        },
    );
    assert_refused_and_closed(collision_stream, collision, "name is in use");
    let (_, after_collision) = exchange(
        address,
        &ClientMsg::Join {
            seat_token: "seat-two".into(),
            name: "Guest-2".into(),
        },
    );
    assert!(matches!(after_collision, ServerMsg::Joined { .. }));

    let people: serde_json::Value = serde_json::from_slice(
        &fs::read(state_dir.join("people.json")).expect("people registry must read"),
    )
    .expect("people registry must decode");
    assert!(
        people
            .as_array()
            .expect("people registry must be an array")
            .iter()
            .any(|person| person["user_id"] == joined_user)
    );
    fs::remove_dir_all(state_dir).expect("state directory must be removed");
}

fn test_config(listen: SocketAddr) -> (Config, PathBuf) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be valid")
        .as_nanos()
        % 1_000_000_000;
    let state_dir = PathBuf::from(format!("/tmp/sh-{}-{timestamp}", std::process::id()));
    fs::create_dir(&state_dir).expect("state directory must be created");
    let alice_hash = hash("alice-secret");
    let bob_hash = hash("bob-secret");
    let seat_one_hash = hash("seat-one");
    let seat_two_hash = hash("seat-two");
    let expired_hash = hash("expired-seat");
    let people = format!(
        "[{{\"user_id\":\"u-alice\",\"name\":\"Alice\",\"credential_hash\":\"{alice_hash}\",\"created_at\":1,\"is_owner\":true}},{{\"user_id\":\"u-bob\",\"name\":\"Bob\",\"credential_hash\":\"{bob_hash}\",\"created_at\":2,\"is_owner\":false}}]\n"
    );
    fs::write(state_dir.join("people.json"), people).expect("people registry must write");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be valid")
        .as_secs();
    let expires = now + 3_600;
    let seats = format!(
        "[{{\"token_hash\":\"{seat_one_hash}\",\"expires_at\":{expires},\"used\":false}},{{\"token_hash\":\"{seat_two_hash}\",\"expires_at\":{expires},\"used\":false}},{{\"token_hash\":\"{expired_hash}\",\"expires_at\":{now},\"used\":false}}]\n"
    );
    fs::write(state_dir.join("seats.json"), seats).expect("seat registry must write");
    (
        Config {
            listen,
            published_addr: "host:7321".into(),
            state_dir: state_dir.clone(),
            owner_name: "Owner".into(),
            shell: "sh".into(),
        },
        state_dir,
    )
}

fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn exchange(address: SocketAddr, request: &ClientMsg) -> (TcpStream, ServerMsg) {
    let mut stream = TcpStream::connect(address).expect("client must connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("read timeout must set");
    codec::encode(&mut stream, request).expect("request must encode");
    let response = codec::decode(&mut stream).expect("response must decode");
    (stream, response)
}

fn assert_refused_and_closed(mut stream: TcpStream, response: ServerMsg, reason: &str) {
    assert_eq!(
        response,
        ServerMsg::Refused {
            reason: reason.into()
        }
    );

    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("read timeout must set");
    let mut byte = [0];
    assert_eq!(stream.read(&mut byte).expect("connection must close"), 0);
}
