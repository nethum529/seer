use std::{
    fs,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use gpui::{ClipboardItem, TestAppContext, VisualTestContext};

use super::{
    SeerWindow,
    input_e2e::{Runtime, open_view},
};

#[gpui::test]
fn launch_requires_a_room_pick(cx: &mut TestAppContext) {
    if std::env::var_os("SEER_START_SCREEN_CHILD").is_none() {
        let directory = tempfile::tempdir().unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "window::start_e2e::launch_requires_a_room_pick",
                "--nocapture",
            ])
            .env("SEER_START_SCREEN_CHILD", "1")
            .env("XDG_CONFIG_HOME", directory.path().join("config"))
            .env("XDG_STATE_HOME", directory.path().join("state"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let config =
        std::path::PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap()).join("seer");
    fs::create_dir_all(&config).unwrap();
    let runtime = Runtime::start();
    let (mut original, _) = runtime.connect();
    let local = std::path::PathBuf::from(std::env::var_os("XDG_STATE_HOME").unwrap())
        .join("seer/runtimes/e2e");
    fs::create_dir_all(&local).unwrap();
    std::os::unix::fs::symlink(
        runtime.directory.path().join("socket"),
        local.join("socket"),
    )
    .unwrap();
    for saved in [false, true] {
        if saved {
            fs::write(config.join("servers.toml"), "[[servers]]\nendpoint = '127.0.0.1:1'\nalias = 'Pairing'\nuser_id = 'e2e'\nname = 'Host'\ncredential = 'secret'\ncurrent = true\n").unwrap();
        }
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::assets::load_fonts(cx).unwrap();
            SeerWindow::new(window, cx)
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("start-join").is_some(),
            "launch must show the start screen"
        );
        assert!(cx.debug_bounds("start-room").is_some());
        assert!(cx.debug_bounds("terminal").is_none());
        assert_eq!(cx.debug_bounds("saved-room-e2e").is_some(), saved);
        click(cx, "start-join");
        assert!(cx.debug_bounds("join-field").is_some());
        click(cx, "join-back");
        assert!(cx.debug_bounds("start-join").is_some());
        cx.simulate_keystrokes("tab enter");
        assert!(
            cx.debug_bounds("join-field").is_some(),
            "Tab reaches Join a room"
        );
        cx.simulate_keystrokes("escape");
        assert!(cx.debug_bounds("start-join").is_some());
        if saved {
            cx.simulate_keystrokes("tab tab enter");
            cx.run_until_parked();
            let deadline = Instant::now() + Duration::from_secs(5);
            while cx.debug_bounds("terminal").is_none() {
                cx.run_until_parked();
                assert!(Instant::now() < deadline, "a pick opens the saved room");
                thread::sleep(Duration::from_millis(10));
            }
            original
                .input(seer_core::InputEvent::Text("k".into()))
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !runtime.input().ends_with(b"k") {
                assert!(
                    Instant::now() < deadline,
                    "launch must keep the original shell"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[gpui::test]
fn room_menu_join_and_back_keep_the_terminal(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (_, cx) = open_view(&runtime, cx);
    cx.run_until_parked();
    click(cx, "room-trigger");
    click(cx, "room-join");
    assert!(cx.debug_bounds("join-field").is_some());
    cx.simulate_input("bad invite");
    cx.simulate_keystrokes("enter");
    assert!(
        cx.debug_bounds("join-error").is_some(),
        "Join opens with input focus"
    );
    click(cx, "join-back");
    assert!(cx.debug_bounds("terminal").is_some());
    cx.simulate_keystrokes("q");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !runtime.input().ends_with(b"q") {
        assert!(
            Instant::now() < deadline,
            "Back must restore terminal input"
        );
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(10));
    }
    click(cx, "room-trigger");
    click(cx, "room-join");
    cx.write_to_clipboard(ClipboardItem::new_string("Sentinel".into()));
    cx.simulate_keystrokes("secondary-a secondary-c");
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(""),
        "each Join starts with an empty field"
    );
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("terminal").is_some());
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    cx.run_until_parked();
}
