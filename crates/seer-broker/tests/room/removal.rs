use super::revocation::{pending_request, pending_viewer, publish_delayed};
use super::*;
use std::io::{ErrorKind, Read};
use std::net::TcpStream;

#[test]
fn removal_stops_live_control_even_when_grants_cannot_be_saved() {
    departure_stops_live_control(false);
}

#[test]
fn leave_stops_live_control_even_when_grants_cannot_be_saved() {
    departure_stops_live_control(true);
}

fn departure_stops_live_control(leaving: bool) {
    let mut room = Room::start(false);
    let mut window = room.publish_to("127.0.0.1:0", "alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut window));
    let mut publisher = publish_delayed(&room);
    let mut bob_runtime = TcpStream::connect(room.address).unwrap();
    bob_runtime.set_read_timeout(Some(WAIT)).unwrap();
    send(
        &mut bob_runtime,
        &ClientMsg::PublishRuntime {
            user_id: "bob".into(),
            credential: BOB_SECRET.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            generation: "bob-runtime".into(),
        },
    );
    wait_for(&mut bob_runtime, |m| {
        matches!(m, ServerMsg::Published { .. })
    });
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    let mut other_bob = join_room(room.address, "bob", BOB_SECRET);
    send(
        &mut alice,
        &ClientMsg::SetAllGrants {
            can_type: true,
            request_id: None,
        },
    );
    wait_for_grant(&mut bob, true);
    send(
        &mut other_bob,
        &ClientMsg::SetAllGrants {
            can_type: true,
            request_id: None,
        },
    );
    wait_for(&mut other_bob, |m| *m == ServerMsg::GrantsUpdated);
    send(
        &mut bob,
        &ClientMsg::Terminals {
            user: "alice".into(),
        },
    );
    let mut stalled = pending_viewer(&mut publisher, room.address);
    let mut local = UnixStream::connect(room.root.join("alice/socket")).unwrap();
    let ready: ServerMsg = codec::decode(&mut local).unwrap();
    codec::encode(&mut stalled, &ready).unwrap();
    send(&mut local, &ClientMsg::ObserveRuntime);
    for _ in 0..2 {
        let message: ServerMsg = codec::decode(&mut local).unwrap();
        codec::encode(&mut stalled, &message).unwrap();
    }
    terminals(&mut bob, "alice");
    send(
        &mut bob,
        &ClientMsg::InputInto {
            user: "alice".into(),
            pane: at.pane,
            input: seer_core::TerminalInput::new(seer_core::InputEvent::Paste(
                "x".repeat(8 * 1024 * 1024),
            )),
        },
    );
    stalled.set_read_timeout(Some(WAIT)).unwrap();
    let mut prefix = vec![0; 64 * 1024];
    stalled.read_exact(&mut prefix).unwrap();
    send(
        &mut bob,
        &ClientMsg::QueryTargets {
            user: "alice".into(),
        },
    );
    let _held_query = pending_request(
        &mut publisher,
        room.address,
        ClientMsg::QueryTargets {
            user: "alice".into(),
        },
    );
    let fault = room.root.join("state/grants/.alice.json.tmp");
    std::fs::create_dir(&fault).unwrap();
    let caller = if leaving { &mut other_bob } else { &mut alice };
    send(
        caller,
        &if leaving {
            ClientMsg::Leave
        } else {
            ClientMsg::RemovePerson { name: "bob".into() }
        },
    );
    wait_for(caller, |m| matches!(m, ServerMsg::Refused { .. }));
    let length = u32::from_be_bytes(prefix[..4].try_into().unwrap()) as usize;
    let mut tail = vec![0; length + 4 - prefix.len()];
    let result = stalled.read_exact(&mut tail);
    assert_eq!(
        result
            .expect_err("a departed person must not complete a live input frame")
            .kind(),
        ErrorKind::UnexpectedEof
    );
    assert_closed(&mut bob);
    if !leaving {
        assert_closed(&mut other_bob);
    }
    assert_closed(&mut bob_runtime);
    let mut reconnected = join_room(room.address, "alice", ALICE_SECRET);
    assert_no_grants(&mut reconnected);
    send(&mut reconnected, &ClientMsg::ListPeople);
    wait_for(
        &mut reconnected,
        |m| matches!(m, ServerMsg::People { people } if people.len() == 1 && people[0].user_id == "alice"),
    );
    std::fs::remove_dir(fault).unwrap();
    room.stop_broker();
    room.start_broker();
    let mut restored = join_room(room.address, "alice", ALICE_SECRET);
    assert_no_grants(&mut restored);
}

fn assert_closed(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.read_to_end(&mut Vec::new()).unwrap();
}

fn assert_no_grants(stream: &mut TcpStream) {
    let grants = wait_for(stream, |m| matches!(m, ServerMsg::Grants { .. }));
    assert!(
        matches!(grants, ServerMsg::Grants { can_type_here, you_may_type_into }
        if can_type_here.is_empty() && you_may_type_into.is_empty())
    );
}
