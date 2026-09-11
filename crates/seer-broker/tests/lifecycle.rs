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

#[test]
fn only_the_host_can_stop_the_room() {
    let room = Room::start(false);
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    send(&mut bob, &ClientMsg::Stop);
    let refused = wait_for(&mut bob, |m| matches!(m, ServerMsg::Refused { .. }));
    assert!(matches!(refused, ServerMsg::Refused { reason } if reason.contains("seer leave")));
    send(&mut alice, &ClientMsg::Invite { hours: None });
    wait_for(&mut alice, |m| matches!(m, ServerMsg::Seat { .. }));
    send(&mut alice, &ClientMsg::Stop);
    wait_for(&mut alice, |m| matches!(m, ServerMsg::Bye { .. }));
    let deadline = std::time::Instant::now() + WAIT;
    while std::net::TcpStream::connect(room.address).is_ok() {
        assert!(std::time::Instant::now() < deadline, "the room must stop");
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn bulk_permissions_change_only_the_callers_grants_for_current_people() {
    let room = Room::start(false);
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    send(&mut alice, &ClientMsg::SetAllGrants { can_type: true });
    wait_for(&mut alice, |m| matches!(m, ServerMsg::GrantsUpdated));
    send(&mut bob, &ClientMsg::SetAllGrants { can_type: true });
    wait_for(&mut bob, |m| matches!(m, ServerMsg::GrantsUpdated));
    send(&mut bob, &ClientMsg::SetAllGrants { can_type: false });
    let revoked = wait_for(&mut bob, |m| {
        matches!(m, ServerMsg::Grants { can_type_here, you_may_type_into }
        if can_type_here.is_empty() && you_may_type_into == &["alice"])
    });
    assert!(matches!(revoked, ServerMsg::Grants { .. }));
    wait_for(&mut bob, |m| matches!(m, ServerMsg::GrantsUpdated));
    send(&mut alice, &ClientMsg::Invite { hours: None });
    let ServerMsg::Seat { capsule, .. } =
        wait_for(&mut alice, |m| matches!(m, ServerMsg::Seat { .. }))
    else {
        panic!("the host must create an invite");
    };
    let token = capsule
        .rsplit('-')
        .next()
        .expect("invite must contain a token");
    let mut joining = std::net::TcpStream::connect(room.address).expect("room must accept");
    joining
        .set_read_timeout(Some(WAIT))
        .expect("timeout must set");
    send(
        &mut joining,
        &ClientMsg::Join {
            seat_token: token.into(),
            name: "charlie".into(),
        },
    );
    let ServerMsg::Joined {
        user_id,
        credential,
        ..
    } = wait_for(&mut joining, |m| matches!(m, ServerMsg::Joined { .. }))
    else {
        panic!("the new person must join");
    };
    let mut charlie = join_room(room.address, &user_id, &credential);
    wait_for(&mut charlie, |m| {
        matches!(m, ServerMsg::Grants { can_type_here, you_may_type_into }
        if can_type_here.is_empty() && you_may_type_into.is_empty())
    });
}
