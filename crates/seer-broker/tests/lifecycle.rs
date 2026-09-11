#![cfg(target_os = "linux")]

use seer_core::proto::{ClientMsg, ServerMsg};

#[path = "support/binary.rs"]
mod binary;
mod room;
use room::*;

#[test]
fn leaving_removes_membership_and_keeps_the_other_persons_room() {
    let mut room = Room::start(false);
    let mut bob_window = room.publish("bob", BOB_SECRET);
    own_tree(&mut bob_window);
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    let mut other_bob = join_room(room.address, "bob", BOB_SECRET);
    wait_for_published(&mut alice, "bob");
    send(&mut bob, &ClientMsg::Leave);
    wait_for(&mut bob, |m| matches!(m, ServerMsg::Bye { .. }));
    wait_for(&mut other_bob, |m| matches!(m, ServerMsg::Bye { .. }));
    send(&mut alice, &ClientMsg::ListPeople);
    wait_for(&mut alice, |m| {
        matches!(m, ServerMsg::People { people }
        if people.iter().any(|p| p.user_id == "alice") && !people.iter().any(|p| p.user_id == "bob"))
    });
    let mut denied = std::net::TcpStream::connect(room.address).expect("room must stay up");
    denied
        .set_read_timeout(Some(WAIT))
        .expect("timeout must set");
    send(
        &mut denied,
        &ClientMsg::Hello {
            user_id: "bob".into(),
            credential: BOB_SECRET.into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
    );
    wait_for(&mut denied, |m| matches!(m, ServerMsg::Refused { .. }));
    send(&mut alice, &ClientMsg::Invite { hours: None });
    wait_for(&mut alice, |m| matches!(m, ServerMsg::Seat { .. }));
}
