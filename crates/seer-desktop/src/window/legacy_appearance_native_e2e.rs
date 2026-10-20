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
    let original = fs::read(settings).unwrap();
    let modified = fs::metadata(settings).unwrap().modified().unwrap();
    let loaded = seer::Settings::load_from(settings).unwrap();
    assert!(
        !loaded.glass,
        "legacy settings must keep Solid background on"
    );
    assert_eq!(loaded.picture_brightness, 18);
    assert_eq!(loaded.glass_darkness, 0);
    assert_eq!(loaded.glass_blur, 12);
    assert!(!loaded.hide_top_bar);
    assert_eq!(fs::read(settings).unwrap(), original);
    assert_eq!(
        fs::metadata(settings).unwrap().modified().unwrap(),
        modified
    );
    pause(handle, cx).await;
    snapshot(capture, evidence, "solid-window");
    assert_grey(evidence);
    handle
        .update(cx, |view, window, cx| view.open_settings(true, window, cx))
        .unwrap();
    pause(handle, cx).await;
    snapshot(capture, evidence, "solid-on");
    let mut samples = Vec::new();
    handle
        .update(cx, |view, window, _| {
            assert!(!view.appearance.settings.glass);
            super::appearance_pixel_samples::neighbors(view, window, "solid-on", &mut samples);
        })
        .unwrap();
    handle
        .update(cx, |view, window, cx| {
            window.activate_window();
            cx.activate(true);
            view.settings_key(&Keystroke::parse("tab").unwrap(), window, cx);
        })
        .unwrap();
    pause(handle, cx).await;
    handle
        .update(cx, |_, _, _| {
            super::sidebar_native_e2e::post_shortcut(0x31, 0)
        })
        .unwrap();
    pause(handle, cx).await;
    assert!(seer::Settings::load_from(settings).unwrap().glass);
    snapshot(capture, evidence, "solid-keyboard");
    handle
        .update(cx, |view, window, cx| {
            window.focus(&view.appearance.close);
            view.keys.mode = crate::sidebar::Mode::Pointer;
            cx.notify();
        })
        .unwrap();
    pause(handle, cx).await;
    snapshot(capture, evidence, "solid-off");
    handle
        .update(cx, |view, window, _| {
            super::appearance_pixel_samples::neighbors(view, window, "solid-off", &mut samples);
            super::appearance_pixel_samples::sliders(view, window, "solid-off", &mut samples);
        })
        .unwrap();
    click_solid(handle, cx).await;
    assert!(!seer::Settings::load_from(settings).unwrap().glass);
    fs::copy(settings, evidence.join("solid-saved.toml")).unwrap();
    click_solid(handle, cx).await;
    handle
        .update(cx, |view, _, cx| {
            let mut settings = view.appearance.settings.clone();
            settings.picture_brightness = 0;
            settings.glass_darkness = 100;
            settings.glass_blur = 12;
            assert!(view.appearance.save(settings));
            cx.notify();
        })
        .unwrap();
    pause(handle, cx).await;
    let mut results = slider_noops(handle, settings, evidence, capture, cx).await;
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

async fn slider_noops(
    handle: WindowHandle<SeerWindow>,
    settings: &Path,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) -> Vec<serde_json::Value> {
    let saved = fs::read(settings).unwrap();
    let modified = fs::metadata(settings).unwrap().modified().unwrap();
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
            .update(cx, |view, window, cx| adjust_noop(view, index, window, cx))
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
    results
}

async fn click_solid(handle: WindowHandle<SeerWindow>, cx: &mut gpui::AsyncApp) {
    handle
        .update(cx, |view, window, _| {
            let bounds = view.appearance.scroll.bounds_for_item(0).unwrap()
                + view.appearance.scroll.offset();
            let position = point(bounds.right() - px(30.), bounds.top() + px(16.));
            post_mouse(window, position, 1);
            post_mouse(window, position, 2);
            post_mouse(window, point(px(900.), px(620.)), 5);
        })
        .unwrap();
    pause(handle, cx).await;
}

fn assert_grey(evidence: &Path) {
    let image = image::open(evidence.join("solid-window.png"))
        .unwrap()
        .to_rgb8();
    let scale = image.width() / 960;
    for (x, y, value) in [(800, 500, 0x1b), (200, 300, 0x22), (800, 20, 0x22)] {
        assert_eq!(image.get_pixel(x * scale, y * scale).0, [value; 3]);
    }
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
