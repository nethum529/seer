#![cfg(target_os = "linux")]

use std::fs;
use std::net::TcpStream;
use std::time::Instant;

use seer_core::proto::{ClientMsg, ServerMsg, codec};

mod binary {
    pub fn build(binary: &str) -> std::path::PathBuf {
        match binary {
            "seer-broker" => env!("CARGO_BIN_EXE_seer-broker").into(),
            "seer-runtime" => env!("CARGO_BIN_EXE_seer-runtime").into(),
            _ => panic!("unknown test binary"),
        }
    }
}
#[path = "support/cli_run.rs"]
mod cli_run;
#[path = "../../seer-broker/tests/room/mod.rs"]
mod room;
#[path = "../../seer-broker/tests/room/screen.rs"]
mod screen;

use cli_run::{TestConfig, run, text};
use room::*;
use screen::*;

#[test]
fn host_removes_one_person_without_stopping_their_local_shell() {
    let mut room = Room::start(false);
    let host_config = saved_person(&room, "alice", "alice", ALICE_SECRET);
    let bob_config = saved_person(&room, "bob", "bob", BOB_SECRET);
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    let token = invite(&mut alice);
    let (guest_id, secret) = join_seat(&room, &token, "Guest");
    let guest_config = saved_person(&room, "Guest", &guest_id, &secret);
    let mut guest = join_room(room.address, &guest_id, &secret);
    let mut other_guest = join_room(room.address, &guest_id, &secret);
    let mut guest_window = room.publish(&guest_id, &secret);
    let at = first_terminal(&own_tree(&mut guest_window));
    let mut bob_window = room.publish("bob", BOB_SECRET);
    let bob_at = first_terminal(&own_tree(&mut bob_window));
    wait_for_published(&mut alice, &guest_id);
    wait_for_published(&mut alice, "bob");
    check_invalid_removals(&room, &host_config, &bob_config, &guest_id);
    grant(&mut guest, "bob");
    grant(&mut bob, &guest_id);
    grant(&mut bob, "alice");
    watch(&mut guest, "bob", &bob_at.pane);
    watch(&mut alice, &guest_id, &at.pane);
    watch(&mut bob, &guest_id, &at.pane);
    let before = room.root.join("shell-before");
    type_locally(
        &mut guest_window,
        &at,
        &format!("printf '%s' \"$$\" > '{}'\n", before.display()),
    );
    wait_for_file(&before);
    remote_write(&mut guest, "bob", &bob_at.pane, "from-guest", &room);
    remote_write(&mut bob, &guest_id, &at.pane, "from-bob", &room);

    let removed = run(&host_config, &["remove", "Guest"], "");
    assert!(removed.status.success(), "{}", text(&removed.stderr));
    assert!(text(&removed.stdout).contains("Guest"));
    wait_for(&mut bob, |message| {
        matches!(message, ServerMsg::Grants { can_type_here, you_may_type_into }
        if can_type_here == &["alice"] && you_may_type_into.is_empty())
    });
    for stream in [&mut alice, &mut bob] {
        wait_for(stream, |message| {
            matches!(message, ServerMsg::People { people }
            if people.len() == 2 && !people.iter().any(|p| p.user_id == guest_id))
        });
    }
    for stream in [&mut guest, &mut other_guest] {
        wait_for(
            stream,
            |message| matches!(message, ServerMsg::Bye { reason } if reason == "removed by host"),
        );
        assert_closed(stream);
    }
    send(
        &mut bob,
        &ClientMsg::TypeInto {
            user: guest_id.clone(),
            pane: at.pane.clone(),
            bytes: b"echo forbidden\n".to_vec(),
        },
    );
    wait_for(&mut bob, |m| matches!(m, ServerMsg::Refused { .. }));
    let after = room.root.join("shell-after");
    type_locally(
        &mut guest_window,
        &at,
        &format!("printf '%s' \"$$\" > '{}'\n", after.display()),
    );
    wait_for_file(&after);
    assert_eq!(fs::read(before).unwrap(), fs::read(after).unwrap());
    assert_removed_message(&guest_config);
    assert_used_seat(&room, &token);
    let old_line = format!("SEER1-127.0.0.1-{}-{token}", room.address.port());
    let refused_join = run(&guest_config, &["join", &old_line], "Guest\n");
    assert!(!refused_join.status.success());
    room.stop_broker();
    room.start_broker();
    assert_removed_message(&guest_config);
    assert_used_seat(&room, &token);
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    wait_for(&mut bob, |message| {
        matches!(message, ServerMsg::Grants { can_type_here, you_may_type_into }
        if can_type_here == &["alice"] && you_may_type_into.is_empty())
    });
    let new_token = invite(&mut alice);
    let (new_id, new_secret) = join_seat(&room, &new_token, "Guest");
    assert_ne!(new_id, guest_id);
    let mut new_guest = join_room(room.address, &new_id, &new_secret);
    wait_for(&mut new_guest, |message| {
        matches!(message, ServerMsg::Grants { can_type_here, you_may_type_into }
        if can_type_here.is_empty() && you_may_type_into.is_empty())
    });
}

fn check_invalid_removals(room: &Room, host: &TestConfig, bob: &TestConfig, guest_id: &str) {
    for (config, name, words) in [
        (bob, "Guest", "Only the host"),
        (host, "alice", "host"),
        (host, "Nobody", "Nobody"),
    ] {
        let output = run(config, &["remove", name], "");
        assert!(!output.status.success());
        assert!(
            text(&output.stderr).contains(words),
            "{}",
            text(&output.stderr)
        );
        let mut client = join_room(room.address, "alice", ALICE_SECRET);
        send(&mut client, &ClientMsg::ListPeople);
        wait_for(&mut client, |message| {
            matches!(message, ServerMsg::People { people }
            if people.len() == 3 && people.iter().any(|p| p.user_id == guest_id))
        });
    }
}

fn saved_person(room: &Room, name: &str, id: &str, credential: &str) -> TestConfig {
    let config = TestConfig::new();
    let directory = config.root.join("seer");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("servers.toml"), format!(
        "[[servers]]\nendpoint = \"{}\"\nalias = \"Test\"\nuser_id = \"{id}\"\nname = \"{name}\"\ncredential = \"{credential}\"\ncurrent = true\n", room.address
    )).unwrap();
    config
}

fn invite(host: &mut TcpStream) -> String {
    send(host, &ClientMsg::Invite { hours: None });
    let ServerMsg::Seat { capsule, .. } = wait_for(host, |m| matches!(m, ServerMsg::Seat { .. }))
    else {
        panic!("host must get an invitation");
    };
    capsule.rsplit('-').next().unwrap().to_owned()
}

fn seat_connection(room: &Room, token: &str, name: &str) -> TcpStream {
    let mut stream = TcpStream::connect(room.address).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    send(
        &mut stream,
        &ClientMsg::Join {
            seat_token: token.into(),
            name: name.into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        },
    );
    stream
}

fn join_seat(room: &Room, token: &str, name: &str) -> (String, String) {
    let mut stream = seat_connection(room, token, name);
    let ServerMsg::Joined {
        user_id,
        credential,
        ..
    } = decode(&mut stream)
    else {
        panic!("new invitation must let Guest join");
    };
    (user_id, credential)
}

fn assert_used_seat(room: &Room, token: &str) {
    let mut stream = seat_connection(room, token, "Guest");
    assert!(matches!(decode(&mut stream), ServerMsg::Refused { .. }));
}

fn assert_removed_message(config: &TestConfig) {
    let output = run(config, &["invite"], "");
    assert!(!output.status.success());
    let message = text(&output.stderr);
    assert!(message.contains("host removed you"), "{message}");
}

fn grant(owner: &mut TcpStream, user: &str) {
    send(
        owner,
        &ClientMsg::SetGrant {
            user: user.into(),
            can_type: true,
        },
    );
    wait_for(
        owner,
        |m| matches!(m, ServerMsg::Grants { can_type_here, .. } if can_type_here.iter().any(|u| u == user)),
    );
}

fn watch(client: &mut TcpStream, user: &str, pane: &str) {
    send(
        client,
        &ClientMsg::Watch {
            user: user.into(),
            pane: pane.into(),
            cols: 80,
            rows: 24,
            viewer: true,
        },
    );
    wait_for(
        client,
        |m| matches!(m, ServerMsg::Cells { frame, .. } if !frame_text(frame).is_empty()),
    );
}

fn remote_write(client: &mut TcpStream, user: &str, pane: &str, name: &str, room: &Room) {
    let path = room.root.join(name);
    send(
        client,
        &ClientMsg::TypeInto {
            user: user.into(),
            pane: pane.into(),
            bytes: format!("echo received > '{}'\n", path.display()).into_bytes(),
        },
    );
    wait_for_file(&path);
}

fn wait_for_file(path: &std::path::Path) {
    let deadline = Instant::now() + WAIT;
    while !fs::read(path).is_ok_and(|bytes| !bytes.is_empty()) {
        assert!(
            Instant::now() < deadline,
            "shell must write {}",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn assert_closed(stream: &mut TcpStream) {
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(200)))
        .unwrap();
    loop {
        assert!(
            Instant::now() < deadline,
            "removed person's connection must close"
        );
        match codec::decode::<_, ServerMsg>(stream) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return,
            Err(error) => panic!("connection did not close: {error}"),
        }
    }
}
