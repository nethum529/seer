use gpui::{App, Window};
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindowButton};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(crate) struct NativeFrame {
    view: Retained<NSView>,
    drag: Option<(f64, f64)>,
    button_origins: [Option<(f64, f64)>; 3],
}

impl NativeFrame {
    pub(crate) fn top_bar_opacity(&mut self, opacity: f32) {
        let Some(window) = self.view.window() else {
            return;
        };
        let buttons = [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ]
        .map(|kind| window.standardWindowButton(kind));
        let hidden = opacity == 0.;
        // AppKit lays out all three buttons when alpha or visibility changes.
        for (index, button) in buttons.iter().enumerate() {
            if let Some(button) = button
                && !button.isHidden()
            {
                let frame = button.frame();
                self.button_origins[index] = Some((frame.origin.x, frame.origin.y));
            }
        }
        for button in buttons.iter().flatten() {
            button.setAlphaValue(f64::from(opacity));
            if button.isHidden() != hidden {
                button.setHidden(hidden);
            }
        }
        if !hidden {
            for (index, button) in buttons.iter().enumerate() {
                if let Some(button) = button
                    && let Some((x, y)) = self.button_origins[index]
                {
                    let mut frame = button.frame();
                    frame.origin.x = x;
                    frame.origin.y = y;
                    button.setFrame(frame);
                }
            }
        }
    }

    pub(crate) fn start_drag(&mut self) {
        if let Some(event) = self.view.window().and_then(|window| window.currentEvent()) {
            let position = event.locationInWindow();
            self.drag = Some((position.x, position.y));
        }
    }

    pub(crate) fn drag(&self, cx: &App) -> bool {
        if let Some((x, y)) = self.drag
            && let Some(window) = self.view.window()
            && let Some(event) = window.currentEvent()
        {
            let position = event.locationInWindow();
            let mut frame = window.frame();
            frame.origin.x += position.x - x;
            frame.origin.y += position.y - y;
            // Moving the window calls GPUI back, so release the App borrow first.
            cx.foreground_executor()
                .spawn(async move { window.setFrameOrigin(frame.origin) })
                .detach();
            return true;
        }
        false
    }

    pub(crate) fn finish_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    pub(crate) fn new(window: &Window) -> Option<Self> {
        let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw()
        else {
            return None;
        };
        // GPUI supplies a live NSView; the retained view and queued work stay on the main thread.
        unsafe { Retained::retain(handle.ns_view.cast::<NSView>().as_ptr()) }.map(|view| Self {
            view,
            drag: None,
            button_origins: [None; 3],
        })
    }

    pub(crate) fn request(&self, cx: &App) {
        let view = self.view.clone();
        // GPUI's displayLayer callback draws and presents. Run after the App borrow ends.
        cx.foreground_executor()
            .spawn(async move {
                if let Some(layer) = view.layer() {
                    layer.setNeedsDisplay();
                    layer.displayIfNeeded();
                }
            })
            .detach();
    }
}
