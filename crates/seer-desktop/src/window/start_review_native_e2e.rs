use super::{Content, SeerWindow, sidebar_native_e2e::post_shortcut};
use gpui::WindowHandle;
use std::{fs, path::Path, time::Duration};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| {
            let server = seer::ServerStore::load()
                .unwrap()
                .default_server()
                .unwrap()
                .clone();
            view.pick_saved_room(server, window, cx);
            capture_level(window);
        })
        .unwrap();
    pause(cx).await;
    handle
        .update(cx, |view, window, _| {
            assert!(
                window.is_window_active(),
                "the native monitor needs a key window"
            );
            assert!(matches!(view.content, Content::Terminal(..)));
            post_shortcut(0x2d, 1 << 20);
        })
        .unwrap();
    pause(cx).await;
    let before = handle
        .update(cx, |view, window, cx| {
            let Content::Terminal(link, _) = &view.content else {
                panic!("room required")
            };
            assert_eq!(
                link.terminals().len(),
                2,
                "Cmd N must reach the native monitor before Join"
            );
            let state = (
                link.terminals().len(),
                link.shown().map(str::to_owned),
                view.appearance.settings.hide_sidebar,
            );
            view.open_join(None, window, cx);
            state
        })
        .unwrap();
    pause(cx).await;
    for (name, code, flags) in [
        ("new-terminal", 0x2d, 1 << 20),
        ("sidebar", 0x0b, 1 << 20),
        ("focus-sidebar", 0x0b, (1 << 20) | (1 << 17)),
        ("select-terminal", 0x12, 1 << 20),
        ("next-terminal", 0x7c, 1 << 20),
        ("rename", 0x78, 0),
        ("tab-menu", 0x6d, 1 << 17),
    ] {
        handle
            .update(cx, |_, window, _| {
                assert!(window.is_window_active());
                post_shortcut(code, flags);
            })
            .unwrap();
        pause(cx).await;
        handle
            .update(cx, |view, window, _| {
                let Content::Terminal(link, _) = &view.content else {
                    panic!("room required")
                };
                assert_eq!(
                    (
                        link.terminals().len(),
                        link.shown().map(str::to_owned),
                        view.appearance.settings.hide_sidebar
                    ),
                    before,
                    "{name} must not change the room behind Join"
                );
                assert!(view.join_screen.is_some(), "Join must remain open");
                assert!(
                    !view.input.focus.is_focused(window),
                    "{name} must not focus the hidden terminal"
                );
                assert!(view.editing.is_none() && view.menu.is_none());
            })
            .unwrap();
    }
    snapshot(capture, evidence, "join-native-shortcuts");
    handle.update(cx, |_, _, _| post_shortcut(0x35, 0)).unwrap();
    pause(cx).await;
    handle
        .update(cx, |view, window, _| {
            assert!(view.join_screen.is_none(), "Escape must return from Join");
            assert!(view.input.focus.is_focused(window));
        })
        .unwrap();
    fs::write(evidence.join("native-shortcuts.json"), serde_json::to_vec_pretty(&serde_json::json!({"active_window": true, "before_join_cmd_n_created_terminal": true, "blocked_shortcuts": 7, "escape_returned_to_terminal": true})).unwrap()).unwrap();
    scroll_rooms(handle, evidence, capture, cx).await;
}

async fn pause(cx: &mut gpui::AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(350))
        .await;
}

async fn scroll_rooms(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let config = std::path::PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap())
        .join("seer/servers.toml");
    let rooms: String = (1..=20).map(|at| format!("[[servers]]\nendpoint = '127.0.0.1:{at}'\nalias = 'Room {at:02}'\nuser_id = 'room-{at}'\nname = 'Host'\ncredential = 'fixture'\ncurrent = {}\n", at == 1)).collect();
    fs::write(config, rooms).unwrap();
    for (width, height) in [(960., 640.), (320., 480.)] {
        handle
            .update(cx, |view, window, cx| {
                view.content = Content::Start(super::start::StartScreen::new(cx));
                view.room = super::room::RoomUi::new(None, cx);
                view.server = None;
                view._messages = None;
                let Content::Start(start) = &view.content else {
                    unreachable!()
                };
                window.focus(&start.host);
                window.resize(gpui::size(gpui::px(width), gpui::px(height)));
                cx.notify();
            })
            .unwrap();
        pause(cx).await;
        handle.update(cx, |_, _, _| post_shortcut(0x30, 0)).unwrap();
        pause(cx).await;
        for at in 1..=20 {
            handle.update(cx, |_, _, _| post_shortcut(0x30, 0)).unwrap();
            pause(cx).await;
            handle
                .update(cx, |view, window, _| {
                    let Content::Start(start) = &view.content else {
                        unreachable!()
                    };
                    assert!(start.saved[at - 1].1.is_focused(window));
                })
                .unwrap();
            if [1, 7, 14, 20].contains(&at) {
                snapshot(
                    capture,
                    evidence,
                    &format!("scroll-{width}x{height}-room-{at}"),
                );
            }
        }
        for _ in 1..20 {
            handle
                .update(cx, |_, _, _| post_shortcut(0x30, 1 << 17))
                .unwrap();
            pause(cx).await;
        }
        snapshot(
            capture,
            evidence,
            &format!("scroll-{width}x{height}-reverse-room-1"),
        );
    }
}

fn snapshot(capture: &Path, evidence: &Path, name: &str) {
    let output = std::process::Command::new(capture)
        .arg(std::process::id().to_string())
        .arg(evidence)
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn capture_level(window: &gpui::Window) {
    use objc2::{msg_send, runtime::AnyObject};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).unwrap().as_raw()
    else {
        panic!("AppKit required")
    };
    // SAFETY: this live AppKit window is on the main thread; the capture script needs level zero.
    unsafe {
        let native: *mut AnyObject = msg_send![handle.ns_view.as_ptr().cast::<AnyObject>(), window];
        let _: () = msg_send![native, setLevel: 0isize];
    }
}
