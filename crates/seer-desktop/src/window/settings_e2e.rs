use gpui::{Modifiers, TestAppContext};

use super::input_e2e::{Runtime, open_view};

#[gpui::test]
fn appearance_survives_reopen_and_missing_picture_keeps_the_window(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.run_until_parked();
    let click = |cx: &mut gpui::VisualTestContext, id| {
        let bounds = cx.debug_bounds(id).expect("control must be visible");
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.run_until_parked();
    };
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
    assert!(cx.debug_bounds("settings-page").is_some());
    assert!(cx.debug_bounds("picture-fallback").is_some());
    click(cx, "default-picture");
    assert!(seer::Settings::load_from(&path).unwrap().picture.is_none());
    click(cx, "settings-back");
    assert!(cx.debug_bounds("terminal").is_some());
    cx.simulate_keystrokes("k");
    super::watch_e2e::pump(cx);
    assert!(runtime.input().ends_with(b"k"));
}
