use super::*;
use gpui::AnyElement;

impl SeerWindow {
    pub(super) fn settings_sections(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        [
            ("settings-appearance", "Appearance", Section::Appearance),
            ("settings-shortcuts", "Shortcuts", Section::Shortcuts),
            ("settings-processes", "Processes", Section::Processes),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (id, name, section))| {
            let focus = &self.appearance.sections[index];
            div()
                .id(id)
                .debug_selector(move || id.into())
                .track_focus(focus)
                .relative()
                .h(px(32.))
                .px(px(8.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .when(self.appearance.section == section, |row| {
                    row.bg(rgb(palette::CHIP))
                })
                .hover(|row| row.bg(crate::appearance::fill(palette::HOVER, cx)))
                .when(self.keys.ring(focus, window), |row| {
                    row.child(sidebar::outline())
                })
                .child(crate::label::text(name))
                .on_click(cx.listener(move |view, event, window, cx| {
                    view.keys.mode = if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                        Mode::Keyboard
                    } else {
                        Mode::Pointer
                    };
                    window.focus(&view.appearance.sections[index]);
                    view.settings_section(section, cx);
                }))
                .into_any_element()
        })
        .collect()
    }

    pub(super) fn shortcut_rows(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        shortcuts::rows()
            .into_iter()
            .enumerate()
            .map(|(row, item)| match item {
                shortcuts::Row::Heading(name) => div()
                    .id(("shortcut-context", row))
                    .flex_none()
                    .min_h(px(32.))
                    .flex()
                    .items_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(crate::label::tooltip(name.into())),
                    )
                    .into_any_element(),
                shortcuts::Row::Binding(index) => {
                    self.shortcut_row(index, window, cx).into_any_element()
                }
            })
            .collect()
    }

    fn shortcut_row(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let shortcut = &shortcuts::SHORTCUTS[index];
        let capturing = self.appearance.capture == Some(index);
        let keys = if capturing {
            "Press new keys".to_owned()
        } else {
            shortcut.effective_keys(shortcuts::Platform::current(), cx)
        };
        let label = format!("{}: {keys}", shortcut.name);
        let focus = &self.appearance.shortcut_focus[index];
        let narrow = window.viewport_size().width < px(NARROW);
        let message = self
            .appearance
            .shortcut_error
            .as_ref()
            .filter(|(at, _)| *at == index)
            .or_else(|| {
                self.appearance
                    .shortcut_load_errors
                    .iter()
                    .find(|(at, _)| *at == index)
            });
        let control = div()
            .id(("shortcut", index))
            .track_focus(focus)
            .relative()
            .min_h(px(32.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(px(16.))
            .cursor_pointer()
            .when(narrow, |row| {
                row.min_h(px(52.))
                    .flex_col()
                    .items_start()
                    .justify_center()
                    .gap(px(4.))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when(narrow, |name| name.flex_none())
                    .font_weight(FontWeight::MEDIUM)
                    .child(crate::label::tooltip(shortcut.name.into())),
            )
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .flex_wrap()
                    .child(
                        div()
                            .id(("shortcut-keys", index))
                            .relative()
                            .min_w_0()
                            .max_w_full()
                            .debug_selector(move || keys.clone())
                            .min_h(px(32.))
                            .px(px(8.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .bg(rgb(palette::CHIP))
                            .text_color(rgb(palette::MUTED))
                            .hover(|key| key.bg(rgb(palette::CHIP_2)))
                            .when(self.keys.ring(focus, window), |key| {
                                key.child(
                                    sidebar::outline().debug_selector(|| "shortcut-focused".into()),
                                )
                            })
                            .child(crate::label::tooltip(
                                if capturing {
                                    "Press new keys".to_owned()
                                } else {
                                    shortcut.effective_keys(shortcuts::Platform::current(), cx)
                                }
                                .into(),
                            )),
                    )
                    .when(capturing, |controls| {
                        controls.child(
                            crate::menu::button(
                                "shortcut-cancel",
                                "Cancel",
                                &self.appearance.shortcut_cancel,
                                None,
                                self.keys.mode,
                                window,
                            )
                            .px(px(8.))
                            .on_click(cx.listener(
                                |view, _, window, cx| {
                                    view.cancel_capture(window, cx);
                                    cx.stop_propagation();
                                },
                            )),
                        )
                    })
                    .when(!capturing && shortcut.changed(cx), |controls| {
                        controls.child(
                            crate::menu::button(
                                "shortcut-reset",
                                "Reset",
                                &self.appearance.shortcut_reset[index],
                                None,
                                self.keys.mode,
                                window,
                            )
                            .id(("shortcut-reset", index))
                            .debug_selector(move || format!("shortcut-reset-{index}"))
                            .px(px(8.))
                            .on_click(cx.listener(
                                move |view, _, window, cx| {
                                    view.change_shortcut(index, None, window, cx);
                                    cx.stop_propagation();
                                },
                            )),
                        )
                    }),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |view, event, window, cx| {
                view.keys.mode = if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                    Mode::Keyboard
                } else {
                    Mode::Pointer
                };
                view.start_capture(index, window, cx);
            }));
        div()
            .id(("shortcut-row", index))
            .debug_selector(move || label.clone())
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(control)
            .when(capturing || message.is_some(), |row| {
                let text = message
                    .map_or("Escape cancels", |(_, text)| text.as_str())
                    .to_owned();
                let selector = text.clone();
                row.child(
                    div()
                        .id(("shortcut-message", index))
                        .debug_selector(move || selector.clone())
                        .min_h(px(32.))
                        .flex()
                        .items_center()
                        .text_color(rgb(if message.is_some() {
                            palette::DANGER
                        } else {
                            palette::MUTED
                        }))
                        .child(
                            div()
                                .id("shortcut-error")
                                .when(message.is_some(), |error| {
                                    error.debug_selector(|| "shortcut-error".into())
                                })
                                .min_w_0()
                                .child(crate::label::tooltip(text.into())),
                        ),
                )
            })
    }
}
