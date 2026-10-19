use super::{SeerWindow, sidebar_native_e2e::post_shortcut};
use gpui::{Keystroke, WindowHandle};
use std::{fs, path::Path, time::Duration};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let mut results = Vec::new();
    for (name, key, shift) in [
        ("Settings", 0x2b, false),
        ("Person menu", 0x0e, true),
        ("Room menu", 0x2e, true),
        ("Invite", 0x22, true),
        ("Start unavailable", 0x0f, true),
    ] {
        handle
            .update(cx, |view, window, _| {
                window.focus(&view.input.focus);
                post_shortcut(key, (1 << 20) | if shift { 1 << 17 } else { 0 });
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(250))
            .await;
        handle
            .update(cx, |view, window, cx| {
                let escape = Keystroke::parse("escape").unwrap();
                match name {
                    "Settings" => {
                        assert!(view.settings_open(), "AppKit must deliver Cmd Comma");
                        view.settings_key(&escape, window, cx);
                    }
                    "Person menu" => {
                        assert!(view.people_menu.is_some());
                        view.people_key(&Keystroke::parse("enter").unwrap(), window, cx);
                    }
                    "Room menu" => {
                        assert!(view.room.open());
                        view.room_key(&Keystroke::parse("enter").unwrap(), window, cx);
                        assert!(
                            view.invite_open(),
                            "the first host menu item must be focused"
                        );
                    }
                    "Invite" => assert!(view.invite_open()),
                    _ => assert!(!view.room.modal(), "macOS cannot start a room"),
                }
                results.push(serde_json::json!({"action":name,"native_key":key,"passed":true}));
            })
            .unwrap();
        if matches!(name, "Room menu" | "Invite") {
            handle.update(cx, |_, _, _| post_shortcut(0x35, 0)).unwrap();
        }
        cx.background_executor()
            .timer(Duration::from_millis(200))
            .await;
    }
    fs::write(
        evidence.join("native-commands.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
