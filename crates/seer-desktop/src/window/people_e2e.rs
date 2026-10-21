use super::input_e2e::Runtime;
use super::watch_e2e::{Room, open_view, pump};
use super::*;
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use seer_core::InputEvent;
use seer_core::proto::{ClientMsg, ServerMsg};

fn shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-shift-e"
    } else {
        "ctrl-alt-super-e"
    }
}

fn click(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_click(gpui::point(px(x), px(y)), Modifiers::default());
    cx.run_until_parked();
}

#[gpui::test]
fn people_pointer_pick_focuses_terminal_and_routes_typing(cx: &mut TestAppContext) {
    pick_and_type(cx, false);
}

#[gpui::test]
fn people_keyboard_pick_focuses_terminal_and_routes_typing(cx: &mut TestAppContext) {
    pick_and_type(cx, true);
}

fn pick_and_type(cx: &mut TestAppContext, keyboard: bool) {
    let runtime = Runtime::start();
    let room = Room::start();
    let (view, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    let mut local_input = Vec::new();
    for control in [false, true] {
        room.messages
            .send(ServerMsg::Grants {
                can_type_here: Vec::new(),
                you_may_type_into: if control {
                    vec!["Kyler".into()]
                } else {
                    Vec::new()
                },
            })
            .unwrap();
        pump(cx);
        if keyboard {
            cx.simulate_keystrokes(shortcut());
            cx.simulate_keystrokes("down enter");
        } else {
            click(cx, 90., 62.);
            click(cx, 100., 151.);
        }
        pump(cx);
        cx.simulate_keystrokes("space");
        pump(cx);
        cx.update(|window, cx| assert!(view.read(cx).input.focus.is_focused(window)));
        let events: Vec<_> = room
            .received
            .lock()
            .unwrap()
            .iter()
            .filter_map(|message| match message {
                ClientMsg::InputInto { user, pane, input }
                    if user == "Kyler"
                        && pane == "remote"
                        && !matches!(input.event, InputEvent::Focus(_)) =>
                {
                    Some(input.event.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            events.len(),
            usize::from(control),
            "remote typing must follow the grant"
        );
        if control {
            assert!(
                matches!(&events[0], InputEvent::Text(text) | InputEvent::KeyWithText { text, .. } if text == " ")
            );
        }
        assert!(
            runtime.input() == local_input,
            "remote typing must not reach the local PTY"
        );
        room.received.lock().unwrap().clear();
        cx.simulate_keystrokes(shortcut());
        cx.simulate_keystrokes("enter");
        pump(cx);
        cx.simulate_keystrokes("space");
        pump(cx);
        local_input.push(b' ');
        assert_eq!(
            runtime.input(),
            local_input,
            "picking yourself must focus the local PTY"
        );
    }
}

#[gpui::test]
fn people_escape_restores_focus_before_open(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let room = Room::start();
    let (view, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    for origin in ["terminal", "tabs", "trigger"] {
        cx.update(|window, cx| {
            let view = view.read(cx);
            window.focus(match origin {
                "terminal" => &view.input.focus,
                "tabs" => &view.keys.tabs,
                _ => &view.keys.who,
            });
        });
        let focus = cx.update(|window, cx| window.focused(cx).unwrap());
        for keyboard in [false, true] {
            if keyboard {
                cx.simulate_keystrokes(shortcut());
            } else {
                click(cx, 90., 62.);
            }
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            cx.update(|window, _| {
                assert!(
                    focus.is_focused(window),
                    "Escape must restore {origin}, keyboard={keyboard}"
                )
            });
        }
    }
    assert!(runtime.input().is_empty());
}
