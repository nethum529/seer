use super::*;
use crate::menu::{self, button};
use gpui::AnyElement;

impl SeerWindow {
    pub(in crate::window) fn settings_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self
            .appearance
            .motion
            .as_ref()
            .is_some_and(|motion| motion.gone())
        {
            self.appearance.motion = None;
        }
        let motion = self.appearance.motion.as_ref()?;
        if motion.animating() {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
        let opacity = motion.value();
        let compact = window.viewport_size().width < px(600.);
        let narrow = window.viewport_size().width < px(480.);
        let inset = if compact { 20. } else { 24. };
        let width = 720_f32.min(f32::from(window.viewport_size().width) - 32.);
        let height: f32 = if self.appearance.section == Section::Appearance {
            560.
        } else {
            400.
        };
        let height = height.min(f32::from(window.viewport_size().height) - 48.);
        let card = menu::sheet("settings-dialog", width, cx)
            .h(px(height))
            .flex()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| {
                    view.cancel_capture(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(if compact { 120. } else { 176. }))
                    .py(px(inset))
                    .px(px(if compact { 12. } else { 16. }))
                    .border_r_1()
                    .border_color(crate::appearance::fill(palette::LINE_2, cx))
                    .child(
                        div()
                            .h(px(32.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::label::text("Settings")),
                    )
                    .child(
                        div()
                            .mt(px(16.))
                            .children(self.settings_sections(window, cx)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .p(px(inset))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(16.))
                            .when(narrow, |header| header.gap(px(12.)))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::label::text(
                                        if self.appearance.section == Section::Appearance {
                                            "Appearance"
                                        } else {
                                            "Shortcuts"
                                        },
                                    )),
                            )
                            .child(
                                button(
                                    "settings-close",
                                    "",
                                    &self.appearance.close,
                                    None,
                                    self.keys.mode,
                                    window,
                                )
                                .w(px(32.))
                                .px(px(0.))
                                .bg(gpui::rgba(0))
                                .child(chrome::icon("close", palette::FAINT))
                                .on_click(cx.listener(
                                    |view, event, window, cx| {
                                        view.close_settings(
                                            matches!(event, gpui::ClickEvent::Keyboard(_)),
                                            window,
                                            cx,
                                        )
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .id("settings-content")
                            .debug_selector(|| "settings-content".into())
                            .mt(px(20.))
                            .mx(px(-4.))
                            .p(px(4.))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.appearance.scroll)
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .when(self.appearance.section == Section::Shortcuts, |content| {
                                content.children(self.shortcut_rows(window, cx))
                            })
                            .when(self.appearance.section == Section::Appearance, |content| {
                                content
                                    .children(
                                        (0..3).map(|index| self.adjustment_row(index, window, cx)),
                                    )
                                    .child(self.appearance_control(
                                        Toggle::UniformGlass,
                                        window,
                                        cx,
                                    ))
                                    .child(self.appearance_control(Toggle::HideTopBar, window, cx))
                                    .child(
                                        self.appearance_control(Toggle::HideLines, window, cx)
                                            .mb(px(12.)),
                                    )
                                    .child(self.picture_control())
                                    .child(self.picture_actions(window, cx))
                                    .children(self.appearance.error.as_ref().map(|error| {
                                        detail(error).text_color(rgb(palette::DANGER))
                                    }))
                            }),
                    )
                    .when(self.appearance.section == Section::Shortcuts, |content| {
                        content.child(
                            div()
                                .id("shortcut-scroll-cue")
                                .debug_selector(|| "shortcut-scroll-cue".into())
                                .mt(px(8.))
                                .h(px(32.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .text_color(rgb(palette::MUTED))
                                .child(crate::label::text("Scroll to see all")),
                        )
                    }),
            );
        Some(
            menu::scrim("settings-scrim")
                .opacity(opacity)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, window, cx| {
                        view.close_settings(false, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(card)
                .child(self.adjustment_events(cx))
                .into_any_element(),
        )
    }

    fn appearance_control(&self, toggle: Toggle, window: &Window, cx: &mut Context<Self>) -> Div {
        let appearance = &self.appearance;
        let (id, label, enabled, focus) = match toggle {
            Toggle::UniformGlass => (
                "uniform-glass-toggle",
                "Uniform glass",
                appearance.settings.uniform_glass,
                &appearance.uniform,
            ),
            Toggle::HideTopBar => (
                "hide-top-bar-toggle",
                "Hide top bar",
                appearance.settings.hide_top_bar,
                &appearance.hide_bar,
            ),
            Toggle::HideLines => (
                "hide-lines-toggle",
                "Hide lines",
                appearance.settings.hide_lines,
                &appearance.hide_lines,
            ),
        };
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(FontWeight::MEDIUM)
                    .child(crate::label::text(label)),
            )
            .child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .track_focus(focus)
                    .relative()
                    .h(px(32.))
                    .px(px(6.))
                    .rounded(px(8.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .hover(|control| control.opacity(0.92))
                    .active(|control| control.opacity(0.84))
                    .when(self.keys.ring(focus, window), |control| {
                        control.child(sidebar::outline())
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(window.viewport_size().width >= px(480.), |control| {
                        control.child(
                            div()
                                .w(px(24.))
                                .text_color(rgb(palette::MUTED))
                                .child(crate::label::text(if enabled { "On" } else { "Off" })),
                        )
                    })
                    .child(crate::menu::switch_track(enabled))
                    .on_click(
                        cx.listener(move |view, _, _, cx| view.toggle_appearance(toggle, cx)),
                    ),
            )
    }

    fn picture_control(&self) -> Div {
        let appearance = &self.appearance;
        div()
            .flex_none()
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(crate::label::text("Background picture")),
            )
            .children(
                appearance
                    .settings
                    .picture
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| {
                        let name = name.to_string_lossy();
                        let name = if appearance.fallback {
                            format!("{name} (Unavailable)")
                        } else {
                            name.into_owned()
                        };
                        detail(&name)
                            .id("picture-file")
                            .when(appearance.fallback, |file| {
                                file.debug_selector(|| "picture-fallback".into())
                                    .text_color(rgb(palette::DANGER))
                            })
                    }),
            )
    }

    fn picture_actions(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let appearance = &self.appearance;
        let narrow = window.viewport_size().width < px(480.);
        div()
            .flex_none()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .child(
                button(
                    "choose-picture",
                    if appearance.busy {
                        "Loading..."
                    } else {
                        "Choose picture"
                    },
                    &appearance.choose,
                    None,
                    self.keys.mode,
                    window,
                )
                .when(narrow, |button| button.px(px(8.)))
                .when(appearance.busy, |button| button.opacity(0.6))
                .on_click(cx.listener(|view, _, window, cx| view.choose_picture(window, cx))),
            )
            .when(appearance.settings.picture.is_some(), |actions| {
                actions.child(
                    button(
                        "default-picture",
                        "Use default",
                        &appearance.reset,
                        None,
                        self.keys.mode,
                        window,
                    )
                    .when(narrow, |button| button.px(px(8.)))
                    .when(appearance.busy, |button| button.opacity(0.6))
                    .on_click(cx.listener(|view, _, window, cx| view.reset_picture(window, cx))),
                )
            })
    }
}

fn detail(text: &str) -> Div {
    div()
        .mt(px(4.))
        .text_color(rgb(palette::MUTED))
        .child(crate::label::text(text.to_owned()))
}
