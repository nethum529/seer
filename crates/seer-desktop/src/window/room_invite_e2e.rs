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
fn host_room_menu_opens_invite_and_restores_trigger_focus(cx: &mut TestAppContext) {
    let room = Room::start();
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !runtime.directory.path().join("input").exists() {
        draw(cx);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let expected = runtime.input();
    for width in [960., 680.] {
        cx.simulate_resize(size(px(width), px(640.)));
        let summary = seer::rooms::inspect_room(room.server.clone());
        assert!(
            summary.host,
            "room probe: {} {:?}",
            summary.state.label(),
            summary.people
        );
        view.update(cx, |view, cx| {
            view.server = Some(room.server.clone());
            view.room.server = Some(room.server.clone());
            view.room.current = Some(summary);
            cx.notify();
        });
        draw(cx);
        let trigger = cx.debug_bounds("room-trigger").unwrap();
        cx.simulate_click(trigger.center(), gpui::Modifiers::default());
        draw(cx);
        let invite = cx
            .debug_bounds("room-invite")
            .expect("a connected host must have Invite a person");
        cx.simulate_click(invite.center(), gpui::Modifiers::default());
        draw(cx);
        assert!(cx.debug_bounds("invite-primary").is_some());
        assert!(cx.debug_bounds("room-confirm").is_none());
        close_invite(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            cx.debug_bounds("room-invite").is_some(),
            "close must return focus to the room trigger"
        );
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            cx.debug_bounds("invite-primary").is_some(),
            "keyboard activation must open Invite"
        );
        create_and_copy(cx, &room.capsule);
        close_invite(cx);
        cx.simulate_keystrokes("space");
        draw(cx);
        cx.simulate_keystrokes("escape");
        draw(cx);
    }
    assert_eq!(
        runtime.input(),
        expected,
        "menu and sheet keys must not reach the shell"
    );
    room.host.store(false, Ordering::Relaxed);
    let (view, cx) = open_view(&runtime, &mut cx.cx);
    let summary = seer::rooms::inspect_room(room.server.clone());
    assert!(!summary.host);
    view.update(cx, |view, cx| {
        view.server = Some(room.server.clone());
        view.room.server = Some(room.server.clone());
        view.room.current = Some(summary);
        cx.notify();
    });
    draw(cx);
    let trigger = cx.debug_bounds("room-trigger").unwrap();
    cx.simulate_click(trigger.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(cx.debug_bounds("room-leave").is_some());
    assert!(
        cx.debug_bounds("room-invite").is_none(),
        "guests must not have Invite a person"
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(
        runtime.input(),
        expected,
        "menu and sheet keys must not reach the shell"
    );
}

fn close_invite(cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes("escape");
    draw(cx);
}

fn create_and_copy(cx: &mut gpui::VisualTestContext, capsule: &str) {
    cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
    cx.simulate_keystrokes("tab enter");
    let expected = format!("seer join {capsule}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        draw(cx);
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

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
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
