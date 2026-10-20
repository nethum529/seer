use gpui::{Modifiers, TestAppContext, VisualTestContext, point, px, size};

use super::input_e2e::{Runtime, open_view};

#[gpui::test]
fn solid_background_loads_and_saves_with_keyboard_and_pointer(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let path = runtime.directory.path().join("settings.toml");
    let legacy = "glass = false\npicture_brightness = 37\nglass_darkness = 61\nglass_blur = 24\n";
    std::fs::write(&path, legacy).unwrap();
    let (view, cx) = open_view(&runtime, cx);
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), legacy);
    click(cx, "settings-trigger");
    assert!(cx.debug_bounds("solid-background-toggle").is_some());
    for id in [
        "picture-brightness",
        "glass-darkness",
        "glass-blur",
        "choose-picture",
    ] {
        assert!(cx.debug_bounds(id).is_none());
    }
    cx.simulate_keystrokes("tab");
    activate(cx, "space");
    assert!(cx.debug_bounds("picture-brightness").is_some());
    assert!(seer::Settings::load_from(&path).unwrap().glass);
    click(cx, "solid-background-toggle");
    assert!(!seer::Settings::load_from(&path).unwrap().glass);
    for id in [
        "uniform-glass-toggle",
        "hide-top-bar-toggle",
        "hide-lines-toggle",
    ] {
        click(cx, id);
    }
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path.clone()), cx);
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "settings-trigger");
    let saved = seer::Settings::load_from(&path).unwrap();
    assert!(!saved.glass);
    assert_eq!(
        [
            saved.picture_brightness,
            saved.glass_darkness,
            saved.glass_blur
        ],
        [37, 61, 24]
    );
    assert!(!saved.uniform_glass);
    assert!(!saved.hide_top_bar);
    assert!(saved.hide_lines);
    cx.simulate_keystrokes("tab");
    activate(cx, "enter");
    assert!(cx.debug_bounds("choose-picture").is_some());
    assert!(seer::Settings::load_from(&path).unwrap().glass);
    let mut converted = saved;
    converted.glass = true;
    converted.picture_brightness = 0;
    converted.glass_darkness = 100;
    converted.save_to(&path).unwrap();
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path.clone()), cx);
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "settings-trigger");
    click(cx, "solid-background-toggle");
    assert!(!seer::Settings::load_from(&path).unwrap().glass);
}

#[gpui::test]
fn appearance_slider_values_survive_restart(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.run_until_parked();
    click(cx, "settings-trigger");
    for id in ["picture-brightness", "glass-darkness", "glass-blur"] {
        click(cx, id);
        cx.simulate_keystrokes("home right right right");
        cx.run_until_parked();
    }
    let path = runtime.directory.path().join("settings.toml");
    let saved = std::fs::read_to_string(&path).unwrap();
    for setting in [
        "picture_brightness = 3",
        "glass_darkness = 3",
        "glass_blur = 3",
    ] {
        assert!(saved.contains(setting), "{setting} must be saved");
    }
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path.clone()), cx);
        cx.notify();
    });
    click(cx, "settings-trigger");
    for _ in 0..3 {
        cx.simulate_keystrokes("tab tab");
        cx.simulate_keystrokes("right");
        cx.run_until_parked();
    }
    let saved = std::fs::read_to_string(&path).unwrap();
    for setting in [
        "picture_brightness = 4",
        "glass_darkness = 4",
        "glass_blur = 4",
    ] {
        assert!(
            saved.contains(setting),
            "{setting} must come back after restart"
        );
    }
}

#[gpui::test]
fn appearance_preferences_survive_restart(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.run_until_parked();
    click(cx, "settings-trigger");
    click(cx, "hide-lines-toggle");
    click(cx, "uniform-glass-toggle");
    click(cx, "hide-top-bar-toggle");
    let path = runtime.directory.path().join("settings.toml");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("hide_lines = true"));
    assert!(saved.contains("uniform_glass = false"));
    assert!(saved.contains("hide_top_bar = false"));
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path), cx);
        cx.notify();
    });
    click(cx, "settings-trigger");
    click(cx, "hide-lines-toggle");
    click(cx, "uniform-glass-toggle");
    click(cx, "hide-top-bar-toggle");
    let saved = std::fs::read_to_string(runtime.directory.path().join("settings.toml")).unwrap();
    assert!(saved.contains("hide_lines = false"));
    assert!(saved.contains("uniform_glass = true"));
    assert!(saved.contains("hide_top_bar = true"));
}

#[gpui::test]
fn appearance_survives_reopen_and_missing_picture_keeps_the_window(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.run_until_parked();
    click(cx, "settings-trigger");
    assert!(cx.debug_bounds("default-picture").is_none());
    click(cx, "picture-brightness");
    cx.simulate_keystrokes("home");
    let picture = runtime.directory.path().join("picture.jpg");
    std::fs::write(&picture, include_bytes!("../../assets/backdrop.jpg")).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_picture(picture.clone(), crate::backdrop::read(&picture), window, cx)
        });
    });
    cx.run_until_parked();
    let path = runtime.directory.path().join("settings.toml");
    let settings = seer::Settings::load_from(&path).unwrap();
    assert_eq!(settings.picture_brightness, 0);
    assert_eq!(settings.picture.as_ref(), Some(&picture));
    std::fs::remove_file(picture).unwrap();
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path.clone()), cx);
        cx.notify();
    });
    click(cx, "settings-trigger");
    click(cx, "picture-brightness");
    cx.simulate_keystrokes("end");
    assert!(cx.debug_bounds("settings-dialog").is_some());
    assert!(cx.debug_bounds("picture-fallback").is_some());
    let viewport = cx.debug_bounds("settings-content").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-400.))),
        modifiers: Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    click(cx, "default-picture");
    assert!(seer::Settings::load_from(&path).unwrap().picture.is_none());
    click(cx, "settings-close");
    assert!(cx.debug_bounds("terminal").is_some());
    cx.simulate_keystrokes("k");
    super::watch_e2e::pump(cx);
    assert!(runtime.input().ends_with(b"k"));
}

#[gpui::test]
fn settings_dialog_dismissal_returns_input_to_the_terminal(cx: &mut TestAppContext) {
    let runtime = Runtime::start_with_output("\x1b[?1003h\x1b[?1006hReady\r\n");
    std::fs::write(
        runtime.directory.path().join("settings.toml"),
        "glass = false\n",
    )
    .unwrap();
    let (_, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    cx.run_until_parked();
    for (dismiss, key) in [("escape", "a"), ("close", "b"), ("outside", "c")] {
        click(cx, "settings-trigger");
        assert!(cx.debug_bounds("settings-dialog").is_some());
        assert!(cx.debug_bounds("settings-appearance").is_some());
        let expected = runtime.input();
        super::input_e2e::exercise_overlay_pointer(
            cx,
            "settings-dialog",
            point(px(300.), px(100.)),
        );
        cx.simulate_keystrokes("x");
        super::watch_e2e::pump(cx);
        assert_eq!(runtime.input(), expected);
        match dismiss {
            "escape" => cx.simulate_keystrokes("escape"),
            "close" => click(cx, "settings-close"),
            _ => cx.simulate_click(point(px(4.), px(64.)), Modifiers::default()),
        }
        cx.run_until_parked();
        cx.simulate_keystrokes(key);
        super::watch_e2e::pump(cx);
        assert!(runtime.input().ends_with(key.as_bytes()));
        std::thread::sleep(std::time::Duration::from_millis(150));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn narrow_settings_keeps_each_keyboard_control_in_view(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    std::fs::write(
        runtime.directory.path().join("settings.toml"),
        "glass = true\npicture = '/missing/a-long-missing-background-picture.jpg'\n",
    )
    .unwrap();
    let (_, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(321.), px(484.)));
    cx.run_until_parked();
    click(cx, "settings-trigger");
    for (key, id) in [
        ("tab", "solid-background-toggle"),
        ("tab", "picture-brightness"),
        ("tab", "reset-brightness"),
        ("tab", "glass-darkness"),
        ("tab", "reset-darkness"),
        ("tab", "glass-blur"),
        ("tab", "reset-blur"),
        ("tab", "uniform-glass-toggle"),
        ("tab", "hide-top-bar-toggle"),
        ("tab", "hide-lines-toggle"),
        ("tab", "choose-picture"),
        ("tab", "default-picture"),
        ("shift-tab", "choose-picture"),
        ("shift-tab", "hide-lines-toggle"),
        ("shift-tab", "hide-top-bar-toggle"),
        ("shift-tab", "uniform-glass-toggle"),
        ("shift-tab", "reset-blur"),
        ("shift-tab", "glass-blur"),
        ("shift-tab", "reset-darkness"),
        ("shift-tab", "glass-darkness"),
        ("shift-tab", "reset-brightness"),
        ("shift-tab", "picture-brightness"),
    ] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        let viewport = cx.debug_bounds("settings-content").unwrap();
        let control = cx.debug_bounds(id).unwrap();
        assert!(
            viewport.contains(&control.origin) && viewport.contains(&control.bottom_right()),
            "{id} must be fully visible: {control:?} inside {viewport:?}"
        );
    }
}

#[gpui::test]
fn shortcuts_are_readable_and_settings_keeps_terminal_input(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (_, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    cx.run_until_parked();
    click(cx, "settings-trigger");
    cx.simulate_keystrokes("shift-tab shift-tab enter");
    cx.run_until_parked();
    let new_label = if cfg!(target_os = "macos") {
        "New terminal: Cmd N"
    } else {
        "New terminal: Ctrl Shift N"
    };
    assert!(cx.debug_bounds(new_label).is_some());
    assert!(cx.debug_bounds("Activate button: Enter / Space").is_none());
    let cue = cx.debug_bounds("shortcut-scroll-cue").unwrap();
    let dialog = cx.debug_bounds("settings-dialog").unwrap();
    assert!(dialog.contains(&cue.origin) && dialog.contains(&cue.bottom_right()));
    let expected = runtime.input();
    cx.simulate_keystrokes("n");
    super::watch_e2e::pump(cx);
    assert_eq!(runtime.input(), expected);
    for _ in 0..90 {
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
        let viewport = cx.debug_bounds("settings-content").unwrap();
        if let Some(row) = cx.debug_bounds("shortcut-focused") {
            assert!(viewport.contains(&row.origin) && viewport.contains(&row.bottom_right()));
        }
    }
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("z");
    super::watch_e2e::pump(cx);
    assert!(runtime.input().ends_with(b"z"));
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).expect("control must be visible");
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
}

fn activate(cx: &mut VisualTestContext, key: &str) {
    cx.run_until_parked();
    cx.simulate_keystrokes(key);
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse(key).unwrap(),
    });
    cx.run_until_parked();
}
