use super::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

type Samples = Rc<RefCell<Vec<(f64, f32, usize)>>>;

fn record(start: Instant, samples: Samples, window: &mut Window, cx: &mut Context<SeerWindow>) {
    cx.on_next_frame(window, move |view, window, cx| {
        if let Some(grid) = view.input.grid {
            samples.borrow_mut().push((
                start.elapsed().as_secs_f64() * 1000.,
                f32::from(grid.origin.x),
                grid.cols,
            ));
        }
        if start.elapsed() < Duration::from_millis(450) {
            record(start, samples, window, cx);
            cx.notify();
        }
    });
}

pub(super) async fn exercise(
    handle: gpui::WindowHandle<SeerWindow>,
    evidence: &std::path::Path,
    capture: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.update(|cx| cx.activate(true)).unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(200))
        .await;
    let reduced = top_bar::reduced_motion();
    let mut csv = String::from("motion,frame,elapsed_ms,terminal_x,columns\n");
    for name in ["hide", "show", "hide-repeat", "show-repeat"] {
        let samples = Rc::new(RefCell::new(Vec::new()));
        handle
            .update(cx, |view, window, cx| {
                if name == "hide" {
                    view.appearance.settings.hide_top_bar = false;
                    view.appearance.settings.hide_lines = false;
                    view.keys.mode = Mode::Keyboard;
                    window.focus(&view.keys.tabs);
                }
                assert!(
                    window.is_window_active(),
                    "native shortcut requires the active window"
                );
                if let Some(grid) = view.input.grid {
                    samples
                        .borrow_mut()
                        .push((0., f32::from(grid.origin.x), grid.cols));
                }
                record(Instant::now(), samples.clone(), window, cx);
                post_shortcut();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(530))
            .await;
        {
            let samples = samples.borrow();
            assert!(!samples.is_empty());
            let expected_x = if name.starts_with("hide") { 12. } else { 260. };
            assert_eq!(samples.last().unwrap().1, expected_x);
            let changes = samples
                .windows(2)
                .filter(|pair| pair[0].2 != pair[1].2)
                .count();
            if reduced {
                assert!(changes <= 1);
                assert!(samples.iter().all(|(_, x, _)| *x == 12. || *x == 260.));
            } else {
                assert_eq!(changes, 1, "the native grid must resize once for {name}");
            }
            assert!(
                samples
                    .windows(2)
                    .all(|pair| { pair[0].2 == pair[1].2 || pair[1].1 == expected_x })
            );
            for (index, (elapsed, x, cols)) in samples.iter().enumerate() {
                csv.push_str(&format!("{name},{index},{elapsed:.3},{x:.3},{cols}\n"));
            }
        }
        handle
            .update(cx, |view, _, cx| {
                view.keys.mode = Mode::Pointer;
                cx.notify();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(80))
            .await;
        let result = std::process::Command::new(capture)
            .arg(std::process::id().to_string())
            .arg(evidence)
            .arg(format!("toggle-{name}"))
            .output()
            .unwrap();
        assert!(result.status.success());
    }
    std::fs::write(evidence.join("sidebar-frames.csv"), csv).unwrap();
    std::fs::write(evidence.join("reduced-motion.txt"), reduced.to_string()).unwrap();
}

#[repr(C)]
struct KeyboardEvent {
    _private: [u8; 0],
}

// Core Graphics defines CGEventRef as a pointer to the opaque __CGEvent structure.
unsafe impl objc2::RefEncode for KeyboardEvent {
    const ENCODING_REF: objc2::Encoding =
        objc2::Encoding::Pointer(&objc2::Encoding::Struct("__CGEvent", &[]));
}

fn post_shortcut() {
    use objc2::{class, msg_send, runtime::AnyObject};
    use std::ffi::c_void;
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *const c_void,
            key: u16,
            down: bool,
        ) -> *const KeyboardEvent;
        fn CGEventSetFlags(event: *const KeyboardEvent, flags: u64);
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: *const c_void);
    }
    // The fixture runs on the AppKit thread. AppKit keeps each posted event after this block.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        for down in [true, false] {
            let event = CGEventCreateKeyboardEvent(std::ptr::null(), 0x0b, down);
            assert!(!event.is_null());
            CGEventSetFlags(event, 1 << 20);
            let native: *mut AnyObject = msg_send![class!(NSEvent), eventWithCGEvent: event];
            assert!(!native.is_null());
            let _: () = msg_send![app, postEvent: native, atStart: false];
            CFRelease(event.cast());
        }
    }
}
