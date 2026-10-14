use std::{path::PathBuf, sync::Arc};

use gpui::{RenderImage, StatefulInteractiveElement};

use super::*;
use crate::backdrop;

mod paint;

impl SeerWindow {
    pub(crate) fn settings_open(&self) -> bool {
        self.appearance.open
    }

    pub(super) fn settings_button(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        tip::on_hover(
            chrome::settings()
                .id("settings-trigger")
                .debug_selector(|| "settings-trigger".into()),
            "Settings".into(),
            self.ui_font.clone(),
            self.keys.hover(),
        )
        .track_focus(&self.appearance.trigger)
        .relative()
        .rounded(px(6.))
        .hover(|style| style.bg(rgb(palette::HOVER)))
        .active(|style| style.opacity(0.84))
        .when(self.keys.ring(&self.appearance.trigger, window), |button| {
            button.child(sidebar::outline())
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|view, _, window, cx| view.open_settings(window, cx)))
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.room.modal() || self.invite_open() {
            return;
        }
        self.finish_name(true, false, window, cx);
        self.menu = None;
        self.people_menu = None;
        self.close_room_menu(true, false, window, cx);
        self.appearance.return_focus = window.focused(cx);
        self.appearance.open = true;
        self.input.reset();
        window.focus(&self.appearance.back);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.appearance.open = false;
        window.focus(
            self.appearance
                .return_focus
                .as_ref()
                .unwrap_or(&self.input.focus),
        );
        cx.notify();
    }

    pub(super) fn settings_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.appearance.open {
            return false;
        }
        self.keys.mode = Mode::Keyboard;
        match key.key.as_str() {
            "escape" => self.close_settings(window, cx),
            "tab" => {
                let mut controls = vec![
                    &self.appearance.back,
                    &self.appearance.toggle,
                    &self.appearance.choose,
                ];
                if self.appearance.settings.picture.is_some() {
                    controls.push(&self.appearance.reset);
                }
                let at = controls
                    .iter()
                    .position(|focus| focus.is_focused(window))
                    .unwrap_or(0);
                let next = if key.modifiers.shift {
                    (at + controls.len() - 1) % controls.len()
                } else {
                    (at + 1) % controls.len()
                };
                window.focus(controls[next]);
            }
            _ => {}
        }
        cx.notify();
        true
    }

    fn toggle_glass(&mut self, cx: &mut Context<Self>) {
        let mut settings = self.appearance.settings.clone();
        settings.glass = !settings.glass;
        if self.appearance.save(settings)
            && self.appearance.settings.glass
            && self.appearance.picture.is_none()
        {
            self.appearance.load_picture();
        }
        cx.notify();
    }

    fn reset_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.appearance.busy || self.appearance.settings.picture.is_none() {
            return;
        }
        let mut settings = self.appearance.settings.clone();
        settings.picture = None;
        if self.appearance.save(settings) {
            let picture = self
                .appearance
                .settings
                .glass
                .then(backdrop::default_picture)
                .flatten();
            self.appearance.replace_picture(picture, window);
            self.appearance.fallback = false;
            if self.appearance.reset.is_focused(window) {
                window.focus(&self.appearance.choose);
            }
        }
        cx.notify();
    }

    fn choose_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.appearance.busy {
            return;
        }
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a picture".into()),
        });
        self.appearance.busy = true;
        cx.spawn_in(window, async move |view, cx| {
            let selected = paths.await;
            match selected {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        let picture_path = path.clone();
                        let loaded = cx
                            .background_executor()
                            .spawn(async move { backdrop::read(&picture_path) })
                            .await;
                        let _ = view.update_in(cx, |view, window, cx| {
                            view.set_picture(path, loaded, window, cx)
                        });
                        return;
                    }
                }
                Ok(Ok(None)) => {}
                _ => {
                    let _ = view.update(cx, |view, _| {
                        view.appearance.error = Some("Could not open the picture picker.".into())
                    });
                }
            }
            let _ = view.update(cx, |view, cx| {
                view.appearance.busy = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn set_picture(
        &mut self,
        path: PathBuf,
        loaded: io::Result<Arc<RenderImage>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.appearance.busy = false;
        match loaded {
            Ok(picture) => {
                let mut settings = self.appearance.settings.clone();
                settings.picture = Some(path);
                if self.appearance.save(settings) {
                    self.appearance.replace_picture(Some(picture), window);
                    self.appearance.fallback = false;
                }
            }
            Err(_) => {
                self.appearance.error = Some(
                    "Could not read this picture. Choose a JPEG, PNG, or WebP file under 32 MB."
                        .into(),
                )
            }
        }
        cx.notify();
    }
}
