use gpui::{App, Window};
use objc2::rc::Retained;
use objc2_app_kit::NSView;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(crate) struct NativeFrame(Retained<NSView>);

impl NativeFrame {
    pub(crate) fn new(window: &Window) -> Option<Self> {
        let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw()
        else {
            return None;
        };
        // GPUI supplies a live NSView; the retained view and queued work stay on the main thread.
        unsafe { Retained::retain(handle.ns_view.cast::<NSView>().as_ptr()) }.map(Self)
    }

    pub(crate) fn request(&self, cx: &App) {
        let view = self.0.clone();
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
