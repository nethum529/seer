use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use gpui::{ClipboardItem, TestAppContext, px, size};
use seer_core::{
    Tree,
    proto::{ClientMsg, ServerMsg, codec},
};

use super::input_e2e::{Runtime, open_view};

#[gpui::test]
fn invite_done_returns_typing_to_terminal(cx: &mut TestAppContext) {
    close_routes_typing(cx, true);
}

#[gpui::test]
fn invite_escape_returns_typing_to_terminal(cx: &mut TestAppContext) {
    close_routes_typing(cx, false);
}

fn close_routes_typing(cx: &mut TestAppContext, done: bool) {
    let room = Room::start();
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    let mut expected = Vec::new();
    for width in [960., 680.] {
        cx.simulate_resize(size(px(width), px(640.)));
        let summary = seer::rooms::inspect_room(room.server.clone());
        assert!(summary.host);
        view.update(cx, |view, cx| {
            view.server = Some(room.server.clone());
            view.room.server = Some(room.server.clone());
            view.room.current = Some(summary);
            cx.notify();
        });
        for keyboard in [false, true] {
            cx.run_until_parked();
            if keyboard {
                cx.simulate_keystrokes(if cfg!(target_os = "macos") {
                    "cmd-shift-i"
                } else {
                    "ctrl-alt-super-i"
                });
            } else {
                let trigger = cx.debug_bounds("room-trigger").unwrap();
                cx.simulate_click(trigger.center(), gpui::Modifiers::default());
                cx.run_until_parked();
                let invite = cx.debug_bounds("room-invite").unwrap();
                cx.simulate_click(invite.center(), gpui::Modifiers::default());
            }
            cx.run_until_parked();
            assert!(cx.debug_bounds("invite-primary").is_some());
            if done {
                create_and_copy(cx, &room.capsule);
                cx.simulate_keystrokes("tab tab enter");
            } else {
                cx.simulate_keystrokes("escape");
            }
            super::watch_e2e::pump(cx);
            cx.simulate_keystrokes("space");
            super::watch_e2e::pump(cx);
            expected.push(b' ');
            assert_eq!(
                runtime.input(),
                expected,
                "typing after close must reach the PTY: done={done}, keyboard={keyboard}, width={width}"
            );
            cx.update(|window, cx| assert!(view.read(cx).input.focus.is_focused(window)));
        }
    }
}

#[gpui::test]
fn guest_invite_shortcut_keeps_terminal_input(cx: &mut TestAppContext) {
    let room = Room::start();
    room.host.store(false, Ordering::Relaxed);
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    let summary = seer::rooms::inspect_room(room.server.clone());
    assert!(!summary.host);
    view.update(cx, |view, cx| {
        view.server = Some(room.server.clone());
        view.room.server = Some(room.server.clone());
        view.room.current = Some(summary);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.focus(&view.read(cx).input.focus));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-shift-i"
    } else {
        "ctrl-alt-super-i"
    });
    cx.simulate_keystrokes("space");
    super::watch_e2e::pump(cx);
    assert_eq!(runtime.input(), b" ");
    let trigger = cx.debug_bounds("room-trigger").unwrap();
    cx.simulate_click(trigger.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-leave").is_some());
    cx.simulate_keystrokes("escape");
    assert_eq!(runtime.input(), b" ");
}

fn create_and_copy(cx: &mut gpui::VisualTestContext, capsule: &str) {
    cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
    cx.simulate_keystrokes("tab enter");
    let expected = format!("seer join {capsule}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        if cx.read_from_clipboard().and_then(|item| item.text()) == Some(expected.clone()) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the reopened sheet must create and copy its invite"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

struct Room {
    server: seer::ServerEntry,
    capsule: String,
    host: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Room {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let capsule = format!(
            "SEER1-127.0.0.1-{}-seat",
            listener.local_addr().unwrap().port()
        );
        let issued = capsule.clone();
        let server = serde_json::from_value(serde_json::json!({"endpoint": listener.local_addr().unwrap().to_string(), "alias":"E2e", "user_id":"e2e", "name":"E2e", "credential":"e2e", "current":true})).unwrap();
        let host = Arc::new(AtomicBool::new(true));
        let stopped = Arc::new(AtomicBool::new(false));
        let role = host.clone();
        let done = stopped.clone();
        let worker = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let Ok(ClientMsg::Hello { .. }) = codec::decode(&mut stream) else {
                    continue;
                };
                codec::encode(
                    &mut stream,
                    &ServerMsg::Welcome {
                        user_id: "e2e".into(),
                        name: "E2e".into(),
                        client_id: "menu-e2e".into(),
                        tree: Tree::new(),
                    },
                )
                .unwrap();
                match codec::decode(&mut stream) {
                    Ok(ClientMsg::ListPeople) => {
                        let person = serde_json::from_value(serde_json::json!({"user_id":"e2e", "name":"E2e", "attached_clients":2, "peekable":true, "online":true, "host":role.load(Ordering::Relaxed)})).unwrap();
                        codec::encode(
                            &mut stream,
                            &ServerMsg::People {
                                people: vec![person],
                            },
                        )
                        .unwrap();
                    }
                    Ok(ClientMsg::Invite { hours: Some(24) }) => {
                        codec::encode(
                            &mut stream,
                            &ServerMsg::Seat {
                                capsule: issued.clone(),
                                expires_in_secs: 86400,
                            },
                        )
                        .unwrap();
                    }
                    _ => {}
                }
            }
        });
        Self {
            server,
            capsule,
            host,
            stopped,
            worker: Some(worker),
        }
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
