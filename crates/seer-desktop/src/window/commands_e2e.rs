use super::input_e2e::{Runtime, open_view};
use super::watch_e2e::pump;
use super::*;
use gpui::{Modifiers, TestAppContext, VisualTestContext, point, size};

fn keys(mac: &'static str, linux: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        linux
    }
}

fn click(cx: &mut VisualTestContext, label: &'static str) {
    let bounds = cx.debug_bounds(label).expect("control must be visible");
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
}

#[gpui::test]
fn window_commands_open_settings_and_menus_without_shell_input(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    pump(cx);
    cx.simulate_click(point(px(300.), px(100.)), Modifiers::default());
    cx.simulate_keystrokes(keys("cmd-,", "ctrl-super-,"));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-dialog").is_some());
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes(keys("cmd-shift-m", "ctrl-alt-super-m"));
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-menu").is_some());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("room-confirm").is_some(),
        "first room action must have focus"
    );
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes(keys("cmd-shift-e", "ctrl-alt-super-e"));
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(
            view.people_menu.is_none(),
            "Enter must pick the first person"
        )
    });
    view.update(cx, |view, cx| {
        view.room = room::RoomUi::new(None, cx);
        cx.notify();
    });
    cx.simulate_click(point(px(300.), px(100.)), Modifiers::default());
    cx.simulate_keystrokes(keys("cmd-shift-i", "ctrl-alt-super-i"));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("invite-primary").is_none(),
        "no room cannot invite"
    );
    if cfg!(target_os = "macos") {
        cx.simulate_keystrokes("cmd-shift-r");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.room.modal(), "macOS cannot host")
        });
    }
    pump(cx);
    assert_eq!(
        runtime.input(),
        b"",
        "unavailable actions must consume their keys"
    );
    cx.simulate_keystrokes("z");
    pump(cx);
    assert_eq!(runtime.input(), b"z", "the shell must keep working");
}

#[gpui::test]
fn window_command_shortcuts_can_be_changed_saved_reloaded_and_reset(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    pump(cx);
    for name in [
        "Open Settings",
        "Start a room",
        "Invite a person",
        "Open person menu",
        "Open room menu",
    ] {
        click(cx, "settings-trigger");
        click(cx, "settings-shortcuts");
        let index = shortcuts::SHORTCUTS
            .iter()
            .position(|shortcut| shortcut.name == name)
            .expect("new action must be in Settings");
        let original: &'static str = format!(
            "{name}: {}",
            shortcuts::SHORTCUTS[index].keys(shortcuts::Platform::current())
        )
        .leak();
        for _ in 0..shortcuts::SHORTCUTS.len() + 3 {
            cx.simulate_keystrokes("tab");
            cx.run_until_parked();
            if cx.update(|window, cx| {
                view.read(cx).appearance.shortcut_focus[index].is_focused(window)
            }) {
                break;
            }
        }
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("ctrl-alt-super-y");
        cx.run_until_parked();
        let changed: &'static str =
            format!("{name}: {}", keys("Ctrl Alt Cmd Y", "Ctrl Alt Super Y")).leak();
        assert!(cx.debug_bounds(changed).is_some());
        let path = runtime.directory.path().join("settings.toml");
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains(keys("ctrl-alt-cmd-y", "ctrl-alt-super-y"))
        );
        cx.simulate_keystrokes("escape");
        view.update(cx, |view, cx| {
            view.appearance = crate::appearance::Appearance::load(Some(path), cx);
            cx.notify();
        });
        if name == "Open Settings" {
            cx.simulate_keystrokes("ctrl-alt-super-y");
            cx.run_until_parked();
            assert!(cx.debug_bounds("settings-dialog").is_some());
        } else {
            click(cx, "settings-trigger");
        }
        click(cx, "settings-shortcuts");
        for _ in 0..shortcuts::SHORTCUTS.len() + 3 {
            cx.simulate_keystrokes("tab");
            cx.run_until_parked();
            if cx.update(|window, cx| {
                view.read(cx).appearance.shortcut_focus[index].is_focused(window)
            }) {
                break;
            }
        }
        assert!(cx.debug_bounds(changed).is_some(), "saved keys must reload");
        cx.simulate_keystrokes("tab enter");
        cx.run_until_parked();
        assert!(cx.debug_bounds(original).is_some());
        cx.simulate_keystrokes("escape");
    }
}
