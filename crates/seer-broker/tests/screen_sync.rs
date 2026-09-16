#![cfg(target_os = "linux")]

use std::net::TcpStream;
use std::time::{Duration, Instant};

use seer_core::TerminalFrame;
use seer_core::frame_diff::apply_next;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/binary.rs"]
mod binary;
mod room;
#[path = "room/screen.rs"]
mod screen;
use room::*;
use screen::*;

// R-411: a viewer that doubts its copy of a screen asks for the screen
// again. The answer is the whole screen, and it takes the next number in
// that viewer's sequence for the pane.
#[test]
fn a_viewer_that_asks_again_gets_the_whole_screen_with_the_next_number() {
    let mut room = Room::start(false);
    let mut window = room.publish("alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut window));
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    wait_for_published(&mut alice, "alice");
    send(
        &mut bob,
        &ClientMsg::Watch {
            user: "alice".into(),
            pane: at.pane.clone(),
            cols: 80,
            rows: 24,
            viewer: true,
        },
    );
    type_locally(&mut window, &at, "printf 'sett''led\\n'\n");
    let held = wait_for(&mut bob, |message| {
        matches!(message, ServerMsg::Cells { pane, frame, .. }
            if *pane == at.pane && prompt_follows(&frame_text(frame), "settled"))
    });
    let ServerMsg::Cells {
        frame: held_frame,
        seq: held_seq,
        ..
    } = held
    else {
        unreachable!()
    };

    assert!(
        no_cells_for(&mut bob, &at.pane, Duration::from_secs(2)),
        "a quiet terminal must send nothing on its own, so the next screen is the answer"
    );

    send(
        &mut bob,
        &ClientMsg::Resync {
            user: "alice".into(),
            pane: at.pane.clone(),
        },
    );
    let answer = wait_for(
        &mut bob,
        |message| matches!(message, ServerMsg::Cells { pane, .. } if *pane == at.pane),
    );
    let ServerMsg::Cells { frame, seq, .. } = answer else {
        unreachable!()
    };
    assert_eq!(
        seq,
        held_seq + 1,
        "the answer must take the next number in the viewer's sequence"
    );
    assert_eq!(
        frame, held_frame,
        "the answer must be the whole screen as it stands"
    );
}

fn no_cells_for(stream: &mut std::net::TcpStream, pane: &str, quiet: Duration) -> bool {
    stream
        .set_read_timeout(Some(quiet))
        .expect("read timeout must set");
    let deadline = Instant::now() + quiet;
    let mut quiet_pane = true;
    while Instant::now() < deadline {
        match codec::decode::<_, ServerMsg>(stream) {
            Ok(ServerMsg::Cells { pane: shown, .. }) if shown == pane => {
                quiet_pane = false;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    stream
        .set_read_timeout(Some(WAIT))
        .expect("read timeout must set");
    quiet_pane
}

fn prompt_follows(text: &str, marker: &str) -> bool {
    text.find(marker)
        .is_some_and(|start| text[start + marker.len()..].contains('$'))
}

#[test]
fn reconnecting_a_viewer_does_not_report_a_live_terminal_as_empty() {
    let mut room = Room::start(false);
    let mut window = room.publish("alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut window));
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    wait_for_published(&mut alice, "alice");
    for _ in 0..4 {
        let mut viewer = join_room(room.address, "bob", BOB_SECRET);
        send(
            &mut viewer,
            &ClientMsg::Terminals {
                user: "alice".into(),
            },
        );
        let first = wait_for(
            &mut viewer,
            |message| matches!(message, ServerMsg::Terminals { user, .. } if user == "alice"),
        );
        assert!(
            matches!(first, ServerMsg::Terminals { terminals, .. }
            if terminals.iter().any(|terminal| terminal.pane == at.pane)),
            "reconnecting must not turn an unread catalog into an empty terminal list"
        );
    }
}

// R-411: after the whole screen, every change reaches a viewer as the
// diff from the screen it holds, numbered one after it. A whole screen
// in that stream means the runtime or the broker did not send the diff.
// The whole screen that answers a Resync at the end must equal the screen
// built from the diffs, so a diff from a stale baseline fails too.
#[test]
fn a_viewer_gets_each_change_as_the_diff_from_the_screen_it_holds() {
    let mut room = Room::start(false);
    let mut window = room.publish("alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut window));
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    wait_for_published(&mut alice, "alice");
    send(
        &mut bob,
        &ClientMsg::Watch {
            user: "alice".into(),
            pane: at.pane.clone(),
            cols: 80,
            rows: 24,
            viewer: true,
        },
    );
    let first = wait_for(
        &mut bob,
        |message| matches!(message, ServerMsg::Cells { pane, .. } if *pane == at.pane),
    );
    let ServerMsg::Cells { frame, seq, .. } = first else {
        unreachable!()
    };
    assert_eq!(seq, 1, "the screen at Watch is the first one numbered");
    let mut held = (frame, seq);

    // The second line clears the screen first, so cells go back to what
    // the Watch screen showed. A diff from a stale baseline does not set
    // those cells, and the whole screen at the end tells.
    for (typed, marker) in [
        ("printf 'cha''nged\\n'\n", "changed"),
        ("printf '\\033[2J\\033[Hsec''ond\\n'\n", "second"),
    ] {
        type_locally(&mut window, &at, typed);
        let whole = follow(&mut bob, &at.pane, &mut held, |frame| {
            prompt_follows(&frame_text(frame), marker)
        });
        assert!(
            whole.is_none(),
            "a change must arrive as a diff, not as the whole screen"
        );
    }

    send(
        &mut bob,
        &ClientMsg::Resync {
            user: "alice".into(),
            pane: at.pane.clone(),
        },
    );
    let (frame, seq) = follow(&mut bob, &at.pane, &mut held, |_| false)
        .expect("the whole screen must answer the request");
    assert_eq!(seq, held.1 + 1, "the answer follows the last diff");
    assert_eq!(
        frame, held.0,
        "the screen built from the diffs must equal the whole screen"
    );
}

// Applies each diff for the pane to the held screen until the held screen
// passes `until`. A whole screen for the pane ends the loop and is
// returned instead.
fn follow(
    stream: &mut TcpStream,
    pane: &str,
    held: &mut (TerminalFrame, u64),
    until: impl Fn(&TerminalFrame) -> bool,
) -> Option<(TerminalFrame, u64)> {
    let deadline = Instant::now() + WAIT;
    while !until(&held.0) {
        assert!(
            Instant::now() < deadline,
            "the change never reached the viewer"
        );
        match decode(stream) {
            ServerMsg::CellsDiff {
                pane: shown,
                seq,
                diff,
                ..
            } if shown == pane => {
                assert_eq!(
                    seq,
                    held.1 + 1,
                    "a diff follows the screen the viewer holds"
                );
                let frame = apply_next(&held.0, held.1, seq, &diff).expect("the diff must apply");
                *held = (frame, seq);
            }
            ServerMsg::Cells {
                pane: shown,
                frame,
                seq,
                ..
            } if shown == pane => return Some((frame, seq)),
            _ => {}
        }
    }
    None
}
