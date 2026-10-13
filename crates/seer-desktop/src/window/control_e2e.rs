use gpui::{TestAppContext, VisualTestContext, point};
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{InputEvent, KeyCode, MouseKind, MouseTracking};

use super::input_e2e::{Runtime, paste_shortcut};
use super::watch_e2e::{Room, frame, open_view, pump};
use super::*;

#[gpui::test]
fn share_control_waits_for_confirmation_and_remote_input_stops_on_revoke(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let room = Room::start();
    let (_, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    cx.simulate_click(point(px(90.), px(62.)), gpui::Modifiers::default());
    cx.simulate_keystrokes("down");
    pump(cx);
    click_switch(cx, "Share control for Kyler: Off");
    pump(cx);
    assert!(grant_requests(&room) == vec![true]);
    assert!(cx.debug_bounds("Share control for Kyler: Off").is_some());
    click_switch(cx, "Share control for Kyler: Off");
    pump(cx);
    assert!(
        grant_requests(&room) == vec![true],
        "pending switches must wait for the broker"
    );
    grants(&room, true, false);
    pump(cx);
    assert!(cx.debug_bounds("Share control for Kyler: On").is_some());
    cx.simulate_keystrokes("space");
    pump(cx);
    assert!(grant_requests(&room) == vec![true, false]);
    assert!(cx.debug_bounds("Share control for Kyler: On").is_some());
    room.messages
        .send(ServerMsg::Refused {
            reason: "test refusal".into(),
        })
        .unwrap();
    pump(cx);
    assert!(cx.debug_bounds("Share control was refused.").is_some());
    assert!(cx.debug_bounds("Share control for Kyler: On").is_some());
    cx.simulate_keystrokes("space");
    pump(cx);
    grants(&room, false, true);
    pump(cx);
    assert!(cx.debug_bounds("Share control for Kyler: Off").is_some());
    cx.simulate_keystrokes("enter");
    pump(cx);
    assert!(cx.debug_bounds("You have control").is_some());
    let mut screen = frame("Remote live");
    if let ServerMsg::Cells { frame, .. } = &mut screen {
        frame.modes.mouse_tracking = MouseTracking::AnyMotion;
    }
    room.messages.send(screen).unwrap();
    pump(cx);
    let before = runtime.input();
    let position = point(px(300.), px(100.));
    cx.simulate_click(position, gpui::Modifiers::default());
    cx.simulate_keystrokes("a enter up");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("Shared paste".into()));
    cx.simulate_keystrokes(paste_shortcut());
    wheel(cx, position);
    pump(cx);
    let events = inputs(&room);
    assert_remote_input(&events);
    assert_eq!(
        runtime.input(),
        before,
        "shared input must not reach the local PTY"
    );
    room.messages.send(frame("Primary screen")).unwrap();
    pump(cx);
    wheel(cx, position);
    cx.simulate_keystrokes("shift-pageup shift-pagedown shift-home shift-end");
    pump(cx);
    assert_eq!(inputs(&room), events, "viewer scrollback must stay local");
    grants(&room, false, false);
    pump(cx);
    assert!(cx.debug_bounds("Read only").is_some());
    cx.simulate_keystrokes("b enter up");
    cx.simulate_click(position, gpui::Modifiers::default());
    wheel(cx, position);
    pump(cx);
    assert_eq!(inputs(&room), events, "revoke must stop all remote input");
    assert_eq!(runtime.input(), before);
}

fn click_switch(cx: &mut VisualTestContext, label: &'static str) {
    let bounds = cx
        .debug_bounds(label)
        .expect("the person menu must have a labeled Share control switch");
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
}

fn grants(room: &Room, here: bool, there: bool) {
    room.messages
        .send(ServerMsg::Grants {
            can_type_here: if here {
                vec!["Kyler".into()]
            } else {
                Vec::new()
            },
            you_may_type_into: if there {
                vec!["Kyler".into()]
            } else {
                Vec::new()
            },
        })
        .unwrap();
}

fn grant_requests(room: &Room) -> Vec<bool> {
    room.received
        .lock()
        .unwrap()
        .iter()
        .filter_map(|message| match message {
            ClientMsg::SetGrant { user, can_type } if user == "Kyler" => Some(*can_type),
            _ => None,
        })
        .collect()
}

fn inputs(room: &Room) -> Vec<InputEvent> {
    room.received
        .lock()
        .unwrap()
        .iter()
        .filter_map(|message| match message {
            ClientMsg::InputInto { user, pane, input } if user == "Kyler" && pane == "remote" => {
                Some(input.event.clone())
            }
            _ => None,
        })
        .collect()
}

fn wheel(cx: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Lines(point(0., 1.)),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
}

fn assert_remote_input(events: &[InputEvent]) {
    assert!(
        events
            .iter()
            .any(|event| matches!(event, InputEvent::Text(text) if text == "a"))
    );
    assert!(events.iter().any(
        |event| matches!(event, InputEvent::Key(key) if key.code == KeyCode::Enter)
            || matches!(event, InputEvent::Text(text) if text == "\r")
    ));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, InputEvent::Key(key) if key.code == KeyCode::Up))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, InputEvent::Paste(text) if text == "Shared paste"))
    );
    for kind in [MouseKind::Down, MouseKind::Up, MouseKind::ScrollUp] {
        assert!(
            events
                .iter()
                .any(|event| matches!(event, InputEvent::Mouse(mouse) if mouse.kind == kind))
        );
    }
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, InputEvent::Scrollback { .. }))
    );
}
