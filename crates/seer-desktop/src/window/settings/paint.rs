use super::*;
use crate::menu::button;

impl SeerWindow {
    pub(in crate::window) fn settings_page(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let compact = window.viewport_size().width < px(560.);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(chrome::bar(cx).pl(px(20.)).child(chrome::lights()))
            .child(
                div()
                    .id("settings-page")
                    .debug_selector(|| "settings-page".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(if compact { 24. } else { 40. }))
                    .child(
                        div()
                            .max_w(px(560.))
                            .mx_auto()
                            .child(
                                button(
                                    "settings-back",
                                    "Back",
                                    &self.appearance.back,
                                    None,
                                    self.keys.mode,
                                    window,
                                )
                                .w(px(64.))
                                .on_click(cx.listener(
                                    |view, _, window, cx| view.close_settings(window, cx),
                                )),
                            )
                            .child(
                                div()
                                    .mt(px(24.))
                                    .text_size(px(22.))
                                    .line_height(px(28.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Settings"),
                            )
                            .child(
                                div()
                                    .mt(px(24.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(24.))
                                    .child(self.glass_control(window, cx))
                                    .child(self.picture_control(window, cx))
                                    .children(self.appearance.error.as_ref().map(|error| {
                                        detail(error).text_color(rgb(palette::DANGER))
                                    })),
                            ),
                    ),
            )
    }

    fn glass_control(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let enabled = self.appearance.settings.glass;
        div()
            .flex()
            .items_center()
            .gap(px(16.))
            .child(
                div()
                    .flex_1()
                    .child(div().font_weight(FontWeight::MEDIUM).child("Dark glass"))
                    .child(detail("A soft picture behind the window.")),
            )
            .child(
                div()
                    .id("glass-toggle")
                    .debug_selector(|| "glass-toggle".into())
                    .track_focus(&self.appearance.toggle)
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
                    .when(self.keys.ring(&self.appearance.toggle, window), |control| {
                        control.child(sidebar::outline())
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .w(px(24.))
                            .text_color(rgb(palette::MUTED))
                            .child(if enabled { "On" } else { "Off" }),
                    )
                    .child(crate::menu::switch_track(enabled))
                    .on_click(cx.listener(|view, _, _, cx| view.toggle_glass(cx))),
            )
    }

    fn picture_control(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let appearance = &self.appearance;
        let picture = appearance
            .settings
            .picture
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "Default picture".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
        div()
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Background picture"),
            )
            .child(detail(&picture))
            .when(appearance.fallback, |row| {
                row.child(
                    detail("Picture unavailable. Using the default picture.")
                        .id("picture-fallback")
                        .debug_selector(|| "picture-fallback".into()),
                )
            })
            .child(
                div()
                    .mt(px(12.))
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
                        .when(appearance.busy, |button| button.opacity(0.6))
                        .on_click(
                            cx.listener(|view, _, window, cx| view.choose_picture(window, cx)),
                        ),
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
                            .when(appearance.busy, |button| button.opacity(0.6))
                            .on_click(
                                cx.listener(|view, _, window, cx| view.reset_picture(window, cx)),
                            ),
                        )
                    }),
            )
    }
}

fn detail(text: &str) -> Div {
    div()
        .mt(px(4.))
        .text_color(rgb(palette::MUTED))
        .child(text.to_owned())
}
