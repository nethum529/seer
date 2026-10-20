use gpui::{TestAppContext, VisualTestContext};
use seer_core::proto::{ClientMsg, ServerMsg};

use super::input_e2e::Runtime;
use super::watch_e2e::{Room, open_view, pump};
use super::*;

#[gpui::test]
fn room_bulk_control_confirms_sharing_and_revokes_without_confirmation(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let room = Room::start();
    let (_, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    click(cx, "room-trigger");
    pump(cx);
    click(cx, "room-share-all");
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-confirm").is_some());
    assert!(requests(&room).is_empty());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(requests(&room).is_empty(), "Cancel must have initial focus");
    click(cx, "room-trigger");
    pump(cx);
    click(cx, "room-share-all");
    cx.simulate_keystrokes("tab enter");
    pump(cx);
    assert_eq!(requests(&room), vec![true]);
    reply(&room, 1, true);
    pump(cx);
    cx.simulate_click(gpui::point(px(90.), px(62.)), gpui::Modifiers::default());
    pump(cx);
    assert!(cx.debug_bounds("Share control for Kyler: On").is_some());
    cx.simulate_keystrokes("escape");
    click(cx, "room-trigger");
    pump(cx);
    click(cx, "room-stop-sharing");
    pump(cx);
    assert_eq!(requests(&room), vec![true, false]);
    reply(&room, 2, false);
    pump(cx);
    cx.simulate_click(gpui::point(px(90.), px(62.)), gpui::Modifiers::default());
    pump(cx);
    assert!(cx.debug_bounds("Share control for Kyler: Off").is_some());
}

fn click(cx: &mut VisualTestContext, label: &'static str) {
    let bounds = cx
        .debug_bounds(label)
        .unwrap_or_else(|| panic!("missing {label}"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
}

fn requests(room: &Room) -> Vec<bool> {
    room.received
        .lock()
        .unwrap()
        .iter()
        .filter_map(|message| match message {
            ClientMsg::SetAllGrants { can_type, .. } => Some(*can_type),
            _ => None,
        })
        .collect()
}

fn reply(room: &Room, request: usize, can_type: bool) {
    room.messages
        .send(ServerMsg::Grants {
            can_type_here: if can_type {
                vec!["Kyler".into()]
            } else {
                Vec::new()
            },
            you_may_type_into: Vec::new(),
        })
        .unwrap();
    grant_result(room, request, false);
}

fn grant_result(room: &Room, request: usize, refused: bool) {
    let request_id = room
        .received
        .lock()
        .unwrap()
        .iter()
        .filter_map(|message| match message {
            ClientMsg::SetAllGrants { request_id, .. } => *request_id,
            _ => None,
        })
        .nth(request - 1)
        .unwrap();
    room.messages
        .send(ServerMsg::AllGrantsResult {
            request_id,
            error: refused.then(|| "no".into()),
        })
        .unwrap();
}

#[gpui::test]
fn bulk_control_failure_can_retry_and_actions_hide_when_alone(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let room = Room::start();
    let (view, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    let people = view.read_with(cx, |view, _| {
        let Content::Terminal(link, _) = &view.content else {
            panic!("terminal missing")
        };
        link.people.people.clone()
    });
    room.messages
        .send(ServerMsg::People { people: Vec::new() })
        .unwrap();
    pump(cx);
    click(cx, "room-trigger");
    pump(cx);
    assert!(cx.debug_bounds("room-share-all").is_none());
    assert!(cx.debug_bounds("room-stop-sharing").is_none());
    cx.simulate_keystrokes("escape");
    room.messages.send(ServerMsg::People { people }).unwrap();
    pump(cx);
    click(cx, "room-trigger");
    pump(cx);
    click(cx, "room-stop-sharing");
    pump(cx);
    grant_result(&room, 1, true);
    pump(cx);
    assert!(
        cx.debug_bounds("Share control was refused. Try again.")
            .is_some()
    );
    cx.simulate_keystrokes("home down enter");
    pump(cx);
    assert_eq!(requests(&room), vec![false, false]);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(5));
    pump(cx);
    assert!(
        cx.debug_bounds("The room did not answer. Try again.")
            .is_some()
    );
    cx.simulate_keystrokes("home down enter");
    pump(cx);
    assert_eq!(requests(&room), vec![false, false, false]);
    reply(&room, 3, false);
    pump(cx);
}

#[gpui::test]
fn late_share_reply_does_not_hide_stop_error(cx: &mut TestAppContext) {
    late_share_reply(cx, false);
}

#[gpui::test]
fn late_share_reply_does_not_hide_stop_refusal(cx: &mut TestAppContext) {
    late_share_reply(cx, true);
}

fn late_share_reply(cx: &mut TestAppContext, refused: bool) {
    let runtime = Runtime::start();
    let room = Room::start();
    let (view, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    click(cx, "room-trigger");
    pump(cx);
    click(cx, "room-share-all");
    cx.simulate_keystrokes("tab enter");
    pump(cx);
    assert_eq!(requests(&room), vec![true]);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(5));
    pump(cx);
    assert!(cx.debug_bounds("room-error").is_some());
    cx.simulate_keystrokes("escape");
    pump(cx);
    click(cx, "room-trigger");
    pump(cx);
    click(cx, "room-stop-sharing");
    pump(cx);
    assert_eq!(requests(&room), vec![true, false]);
    reply(&room, 1, true);
    pump(cx);
    let error = if refused {
        grant_result(&room, 2, true);
        "Share control was refused. Try again."
    } else {
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(5));
        "The room did not answer. Try again."
    };
    pump(cx);
    assert!(
        view.read_with(cx, |view, _| view.room.open()) && cx.debug_bounds(error).is_some(),
        "Stop sharing lost its own error: {error}"
    );
}
