use std::{path::PathBuf, sync::Arc};

use gpui::StatefulInteractiveElement;

use super::*;
use crate::appearance::Section;
use crate::{backdrop, motion::Motion};

mod capture;
mod paint;
pub(crate) mod processes;
mod shortcut_list;
pub(super) mod sliders;

#[derive(Clone, Copy)]
enum Toggle {
    SolidBackground,
    UniformGlass,
    HideTopBar,
    HideLines,
}

impl SeerWindow {
    pub(crate) fn settings_open(&self) -> bool {
        self.appearance
            .motion
            .as_ref()
            .is_some_and(|motion| !motion.closing())
    }

    pub(super) fn settings_button(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        tip::on_hover(
            chrome::settings()
                .id("settings-trigger")
                .debug_selector(|| "settings-trigger".into()),
            "Settings".into(),
            self.ui_font.clone(),
            self.keys.hover() && self.appearance.motion.is_none(),
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
        .on_click(cx.listener(|view, event, window, cx| {
            view.open_settings(matches!(event, gpui::ClickEvent::Keyboard(_)), window, cx)
        }))
    }

    pub(super) fn open_settings(
        &mut self,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.room.modal() || self.invite_open() {
            return;
        }
        self.finish_name(true, false, window, cx);
        self.menu = None;
        self.people_menu = None;
        self.close_room_menu(true, false, window, cx);
        self.keys.mode = if keyboard {
            Mode::Keyboard
        } else {
            Mode::Pointer
        };
        match &mut self.appearance.motion {
            Some(motion) => motion.reopen(keyboard, 200),
            motion => *motion = Some(Motion::open(keyboard, 200)),
        }
        self.appearance.return_focus = window.focused(cx);
        self.input.reset();
        if self.appearance.section == Section::Processes {
            self.load_processes(None, cx);
        }
        window.focus(&self.appearance.close);
        cx.notify();
    }

    fn close_settings(&mut self, keyboard: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.appearance.slider_drag.take().is_some() {
            self.save_adjustments(cx);
        }
        self.appearance.capture = None;
        self.appearance.shortcut_error = None;
        if let Some(motion) = &mut self.appearance.motion {
            motion.close(keyboard, 140);
        }
        let focus = if matches!(self.content, Content::Terminal(..)) {
            &self.input.focus
        } else {
            self.appearance
                .return_focus
                .as_ref()
                .unwrap_or(&self.input.focus)
        };
        window.focus(focus);
        cx.notify();
    }

    pub(crate) fn settings_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings_open() {
            return false;
        }
        self.keys.mode = Mode::Keyboard;
        if self.process_sheet_key(key, window, cx) {
            return true;
        }
        if self.capture_key(key, window, cx) {
            cx.notify();
            return true;
        }
        if self.adjustment_key(key, window, cx) {
            return true;
        }
        match key.key.as_str() {
            "escape" => self.close_settings(true, window, cx),
            "tab" => {
                let controls = self.settings_focus_order(cx);
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
                self.reveal_settings_focus(controls[next]);
            }
            "enter" | "space" => {
                if let Some(index) = self
                    .appearance
                    .sections
                    .iter()
                    .position(|focus| focus.is_focused(window))
                {
                    self.settings_section(
                        [Section::Appearance, Section::Shortcuts, Section::Processes][index],
                        cx,
                    );
                } else if self.appearance.section == Section::Shortcuts {
                    self.shortcut_activation(window, cx);
                } else if self.appearance.section == Section::Processes {
                    self.process_activation(window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
        true
    }

    fn settings_focus_order(&self, cx: &gpui::App) -> Vec<&gpui::FocusHandle> {
        let appearance = &self.appearance;
        let mut controls = vec![&appearance.close];
        if appearance.section == Section::Appearance {
            controls.push(&appearance.solid);
            if appearance.settings.glass {
                controls.extend(self.adjustment_focus());
            }
            controls.extend([
                &appearance.uniform,
                &appearance.hide_bar,
                &appearance.hide_lines,
            ]);
            if appearance.settings.glass {
                controls.push(&appearance.choose);
                if appearance.settings.picture.is_some() {
                    controls.push(&appearance.reset);
                }
            }
            controls.extend(appearance.sections.iter());
        } else if appearance.section == Section::Processes {
            controls.extend(appearance.sections.iter());
            controls.push(&appearance.processes.refresh);
            if appearance.processes.can_clean() {
                controls.push(&appearance.processes.clean);
            }
            controls.extend(
                appearance
                    .processes
                    .rows
                    .iter()
                    .filter(|row| row.can_stop())
                    .map(|row| &row.focus),
            );
        } else {
            controls.extend(appearance.sections.iter());
            for row in shortcuts::rows() {
                if let shortcuts::Row::Binding(index) = row {
                    controls.push(&appearance.shortcut_focus[index]);
                    if shortcuts::SHORTCUTS[index].changed(cx) {
                        controls.push(&appearance.shortcut_reset[index]);
                    }
                }
            }
        }
        controls
    }

    fn reveal_settings_focus(&self, focus: &gpui::FocusHandle) {
        let appearance = &self.appearance;
        if appearance.section == Section::Processes {
            if let Some(index) = appearance
                .processes
                .rows
                .iter()
                .position(|row| &row.focus == focus)
            {
                appearance.scroll.scroll_to_item(index + 1);
            } else if focus == &appearance.processes.refresh || focus == &appearance.processes.clean
            {
                appearance.scroll.scroll_to_item(0);
            }
        } else if appearance.section == Section::Shortcuts {
            let index = appearance
                .shortcut_focus
                .iter()
                .position(|item| item == focus)
                .or_else(|| {
                    appearance
                        .shortcut_reset
                        .iter()
                        .position(|item| item == focus)
                });
            if let Some(row) = shortcuts::rows()
                .iter()
                .position(|row| matches!(row, shortcuts::Row::Binding(at) if Some(*at) == index))
            {
                appearance.scroll.scroll_to_item(row);
            }
        } else if focus == &appearance.solid {
            appearance.scroll.scroll_to_item(0);
        } else if !self.reveal_adjustment(focus)
            && let Some(index) = [
                &appearance.uniform,
                &appearance.hide_bar,
                &appearance.hide_lines,
                &appearance.choose,
                &appearance.reset,
            ]
            .iter()
            .position(|item| *item == focus)
        {
            appearance.scroll.scroll_to_item(if index >= 3 {
                8
            } else {
                index + if appearance.settings.glass { 4 } else { 1 }
            });
        }
    }

    pub(in crate::window) fn settings_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.appearance.capture = None;
        self.appearance.shortcut_error = None;
        self.appearance.section = section;
        if section == Section::Processes {
            self.load_processes(None, cx);
        }
        self.appearance.scroll.set_offset(gpui::Point::default());
        cx.notify();
    }

    fn toggle_appearance(&mut self, toggle: Toggle, window: &mut Window, cx: &mut Context<Self>) {
        let mut settings = self.appearance.settings.clone();
        match toggle {
            Toggle::SolidBackground => settings.glass = !settings.glass,
            Toggle::UniformGlass => settings.uniform_glass = !settings.uniform_glass,
            Toggle::HideTopBar => settings.hide_top_bar = !settings.hide_top_bar,
            Toggle::HideLines => settings.hide_lines = !settings.hide_lines,
        }
        if self.appearance.save(settings) && matches!(toggle, Toggle::SolidBackground) {
            window.focus(&self.appearance.solid);
            self.appearance.scroll.set_offset(gpui::Point::default());
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
            let picture = backdrop::default_picture();
            self.appearance.replace_picture(picture, window);
            self.appearance.fallback = false;
            if self.appearance.reset.is_focused(window) {
                window.focus(&self.appearance.choose);
                self.appearance.scroll.scroll_to_item(8);
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
        loaded: io::Result<Arc<backdrop::Picture>>,
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
