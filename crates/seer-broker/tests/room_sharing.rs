#![cfg(target_os = "linux")]

use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, TerminalInfo, codec};

#[path = "support/binary.rs"]
mod binary;
mod room;
use room::*;

// Issue 338: Alice's shells run on Alice's computer. Bob sees them through the
// room and may type only while Alice allows it.
#[test]
fn a_watcher_sees_another_persons_terminals_and_types_only_with_a_grant() {
    let mut room = Room::start(false);
    let mut alice_window = room.publish("alice", ALICE_SECRET);
    let tree = own_tree(&mut alice_window);
    let pane = tree.workspaces[0].tabs[0].panes[0].id.clone();

    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    wait_for_published(&mut alice, "alice");

    send(
        &mut bob,
        &ClientMsg::Terminals {
            user: "alice".into(),
        },
    );
    let listed = terminals(&mut bob, "alice");
    assert!(
        listed.iter().any(|terminal| terminal.pane == pane),
        "the room must list the terminals running on Alice's computer"
    );

    send(
        &mut bob,
        &ClientMsg::Watch {
            user: "alice".into(),
            pane: pane.clone(),
            cols: 80,
            rows: 24,
        },
    );
    wait_for(
        &mut bob,
        |message| matches!(message, ServerMsg::Cells { pane: shown, .. } if *shown == pane),
    );

    send(
        &mut bob,
        &ClientMsg::TypeInto {
            user: "alice".into(),
            pane: pane.clone(),
            bytes: b"echo ungranted\n".to_vec(),
        },
    );
    let refused = wait_for(&mut bob, |message| {
        matches!(message, ServerMsg::Refused { .. })
    });
    let ServerMsg::Refused { reason } = refused else {
        panic!("a refusal must carry a reason");
    };
    assert!(
        reason.contains("has not let you type"),
        "typing without a grant must be refused: {reason}"
    );

    send(
        &mut alice,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: true,
        },
    );
    wait_for_grant(&mut bob, true);
    send(
        &mut bob,
        &ClientMsg::TypeInto {
            user: "alice".into(),
            pane: pane.clone(),
            bytes: b"echo granted\n".to_vec(),
        },
    );
    assert!(
        cells_contain(&mut bob, &pane, "granted"),
        "a granted watcher's keys must reach the terminal on Alice's computer"
    );

    send(
        &mut alice,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: false,
        },
    );
    wait_for_grant(&mut bob, false);
    send(
        &mut bob,
        &ClientMsg::TypeInto {
            user: "alice".into(),
            pane: pane.clone(),
            bytes: b"echo revoked\n".to_vec(),
        },
    );
    wait_for(&mut bob, |message| {
        matches!(message, ServerMsg::Refused { .. })
    });
}

// A second computer must never take over or stop the work on the first one.
#[test]
fn a_second_computer_is_refused_and_the_first_keeps_working() {
    let mut room = Room::start(false);
    let mut alice_window = room.publish("alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut alice_window));
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    wait_for_published(&mut alice, "alice");

    let mut second = std::net::TcpStream::connect(room.address).expect("room must accept");
    second
        .set_read_timeout(Some(WAIT))
        .expect("read timeout must set");
    send(
        &mut second,
        &ClientMsg::PublishRuntime {
            user_id: "alice".into(),
            credential: ALICE_SECRET.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            generation: "gen-second".into(),
        },
    );
    let reply: ServerMsg = codec::decode(&mut second).expect("the room must answer");
    let ServerMsg::Refused { reason } = reply else {
        panic!("a second computer must be refused, got {reply:?}");
    };
    assert!(
        reason.contains("already publishes"),
        "the refusal must name the cause: {reason}"
    );

    type_locally(&mut alice_window, &at, "echo still mine\n");
    assert!(
        local_cells_contain(&mut alice_window, &at.pane, "still mine"),
        "the first computer must keep working after refusing a second"
    );
}

// The room is for sharing. Losing it must not touch the shells.
#[test]
fn stopping_the_room_leaves_the_local_terminals_running() {
    let mut room = Room::start(false);
    let mut alice_window = room.publish("alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut alice_window));
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    wait_for_published(&mut alice, "alice");

    room.stop_broker();
    drop(alice);

    type_locally(&mut alice_window, &at, "echo after the room\n");
    assert!(
        local_cells_contain(&mut alice_window, &at.pane, "after the room"),
        "typing on your own computer must survive the room stopping"
    );
}

// Grants belong to the room and must outlive a restart of it.
#[test]
fn a_grant_survives_a_room_restart() {
    let mut room = Room::start(false);
    let mut alice_window = room.publish("alice", ALICE_SECRET);
    own_tree(&mut alice_window);
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    wait_for_published(&mut alice, "alice");
    send(
        &mut alice,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: true,
        },
    );
    wait_for(
        &mut alice,
        |message| matches!(message, ServerMsg::Grants { can_type_here, .. } if can_type_here.iter().any(|user| user == "bob")),
    );

    drop(alice);
    room.stop_broker();
    room.start_broker();

    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    let grants = wait_for(&mut bob, |message| {
        matches!(message, ServerMsg::Grants { .. })
    });
    let ServerMsg::Grants {
        you_may_type_into, ..
    } = grants
    else {
        panic!("the room must send grants");
    };
    assert!(
        you_may_type_into.iter().any(|user| user == "alice"),
        "a grant must survive a room restart"
    );
}

fn terminals(stream: &mut std::net::TcpStream, user: &str) -> Vec<TerminalInfo> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let ServerMsg::Terminals {
            user: owner,
            terminals,
        } = decode(stream)
            && owner == user
            && !terminals.is_empty()
        {
            return terminals;
        }
    }
    panic!("the room never listed {user}'s terminals");
}

fn wait_for_grant(stream: &mut std::net::TcpStream, allowed: bool) {
    wait_for(stream, |message| {
        matches!(message, ServerMsg::Grants { you_may_type_into, .. }
            if you_may_type_into.iter().any(|user| user == "alice") == allowed)
    });
}

fn cells_contain(stream: &mut std::net::TcpStream, pane: &str, expected: &str) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let ServerMsg::Cells {
            pane: shown, frame, ..
        } = decode(stream)
            && shown == pane
            && frame_text(&frame).contains(expected)
        {
            return true;
        }
    }
    false
}

fn type_locally(window: &mut UnixStream, at: &Location, text: &str) {
    send(
        window,
        &ClientMsg::TerminalInput {
            workspace: at.workspace.clone(),
            tab: at.tab.clone(),
            pane: at.pane.clone(),
            input: seer_core::TerminalInput::new(seer_core::InputEvent::Text(text.to_owned())),
        },
    );
}

fn local_cells_contain(window: &mut UnixStream, pane: &str, expected: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let Ok(message) = codec::decode::<_, ServerMsg>(window) else {
            return false;
        };
        if let ServerMsg::Cells {
            pane: shown, frame, ..
        } = message
            && shown == pane
            && frame_text(&frame).contains(expected)
        {
            return true;
        }
    }
    false
}

struct Location {
    workspace: String,
    tab: String,
    pane: String,
}

fn first_terminal(tree: &seer_core::Tree) -> Location {
    let workspace = &tree.workspaces[0];
    let tab = &workspace.tabs[0];
    Location {
        workspace: workspace.id.clone(),
        tab: tab.id.clone(),
        pane: tab.panes[0].id.clone(),
    }
}

fn frame_text(frame: &seer_core::TerminalFrame) -> String {
    frame
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.character).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn remote_mouse_is_grant_checked_and_encoded_in_the_destination_mode() {
    let mut room = Room::start(false);
    let mut window = room.publish("alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut window));
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    wait_for_published(&mut alice, "alice");
    send(
        &mut bob,
        &ClientMsg::Terminals {
            user: "alice".into(),
        },
    );
    terminals(&mut bob, "alice");
    send(
        &mut bob,
        &ClientMsg::Watch {
            user: "alice".into(),
            pane: at.pane.clone(),
            cols: 80,
            rows: 24,
        },
    );
    type_locally(
        &mut window,
        &at,
        "stty raw -echo; printf '\\033[?1000h\\033[?1006h'; dd bs=1 count=9 2>/dev/null | od -An -tx1; stty sane\n",
    );
    wait_for(&mut bob, |m| {
        matches!(m, ServerMsg::Cells { frame, .. }
        if frame.modes.mouse_tracking == seer_core::MouseTracking::Click)
    });
    let click = ClientMsg::MouseInto {
        user: "alice".into(),
        pane: at.pane.clone(),
        mouse: seer_core::MouseInput {
            kind: seer_core::MouseKind::Down,
            button: Some(seer_core::MouseButton::Left),
            column: 5,
            row: 2,
            modifiers: seer_core::Modifiers::default(),
        },
    };
    send(&mut bob, &click);
    wait_for(&mut bob, |m| matches!(m, ServerMsg::Refused { .. }));
    send(
        &mut alice,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: true,
        },
    );
    wait_for_grant(&mut bob, true);
    send(&mut bob, &click);
    assert!(cells_contain(
        &mut bob,
        &at.pane,
        "1b 5b 3c 30 3b 36 3b 33 4d"
    ));
}
