use super::{
    SeerWindow,
    appearance_native_e2e::{pause, post_mouse, snapshot},
    settings::sliders,
};
use gpui::{Bounds, Keystroke, Pixels, WindowHandle, point, px};
use std::{fs, path::Path};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    settings: &Path,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| view.open_settings(false, window, cx))
        .unwrap();
    pause(handle, cx).await;
    snapshot(capture, evidence, "legacy-loaded");
    fs::copy(settings, evidence.join("legacy-saved.toml")).unwrap();
    let loaded = seer::Settings::load_from(settings).unwrap();
    assert!(loaded.glass, "legacy settings must migrate to glass mode");
    assert_eq!(loaded.picture_brightness, 0);
    assert_eq!(loaded.glass_darkness, 100);
    assert_eq!(loaded.glass_blur, seer::Settings::default().glass_blur);
    assert!(!loaded.hide_top_bar);
    handle
        .update(cx, |view, _, _| {
            assert!(view.appearance.settings.glass);
            assert_eq!(
                [0, 1, 2].map(|index| sliders::value(&view.appearance.settings, index)),
                [0, 100, 12]
            );
        })
        .unwrap();
    let saved = fs::read(settings).unwrap();
    let modified = fs::metadata(settings).unwrap().modified().unwrap();
    seer::Settings::load_from(settings).unwrap();
    assert_eq!(
        fs::metadata(settings).unwrap().modified().unwrap(),
        modified
    );
    let mut samples = Vec::new();
    handle
        .update(cx, |view, window, _| {
            super::appearance_pixel_samples::sliders(view, window, "legacy-loaded", &mut samples);
            super::appearance_pixel_samples::neighbors(view, window, "legacy-loaded", &mut samples);
        })
        .unwrap();
    let viewport = handle
        .update(cx, |_, window, _| {
            Bounds::new(point(px(0.), px(0.)), window.viewport_size())
        })
        .unwrap();
    let mut results = Vec::new();
    for index in 0..3 {
        handle
            .update(cx, |view, window, cx| {
                window.focus(&view.appearance.sliders[index]);
                view.keys.mode = if index == 2 {
                    post_mouse(window, point(px(900.), px(620.)), 5);
                    crate::sidebar::Mode::Pointer
                } else {
                    crate::sidebar::Mode::Keyboard
                };
                cx.notify();
            })
            .unwrap();
        pause(handle, cx).await;
        let before = format!("legacy-noop-{index}-before");
        let after = format!("legacy-noop-{index}-after");
        snapshot(capture, evidence, &before);
        handle
            .update(cx, |view, window, cx| {
                adjust_noop(view, index, window, cx);
            })
            .unwrap();
        pause(handle, cx).await;
        snapshot(capture, evidence, &after);
        let changed = changed_pixels(evidence, &before, &after, viewport);
        assert_eq!(changed, 0, "a no-op must leave the window unchanged");
        assert_eq!(fs::read(settings).unwrap(), saved);
        assert_eq!(
            fs::metadata(settings).unwrap().modified().unwrap(),
            modified
        );
        results.push(serde_json::json!({"slider":sliders::IDS[index],"noop_changed_pixels":changed,"saved_bytes_unchanged":true,"modified_time_unchanged":true}));
    }
    results.extend(pointer_feedback(handle, settings, evidence, capture, cx).await);
    fs::write(
        evidence.join("appearance-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
    fs::write(
        evidence.join("legacy-checks.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}

async fn pointer_feedback(
    handle: WindowHandle<SeerWindow>,
    settings: &Path,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) -> Vec<serde_json::Value> {
    let saved = fs::read(settings).unwrap();
    let modified = fs::metadata(settings).unwrap().modified().unwrap();
    let mut results = Vec::new();
    for index in 0..3 {
        handle
            .update(cx, |view, window, cx| {
                view.keys.mode = crate::sidebar::Mode::Pointer;
                post_mouse(window, point(px(900.), px(620.)), 5);
                cx.notify();
            })
            .unwrap();
        pause(handle, cx).await;
        let idle = format!("legacy-idle-{index}");
        let hover = format!("legacy-hover-{index}");
        let pressed = format!("legacy-pressed-{index}");
        let released = format!("legacy-released-{index}");
        snapshot(capture, evidence, &idle);
        for (kind, name) in [(5, &hover), (1, &pressed), (2, &released)] {
            handle
                .update(cx, |view, window, _| {
                    post_mouse(window, thumb(view, index), kind);
                    if kind == 2 {
                        post_mouse(window, point(px(900.), px(620.)), 5);
                    }
                })
                .unwrap();
            pause(handle, cx).await;
            snapshot(capture, evidence, name);
        }
        let track = handle
            .update(cx, |view, _, _| view.appearance.slider_bounds[index].get())
            .unwrap();
        let hover_changed = changed_pixels(evidence, &idle, &hover, track);
        let pressed_changed = changed_pixels(evidence, &hover, &pressed, track);
        assert!(hover_changed > 0, "hover must give visible feedback");
        assert!(pressed_changed > 0, "press must give visible feedback");
        assert_eq!(changed_pixels(evidence, &idle, &released, track), 0);
        assert_eq!(fs::read(settings).unwrap(), saved);
        assert_eq!(
            fs::metadata(settings).unwrap().modified().unwrap(),
            modified
        );
        results.push(serde_json::json!({"slider":sliders::IDS[index],"hover_changed_pixels":hover_changed,"pressed_changed_pixels":pressed_changed,"released_changed_pixels":0}));
    }
    results
}

fn adjust_noop(
    view: &mut SeerWindow,
    index: usize,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<SeerWindow>,
) {
    if index < 2 {
        let key = if index == 0 { "home" } else { "end" };
        view.settings_key(&Keystroke::parse(key).unwrap(), window, cx);
    } else {
        let position = thumb(view, index);
        post_mouse(window, position, 1);
        post_mouse(window, position, 2);
        post_mouse(window, point(px(900.), px(620.)), 5);
    }
}

fn thumb(view: &SeerWindow, index: usize) -> gpui::Point<Pixels> {
    let bounds = view.appearance.slider_bounds[index].get();
    let fraction = f32::from(sliders::value(&view.appearance.settings, index))
        / if index == 2 { 40. } else { 100. };
    bounds.origin + point(px(8.) + (bounds.size.width - px(16.)) * fraction, px(12.))
}

fn changed_pixels(evidence: &Path, before: &str, after: &str, bounds: Bounds<Pixels>) -> usize {
    let read = |name| {
        let image = image::open(evidence.join(format!("{name}.png"))).unwrap();
        let scale = image.width() / 960;
        assert!(scale == 1 || scale == 2);
        let device = |value| (f32::from(value) * scale as f32).round() as u32;
        image
            .crop_imm(
                device(bounds.left()),
                device(bounds.top()),
                device(bounds.size.width),
                device(bounds.size.height),
            )
            .to_rgba8()
    };
    let before = read(before);
    let after = read(after);
    before
        .pixels()
        .zip(after.pixels())
        .filter(|(before, after)| before != after)
        .count()
}
