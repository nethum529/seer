use super::revocation::{pending_viewer, publish_delayed, start_capture};
use super::*;
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};

const RESPONSIVE: Duration = Duration::from_secs(1);

#[test]
fn a_stalled_target_does_not_block_other_owners_and_revocation_stops_queued_input() {
    let mut room = Room::start(false);
    let mut alice_window = room.publish_to("127.0.0.1:0", "alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut alice_window));
    let alice_capture = start_capture(&room, &mut alice_window, &at, "alice");
    let mut bob_window = room.publish("bob", BOB_SECRET);
    let bt = first_terminal(&own_tree(&mut bob_window));
    let bob_capture = start_capture(&room, &mut bob_window, &bt, "bob");
    let mut publisher = publish_delayed(&room);
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    let mut bob_control = join_room(room.address, "bob", BOB_SECRET);
    wait_for_published(&mut alice, "bob");
    set_grant(&mut alice, "bob", true);
    wait_for_grant(&mut bob, true);

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
            pane: at.pane.clone(),
            input: seer_core::TerminalInput::new(seer_core::InputEvent::Paste(
                "x".repeat(8 * 1024 * 1024),
            )),
        },
    );
    stalled.set_read_timeout(Some(WAIT)).unwrap();
    let mut prefix = vec![0; 64 * 1024];
    stalled.read_exact(&mut prefix).unwrap();
    let stalled_at = Instant::now();
    send(
        &mut bob,
        &ClientMsg::TypeInto {
            user: "alice".into(),
            pane: at.pane.clone(),
            bytes: b"Queued451".to_vec(),
        },
    );
    send(
        &mut bob,
        &ClientMsg::DetachClient {
            client_id: String::new(),
        },
    );

    set_grant(&mut bob_control, "alice", true);
    responsive_reply(
        &mut bob_control,
        |m| matches!(m, ServerMsg::Grants { can_type_here, .. } if can_type_here.contains(&"alice".into())),
    );
    responsive_reply(
        &mut alice,
        |m| matches!(m, ServerMsg::Grants { you_may_type_into, .. } if you_may_type_into.contains(&"bob".into())),
    );
    send(
        &mut alice,
        &ClientMsg::InputInto {
            user: "bob".into(),
            pane: bt.pane,
            input: seer_core::TerminalInput::new(seer_core::InputEvent::Text("Healthy451".into())),
        },
    );
    let deadline = Instant::now() + RESPONSIVE;
    while Instant::now() < deadline && fs::read(&bob_capture).unwrap() != b"Healthy451" {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fs::read(&bob_capture).unwrap(), b"Healthy451");
    responsive_reply(&mut bob, |m| matches!(m, ServerMsg::Clients { .. }));

    set_grant(&mut alice, "bob", false);
    responsive_reply(
        &mut bob,
        |m| matches!(m, ServerMsg::Grants { you_may_type_into, .. } if !you_may_type_into.contains(&"alice".into())),
    );
    set_grant(&mut alice, "bob", true);
    responsive_reply(
        &mut bob,
        |m| matches!(m, ServerMsg::Grants { you_may_type_into, .. } if you_may_type_into.contains(&"alice".into())),
    );
    assert!(
        stalled_at.elapsed() < RESPONSIVE,
        "the revoke must finish before the stalled send times out"
    );
    stalled.set_read_timeout(Some(RESPONSIVE)).unwrap();
    stalled.read_to_end(&mut prefix).unwrap();
    let result = codec::decode::<_, ClientMsg>(&mut prefix.as_slice());
    assert!(
        result.is_err(),
        "revocation must cancel the stalled input frame"
    );
    assert_eq!(
        result.unwrap_err().kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    local.write_all(&prefix).unwrap();
    local.shutdown(Shutdown::Write).unwrap();
    local.set_read_timeout(Some(RESPONSIVE)).unwrap();
    local.read_to_end(&mut Vec::new()).unwrap();
    assert!(fs::read(&alice_capture).unwrap().is_empty());
}

fn set_grant(stream: &mut TcpStream, user: &str, can_type: bool) {
    send(
        stream,
        &ClientMsg::SetGrant {
            user: user.into(),
            can_type,
        },
    );
}

fn responsive_reply(stream: &mut TcpStream, matches: impl Fn(&ServerMsg) -> bool) {
    let started = Instant::now();
    stream.set_read_timeout(Some(RESPONSIVE)).unwrap();
    loop {
        let message: ServerMsg = codec::decode(stream)
            .expect("another owner must get a reply while the target is stalled");
        if matches(&message) {
            break;
        }
        assert!(started.elapsed() < RESPONSIVE);
    }
    assert!(
        started.elapsed() < RESPONSIVE,
        "a stalled target must not delay another owner"
    );
    stream.set_read_timeout(Some(WAIT)).unwrap();
}
