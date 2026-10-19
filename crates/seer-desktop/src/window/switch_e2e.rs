use super::input_e2e::{Runtime, open_view};
use super::watch_e2e::pump;
use super::*;
use gpui::{TestAppContext, VisualTestContext};

fn keys(mac: &'static str, linux: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        linux
    }
}

fn shown(view: &Entity<SeerWindow>, cx: &VisualTestContext) -> usize {
    view.read_with(cx, |view, _| {
        let Content::Terminal(link, _) = &view.content else {
            panic!("terminal required")
        };
        link.terminals()
            .iter()
            .position(|terminal| Some(terminal.pane.as_str()) == link.shown())
            .unwrap()
    })
}

#[gpui::test]
fn terminal_switching_wraps_uses_sidebar_order_and_consumes_missing_numbers(
    cx: &mut TestAppContext,
) {
    let runtime = Runtime::start_with_output("\x1b[?1049hReady\r\n");
    let (view, cx) = open_view(&runtime, cx);
    pump(cx);
    cx.simulate_keystrokes(keys(
        "cmd-9 cmd-right cmd-left",
        "ctrl-super-9 ctrl-super-f6 ctrl-super-shift-f6",
    ));
    pump(cx);
    assert_eq!(shown(&view, cx), 0);
    assert_eq!(
        runtime.input(),
        b"",
        "one terminal still consumes switch keys"
    );
    cx.simulate_keystrokes(keys("cmd-n", "ctrl-shift-n"));
    pump(cx);
    assert_eq!(shown(&view, cx), 1);
    for (key, at) in [
        (keys("cmd-right", "ctrl-super-f6"), 0),
        (keys("cmd-left", "ctrl-super-shift-f6"), 1),
        (keys("cmd-1", "ctrl-super-1"), 0),
        (keys("cmd-9", "ctrl-super-9"), 0),
    ] {
        cx.simulate_keystrokes(key);
        pump(cx);
        assert_eq!(shown(&view, cx), at, "{key} must select in sidebar order");
        assert_eq!(runtime.input(), b"", "switch keys must not reach the PTY");
    }
    for _ in 2..10 {
        cx.simulate_keystrokes(keys("cmd-n", "ctrl-shift-n"));
        pump(cx);
    }
    for number in 1..=9 {
        cx.simulate_keystrokes(&format!("{}-{number}", keys("cmd", "ctrl-super")));
        pump(cx);
        assert_eq!(shown(&view, cx), number - 1);
    }
    cx.simulate_keystrokes(keys("cmd-right", "ctrl-super-f6"));
    pump(cx);
    assert_eq!(
        shown(&view, cx),
        9,
        "next must include terminals after nine"
    );
    cx.simulate_input("typed-after-switch");
    pump(cx);
    assert_eq!(runtime.input(), b"typed-after-switch");
    cx.simulate_keystrokes(keys("cmd-b", "ctrl-shift-b"));
    cx.simulate_keystrokes(keys("cmd-1", "ctrl-super-1"));
    pump(cx);
    assert_eq!(shown(&view, cx), 0, "hidden sidebar keeps switching");
    cx.simulate_keystrokes("ctrl-c->\x03 ctrl-tab alt-1->1");
    pump(cx);
    assert!(runtime.input().starts_with(b"typed-after-switch\x03"));
    assert!(runtime.input().ends_with(keys("1", "\x1b1").as_bytes()));
}

#[gpui::test]
fn terminal_switch_shortcuts_can_be_changed_saved_and_reset(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_resize(gpui::size(px(960.), px(640.)));
    pump(cx);
    cx.simulate_keystrokes(keys("cmd-n", "ctrl-shift-n"));
    pump(cx);
    let button = cx.debug_bounds("settings-trigger").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.simulate_keystrokes("shift-tab enter");
    for _ in 0..5 {
        cx.simulate_keystrokes("tab");
    }
    cx.run_until_parked();
    let label = keys("Next terminal: Cmd Right", "Next terminal: Ctrl Super F6");
    let button = cx
        .debug_bounds(label)
        .expect("Next terminal must be in Settings");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.simulate_keystrokes("ctrl-shift-y escape ctrl-shift-y");
    pump(cx);
    assert_eq!(shown(&view, cx), 0);
    let path = runtime.directory.path().join("settings.toml");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("ctrl-shift-y")
    );
    cx.simulate_keystrokes(keys("cmd-right", "ctrl-super-f6"));
    pump(cx);
    assert_eq!(shown(&view, cx), 0, "old keys must stop switching");
    assert!(
        !runtime.input().is_empty(),
        "released keys must reach the PTY"
    );
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path.clone()), cx);
        cx.notify();
    });
    cx.simulate_keystrokes("ctrl-shift-y");
    pump(cx);
    assert_eq!(shown(&view, cx), 1);
    let button = cx.debug_bounds("settings-trigger").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    let button = cx.debug_bounds("settings-shortcuts").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    for _ in 0..5 {
        cx.simulate_keystrokes("tab");
    }
    cx.run_until_parked();
    let button = cx.debug_bounds("shortcut-reset-11").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds(label).is_some());
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes(keys("cmd-right", "ctrl-super-f6"));
    pump(cx);
    assert_eq!(shown(&view, cx), 0);
}
