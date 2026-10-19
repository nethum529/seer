use super::input_e2e::{Runtime, open_view};
use gpui::{Modifiers, TestAppContext, VisualTestContext, px, size};

fn click(cx: &mut VisualTestContext, label: &'static str) {
    let bounds = cx.debug_bounds(label).expect("control must be visible");
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
}

#[gpui::test]
fn changed_shortcuts_save_reject_conflicts_and_reset(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    cx.run_until_parked();
    click(cx, "settings-trigger");
    cx.simulate_keystrokes("shift-tab enter tab enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("Press new keys").is_some());
    let conflict = if cfg!(target_os = "macos") {
        "cmd-b"
    } else {
        "ctrl-shift-b"
    };
    cx.simulate_keystrokes(conflict);
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("Used by Show or hide sidebar. Choose other keys.")
            .is_some()
    );
    for reserved in ["ctrl-c", "ctrl-a", "tab", "up", "a"] {
        cx.simulate_keystrokes(reserved);
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("Reserved for terminal input. Choose other keys.")
                .is_some()
        );
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-dialog").is_some());
    cx.simulate_keystrokes("ctrl-shift-y");
    assert!(!runtime.directory.path().join("settings.toml").exists());
    cx.simulate_keystrokes("enter");
    click(cx, "shortcut-cancel");
    cx.simulate_keystrokes("ctrl-shift-y");
    assert!(!runtime.directory.path().join("settings.toml").exists());
    let original = if cfg!(target_os = "macos") {
        "New terminal: Cmd N"
    } else {
        "New terminal: Ctrl Shift N"
    };
    click(cx, original);
    cx.simulate_keystrokes("ctrl-shift-y");
    cx.run_until_parked();
    assert!(cx.debug_bounds("New terminal: Ctrl Shift Y").is_some());
    let path = runtime.directory.path().join("settings.toml");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("ctrl-shift-y"));
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-n"
    } else {
        "ctrl-shift-n"
    });
    super::watch_e2e::pump(cx);
    assert!(runtime.input().ends_with(if cfg!(target_os = "macos") {
        b"\x1b[110;9u"
    } else {
        b"\x0e"
    }));
    cx.simulate_keystrokes("ctrl-shift-y");
    super::watch_e2e::pump(cx);
    view.read_with(cx, |view, _| {
        let super::Content::Terminal(link, _) = &view.content else {
            panic!("terminal required")
        };
        assert_eq!(link.terminals().len(), 2);
    });
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path.clone()), cx);
        cx.notify();
    });
    click(cx, "settings-trigger");
    click(cx, "settings-shortcuts");
    assert!(cx.debug_bounds("New terminal: Ctrl Shift Y").is_some());
    click(cx, "shortcut-reset-0");
    assert!(cx.debug_bounds(original).is_some());
    assert!(
        !std::fs::read_to_string(path)
            .unwrap()
            .contains("ctrl-shift-y")
    );
    click(
        cx,
        if cfg!(target_os = "macos") {
            "Show or hide sidebar: Cmd B"
        } else {
            "Show or hide sidebar: Ctrl Shift B"
        },
    );
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-n"
    } else {
        "ctrl-shift-n"
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("Used by New terminal. Choose other keys.")
            .is_some()
    );
    cx.simulate_keystrokes("escape escape");
}

#[gpui::test]
fn replaced_terminal_shortcut_returns_old_keys_to_pty(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    cx.run_until_parked();
    click(cx, "settings-trigger");
    click(cx, "settings-shortcuts");
    for _ in 0..crate::shortcuts::SHORTCUTS.len() {
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
        if cx
            .debug_bounds("Scroll to top: Shift Home")
            .is_some_and(|row| {
                cx.debug_bounds("shortcut-focused")
                    .is_some_and(|focused| row.contains(&focused.center()))
            })
        {
            break;
        }
    }
    cx.run_until_parked();
    click(cx, "Scroll to top: Shift Home");
    cx.simulate_keystrokes("ctrl-shift-u escape shift-home");
    super::watch_e2e::pump(cx);
    assert!(
        runtime.input().ends_with(b"\x1b[1;2H"),
        "PTY input: {:?}",
        runtime.input()
    );
    let before = runtime.input();
    cx.simulate_keystrokes("ctrl-shift-u");
    super::watch_e2e::pump(cx);
    assert_eq!(runtime.input(), before);
    view.read_with(cx, |view, _| assert!(!view.settings_open()));
}

#[gpui::test]
fn failed_shortcut_save_keeps_the_current_binding(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    cx.run_until_parked();
    click(cx, "settings-trigger");
    cx.simulate_keystrokes("shift-tab enter tab enter");
    view.update(cx, |view, _| {
        view.appearance.path = Some(runtime.directory.path().to_owned())
    });
    cx.simulate_keystrokes("ctrl-shift-y");
    cx.run_until_parked();
    assert!(cx.debug_bounds("shortcut-error").is_some());
    assert!(cx.debug_bounds("Press new keys").is_some());
    cx.simulate_keystrokes("escape escape");
    click(cx, "settings-trigger");
    click(cx, "settings-shortcuts");
    let original = if cfg!(target_os = "macos") {
        "New terminal: Cmd N"
    } else {
        "New terminal: Ctrl Shift N"
    };
    assert!(cx.debug_bounds(original).is_some());
}

#[gpui::test]
fn invalid_saved_shortcuts_can_be_replaced_in_settings(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    let platform = if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let other = if cfg!(target_os = "macos") {
        "linux"
    } else {
        "mac"
    };
    let path = runtime.directory.path().join("settings.toml");
    let mut settings = seer::Settings::default();
    for (id, keys) in [
        (format!("{platform}.new_terminal"), "ctrl-c"),
        (format!("{platform}.toggle_sidebar"), "tab"),
        (format!("{platform}.focus_sidebar"), "ctrl-shift-f"),
        (format!("{platform}.future_action"), "future-key"),
        (format!("{other}.new_terminal"), "future-key"),
    ] {
        settings.shortcut_overrides.insert(id, keys.into());
    }
    settings.save_to(&path).unwrap();
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path.clone()), cx);
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(640.)));
    cx.run_until_parked();
    click(cx, "settings-trigger");
    click(cx, "settings-shortcuts");
    click(
        cx,
        if cfg!(target_os = "macos") {
            "Cmd N"
        } else {
            "Ctrl Shift N"
        },
    );
    cx.simulate_keystrokes("ctrl-shift-y");
    cx.run_until_parked();
    assert!(cx.debug_bounds("New terminal: Ctrl Shift Y").is_some());
    click(
        cx,
        if cfg!(target_os = "macos") {
            "Cmd B"
        } else {
            "Ctrl Shift B"
        },
    );
    cx.simulate_keystrokes("ctrl-shift-u");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("Show or hide sidebar: Ctrl Shift U")
            .is_some()
    );
    let saved = seer::Settings::load_from(&path).unwrap();
    for (action, keys) in [
        ("new_terminal", "ctrl-shift-y"),
        ("toggle_sidebar", "ctrl-shift-u"),
        ("focus_sidebar", "ctrl-shift-f"),
        ("future_action", "future-key"),
    ] {
        assert_eq!(
            saved.shortcut_overrides[&format!("{platform}.{action}")],
            keys
        );
    }
    assert_eq!(
        saved.shortcut_overrides[&format!("{other}.new_terminal")],
        "future-key"
    );
    click(cx, "shortcut-reset-0");
    assert!(
        !seer::Settings::load_from(&path)
            .unwrap()
            .shortcut_overrides
            .contains_key(&format!("{platform}.new_terminal"))
    );
}
