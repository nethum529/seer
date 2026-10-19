use super::{SeerWindow, settings::sliders};
use gpui::{WindowHandle, point, px};
use std::{
    cell::RefCell,
    fs,
    path::Path,
    process::Command,
    rc::Rc,
    time::{Duration, Instant},
};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| {
            view.appearance.settings = seer::Settings::default();
            view.appearance.load_picture();
            view.open_settings(true, window, cx);
        })
        .unwrap();
    pause(handle, cx).await;
    let mut samples = Vec::new();
    let mut timings = Vec::new();
    for source in ["default", "chosen"] {
        if source == "chosen" {
            let path = evidence.join("chosen.png");
            image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/backdrop.jpg"))
                .unwrap()
                .fliph()
                .save(&path)
                .unwrap();
            let picture = crate::backdrop::read(&path);
            handle
                .update(cx, |view, window, cx| {
                    view.set_picture(path, picture, window, cx)
                })
                .unwrap();
        }
        for (state, values) in [
            ("minimum", [0, 0, 0]),
            ("maximum", [100, 100, 40]),
            ("default", [18, 0, 12]),
        ] {
            handle
                .update(cx, |view, _, cx| {
                    for (index, value) in values.into_iter().enumerate() {
                        view.set_adjustment(index, value, cx);
                    }
                    view.appearance.scroll.scroll_to_top_of_item(0);
                })
                .unwrap();
            pause(handle, cx).await;
            let name = format!("appearance-{source}-{state}");
            handle
                .update(cx, |view, window, _| {
                    super::appearance_pixel_samples::sliders(view, window, &name, &mut samples);
                })
                .unwrap();
            snapshot(capture, evidence, &name);
        }
        for index in 0..3 {
            handle
                .update(cx, |view, _, cx| {
                    for (index, value) in [18, 0, 12].into_iter().enumerate() {
                        view.set_adjustment(index, value, cx);
                    }
                    view.appearance.scroll.scroll_to_top_of_item(index);
                })
                .unwrap();
            pause(handle, cx).await;
            for (end, value) in [("min", 0), ("max", if index == 2 { 40 } else { 100 })] {
                handle
                    .update(cx, |view, _, cx| view.set_adjustment(index, value, cx))
                    .unwrap();
                pause(handle, cx).await;
                snapshot(
                    capture,
                    evidence,
                    &format!("appearance-{source}-slider-{index}-{end}"),
                );
            }
            let bounds = handle
                .update(cx, |view, _, _| view.appearance.slider_bounds[index].get())
                .unwrap();
            let position = bounds.origin + point(px(8.), bounds.size.height / 2.);
            handle
                .update(cx, |_, window, _| post_mouse(window, position, 1))
                .unwrap();
            pause(handle, cx).await;
            for step in 1..=20 {
                let next =
                    position + point((bounds.size.width - px(16.)) * (step as f32 / 20.), px(0.));
                let recorded = Rc::new(RefCell::new(None));
                let frame = recorded.clone();
                let start = Instant::now();
                handle
                    .update(cx, |_, window, cx| {
                        post_mouse(window, next, 6);
                        let entity = cx.entity();
                        let expected = ((if index == 2 { 40. } else { 100. }) * step as f32 / 20.)
                            .round() as u8;
                        record_frame(entity, index, expected, start, frame, window);
                        window.refresh();
                    })
                    .unwrap();
                for _ in 0..100 {
                    cx.background_executor()
                        .timer(Duration::from_millis(2))
                        .await;
                    if recorded.borrow().is_some() {
                        break;
                    }
                }
                let elapsed = recorded.borrow().expect("drag must produce a frame");
                timings.push(serde_json::json!({"source":source,"slider":sliders::IDS[index],"step":step,"next_frame_ms":elapsed}));
            }
            handle
                .update(cx, |_, window, _| post_mouse(window, position, 2))
                .unwrap();
            pause(handle, cx).await;
            handle
                .update(cx, |view, window, cx| {
                    window.focus(&view.appearance.sliders[index]);
                    for key in ["home", "right", "right"] {
                        view.settings_key(&gpui::Keystroke::parse(key).unwrap(), window, cx);
                    }
                    assert_eq!(sliders::value(&view.appearance.settings, index), 2);
                })
                .unwrap();
            pause(handle, cx).await;
            snapshot(
                capture,
                evidence,
                &format!("appearance-{source}-focus-{index}"),
            );
            handle
                .update(cx, |view, window, cx| {
                    window.focus(&view.appearance.slider_reset[index]);
                    view.settings_key(&gpui::Keystroke::parse("enter").unwrap(), window, cx);
                    assert_eq!(
                        sliders::value(&view.appearance.settings, index),
                        [18, 0, 12][index]
                    );
                })
                .unwrap();
        }
    }
    handle
        .update(cx, |view, _, cx| {
            for (index, value) in [18, 0, 12].into_iter().enumerate() {
                view.set_adjustment(index, value, cx);
            }
            view.appearance.settings.picture = None;
            view.appearance.load_picture();
            view.appearance.scroll.scroll_to_top_of_item(3);
            cx.notify();
        })
        .unwrap();
    pause(handle, cx).await;
    handle
        .update(cx, |view, window, _| {
            super::appearance_pixel_samples::neighbors(
                view,
                window,
                "appearance-neighbors",
                &mut samples,
            );
        })
        .unwrap();
    snapshot(capture, evidence, "appearance-neighbors");
    for width in [321., 479.] {
        handle
            .update(cx, |view, window, cx| {
                window.resize(gpui::size(px(width), px(640.)));
                view.appearance.scroll.scroll_to_top_of_item(0);
                cx.notify();
            })
            .unwrap();
        pause(handle, cx).await;
        let name = format!("appearance-narrow-{width}");
        handle
            .update(cx, |view, window, _| {
                super::appearance_pixel_samples::sliders(view, window, &name, &mut samples);
            })
            .unwrap();
        snapshot(capture, evidence, &name);
    }
    fs::write(
        evidence.join("appearance-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
    fs::write(
        evidence.join("appearance-timings.json"),
        serde_json::to_vec_pretty(&timings).unwrap(),
    )
    .unwrap();
    handle
        .update(cx, |view, window, cx| {
            window.resize(gpui::size(px(960.), px(640.)));
            view.settings_key(&gpui::Keystroke::parse("escape").unwrap(), window, cx);
        })
        .unwrap();
}

pub(super) async fn pause(handle: WindowHandle<SeerWindow>, cx: &mut gpui::AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |view, window, cx| {
            window.refresh();
            if let Some(frame) = &view.input.frame {
                frame.request(cx);
            }
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(200))
        .await;
}

pub(super) fn snapshot(capture: &Path, evidence: &Path, name: &str) {
    let output = Command::new(capture)
        .arg(std::process::id().to_string())
        .arg(evidence)
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn record_frame(
    view: gpui::Entity<SeerWindow>,
    index: usize,
    expected: u8,
    start: Instant,
    recorded: Rc<RefCell<Option<f64>>>,
    window: &mut gpui::Window,
) {
    window.on_next_frame(move |window, cx| {
        if sliders::value(&view.read(cx).appearance.settings, index) == expected {
            *recorded.borrow_mut() = Some(start.elapsed().as_secs_f64() * 1000.);
        } else {
            record_frame(view, index, expected, start, recorded, window);
            window.refresh();
        }
    });
}

pub(super) fn post_mouse(window: &gpui::Window, position: gpui::Point<gpui::Pixels>, kind: usize) {
    use objc2::{class, msg_send, runtime::AnyObject};
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).unwrap().as_raw()
    else {
        panic!("AppKit required")
    };
    // SAFETY: GPUI owns the live NSView. This fixture posts retained events on the AppKit thread.
    unsafe {
        let native = handle.ns_view.cast::<NSView>().as_ref();
        let target = native.window().unwrap();
        let mut location = native.frame().origin;
        location.x = f64::from(f32::from(position.x));
        location.y = f64::from(f32::from(window.viewport_size().height - position.y));
        let event: *mut AnyObject = msg_send![class!(NSEvent),
            mouseEventWithType: kind,
            location: location,
            modifierFlags: 0usize,
            timestamp: 0f64,
            windowNumber: target.windowNumber(),
            context: std::ptr::null::<AnyObject>(),
            eventNumber: 0isize,
            clickCount: 1isize,
            pressure: 0.5f32
        ];
        assert!(!event.is_null());
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, postEvent: event, atStart: false];
    }
}
