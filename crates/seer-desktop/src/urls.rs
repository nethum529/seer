use std::{cell::RefCell, rc::Rc};

use gpui::{App, AsyncApp, WindowHandle};

use crate::window::SeerWindow;

pub(crate) struct Links {
    pending: Vec<String>,
    window: Option<(WindowHandle<SeerWindow>, AsyncApp)>,
}

impl Links {
    pub(crate) fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            pending: std::env::args()
                .skip(1)
                .filter(|arg| arg.starts_with("seer://"))
                .collect(),
            window: None,
        }))
    }

    pub(crate) fn open(&mut self, urls: Vec<String>) {
        let urls = urls.into_iter().filter(|url| url.starts_with("seer://"));
        let Some((handle, cx)) = &mut self.window else {
            self.pending.extend(urls);
            return;
        };
        for url in urls {
            let _ = handle.update(cx, |view, window, cx| view.open_invite(&url, window, cx));
        }
    }

    pub(crate) fn attach(&mut self, window: WindowHandle<SeerWindow>, cx: &mut App) {
        self.window = Some((window, cx.to_async()));
        for url in std::mem::take(&mut self.pending) {
            let _ = window.update(cx, |view, window, cx| view.open_invite(&url, window, cx));
        }
    }
}
