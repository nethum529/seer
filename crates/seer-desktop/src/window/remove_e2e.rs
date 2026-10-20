use super::{
    Content,
    input_e2e::{Runtime, open_view},
};
use futures::StreamExt;
use gpui::{TestAppContext, VisualTestContext, px, size};
use seer_core::proto::ClientMsg;
use std::{
    thread,
    time::{Duration, Instant},
};

#[path = "remove_peer.rs"]
mod peer;

#[gpui::test]
fn host_can_cancel_remove_retry_and_update_every_window(cx: &mut TestAppContext) {
    let room = peer::Room::start(true);
    let runtime = Runtime::start();
    let mut other_app = cx.new_app();
    let (view, window, _listener) = join(&room, &runtime, cx);
    let (other, other_cx, _other_listener) = join(&room, &runtime, &mut other_app);
    pump(other_cx, |_| room.connections() >= 2);
    window.update(|window, cx| {
        view.update(cx, |view, cx| view.open_people(true, window, cx));
    });
    window.run_until_parked();
    assert!(window.debug_bounds("Remove Guest").is_some());
    assert!(window.debug_bounds("Remove Host").is_none());
    window.simulate_keystrokes("end enter");
    window.run_until_parked();
    assert!(window.debug_bounds("Remove Guest?").is_some());
    window.simulate_keystrokes("enter");
    window.run_until_parked();
    assert!(
        view.read_with(window, |view, _| !view.room.modal()),
        "Cancel has initial focus"
    );
    assert_eq!(room.removals(), 0);
    window.update(|window, cx| {
        view.update(cx, |view, cx| view.open_people(true, window, cx));
    });
    window.run_until_parked();
    let action = window.debug_bounds("Remove Guest").unwrap();
    window.simulate_click(action.center(), gpui::Modifiers::default());
    window.run_until_parked();
    let confirm = window.debug_bounds("room-confirm-action").unwrap();
    window.simulate_click(confirm.center(), gpui::Modifiers::default());
    pump(window, |cx| cx.debug_bounds("room-error").is_some());
    assert!(window.debug_bounds("room-confirm").is_some());
    room.retry();
    window.simulate_keystrokes("enter");
    pump(window, |cx| {
        view.read_with(cx, |view, _| !view.room.pending) && room.removals() == 2
    });
    assert!(view.read_with(window, |view, _| !view.room.modal()));
    pump(window, |cx| {
        view.read_with(cx, |view, _| matches!(&view.content, Content::Terminal(link, _) if link.people.people.len() == 1))
    });
    pump(other_cx, |cx| {
        other.read_with(cx, |view, _| matches!(&view.content, Content::Terminal(link, _) if link.people.people.len() == 1))
    });
    let terminal = window.debug_bounds("terminal").unwrap();
    window.simulate_click(terminal.center(), gpui::Modifiers::default());
    window.simulate_keystrokes("q");
    pump(window, |_| runtime.input().ends_with(b"q"));
    assert!(
        room.received()
            .iter()
            .any(|m| matches!(m, ClientMsg::RemovePerson {name} if name == "Guest"))
    );
}

#[gpui::test]
fn guest_has_no_remove_and_removed_screen_keeps_local_shell(cx: &mut TestAppContext) {
    let room = peer::Room::start(false);
    let runtime = Runtime::start();
    let (mut local, _) = runtime.connect();
    let (view, cx, _listener) = join(&room, &runtime, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_people(true, window, cx));
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("Remove Host").is_none());
    assert!(cx.debug_bounds("Remove Guest").is_none());
    room.remove_guest();
    pump(cx, |cx| cx.debug_bounds("removed-room").is_some());
    assert!(cx.debug_bounds("status-banner").is_none());
    assert!(
        cx.debug_bounds("The host removed you from this room")
            .is_some()
    );
    local
        .input(seer_core::InputEvent::Text("k".into()))
        .unwrap();
    pump(cx, |_| runtime.input().ends_with(b"k"));
    thread::sleep(Duration::from_millis(1100));
    cx.run_until_parked();
    assert_eq!(room.connections(), 1, "removal must not reconnect");
}

fn join<'a>(
    room: &peer::Room,
    runtime: &Runtime,
    cx: &'a mut TestAppContext,
) -> (
    gpui::Entity<super::SeerWindow>,
    &'a mut VisualTestContext,
    gpui::Task<()>,
) {
    let (view, cx) = open_view(runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    let mut messages = view.update(cx, |view, cx| {
        view.room.server = Some(room.server.clone());
        let Content::Terminal(link, _) = &mut view.content else {
            panic!("missing terminal")
        };
        let messages = link.join_room(room.server.clone(), None);
        cx.notify();
        messages
    });
    let listener = cx.spawn({
        let view = view.clone();
        async move |mut cx| {
            while let Some(event) = messages.next().await {
                view.update(&mut cx, |view, cx| view.receive(event, cx))
                    .unwrap();
            }
        }
    });
    pump(cx, |cx| {
        view.read_with(cx, |view, _| matches!(&view.content, Content::Terminal(link, _) if link.people.people.len() == 2))
    });
    (view, cx, listener)
}

fn pump(cx: &mut VisualTestContext, mut ready: impl FnMut(&mut VisualTestContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if ready(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "room action did not finish");
        thread::sleep(Duration::from_millis(5));
    }
}
