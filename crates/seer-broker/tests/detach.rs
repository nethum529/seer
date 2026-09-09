#![cfg(target_os = "linux")]

use seer_core::proto::{ClientMsg, ServerMsg};

#[path = "support/binary.rs"]
mod binary;
mod room;
use room::*;

// Detaching reaches only your own windows, and it does not touch the shells.
#[test]
fn a_person_detaches_only_their_own_client_and_keeps_their_terminals() {
    let mut room = Room::start();
    let mut alice_window = room.publish("alice", ALICE_SECRET);
    let tree = own_tree(&mut alice_window);
    let pane = tree.workspaces[0].tabs[0].panes[0].id.clone();

    let mut first = join_room(room.address, "alice", ALICE_SECRET);
    let mut second = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    wait_for_published(&mut first, "alice");

    send(
        &mut first,
        &ClientMsg::DetachClient {
            client_id: String::new(),
        },
    );
    let listed = wait_for(&mut first, |message| {
        matches!(message, ServerMsg::Clients { .. })
    });
    let ServerMsg::Clients { clients } = listed else {
        panic!("the room must list this person's other windows");
    };
    let other = clients
        .first()
        .expect("a second window must be listed")
        .client_id
        .clone();

    send(
        &mut bob,
        &ClientMsg::DetachClient {
            client_id: other.clone(),
        },
    );
    let refused = wait_for(&mut bob, |message| {
        matches!(message, ServerMsg::Refused { .. })
    });
    let ServerMsg::Refused { reason } = refused else {
        panic!("a refusal must carry a reason");
    };
    assert!(
        reason.contains("does not belong"),
        "one person must not detach another person's window: {reason}"
    );

    send(&mut first, &ClientMsg::DetachClient { client_id: other });
    wait_for(&mut second, |message| {
        matches!(message, ServerMsg::Bye { .. })
    });

    assert!(
        still_listed(&mut first, &pane),
        "detaching a window must leave the terminals running"
    );
}

fn still_listed(stream: &mut std::net::TcpStream, pane: &str) -> bool {
    send(
        stream,
        &ClientMsg::Terminals {
            user: "alice".into(),
        },
    );
    let listed = wait_for(
        stream,
        |message| matches!(message, ServerMsg::Terminals { user, terminals } if user == "alice" && !terminals.is_empty()),
    );
    let ServerMsg::Terminals { terminals, .. } = listed else {
        return false;
    };
    terminals.iter().any(|terminal| terminal.pane == pane)
}
