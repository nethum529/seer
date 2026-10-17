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
                .hover(|row| row.bg(rgb(palette::HOVER)))
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

    pub(super) fn shortcut_rows(&self, window: &Window) -> Vec<AnyElement> {
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
                    let shortcut = &shortcuts::SHORTCUTS[index];
                    let keys = shortcut.keys(shortcuts::Platform::current());
                    let label = format!("{}: {keys}", shortcut.name);
                    let focus = &self.appearance.shortcut_focus[index];
                    div()
                        .id(("shortcut", index))
                        .debug_selector(move || label.clone())
                        .track_focus(focus)
                        .relative()
                        .flex_none()
                        .min_h(px(32.))
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .when(window.viewport_size().width < px(NARROW), |row| {
                            row.min_h(px(52.))
                                .flex_col()
                                .items_start()
                                .justify_center()
                                .gap(px(4.))
                        })
                        .when(self.keys.ring(focus, window), |row| {
                            row.child(sidebar::outline())
                                .debug_selector(|| "shortcut-focused".into())
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .when(window.viewport_size().width < px(NARROW), |label| {
                                    label.flex_none()
                                })
                                .font_weight(FontWeight::MEDIUM)
                                .child(crate::label::tooltip(shortcut.name.into())),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(13.))
                                .line_height(px(18.))
                                .text_color(rgb(palette::MUTED))
                                .child(crate::label::tooltip(keys.into())),
                        )
                        .into_any_element()
                }
            })
            .collect()
    }
}
