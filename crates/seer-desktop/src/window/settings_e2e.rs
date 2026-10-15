use gpui::{Modifiers, TestAppContext, VisualTestContext, point, px, size};

use super::input_e2e::{Runtime, open_view};

#[gpui::test]
fn appearance_survives_reopen_and_missing_picture_keeps_the_window(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.run_until_parked();
    click(cx, "settings-trigger");
    assert!(cx.debug_bounds("default-picture").is_none());
    click(cx, "glass-toggle");
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
    assert!(!settings.glass);
    assert_eq!(settings.picture.as_ref(), Some(&picture));
    std::fs::remove_file(picture).unwrap();
    view.update(cx, |view, cx| {
        view.appearance = crate::appearance::Appearance::load(Some(path.clone()), cx);
        cx.notify();
    });
    click(cx, "settings-trigger");
    click(cx, "glass-toggle");
    assert!(cx.debug_bounds("settings-dialog").is_some());
    assert!(cx.debug_bounds("picture-fallback").is_some());
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

fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).expect("control must be visible");
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
}
