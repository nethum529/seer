use gpui::prelude::*;
use gpui::{
    Context, Div, FocusHandle, FontWeight, KeyDownEvent, MouseButton, Render, Stateful, Window,
    div, px, relative, rgb, rgba,
};

use super::InviteSheet;
use crate::{chrome, palette, sidebar};

impl InviteSheet {
    fn button(&self, id: &'static str, focus: &FocusHandle, window: &Window) -> Stateful<Div> {
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .relative()
            .when(self.keyboard && focus.is_focused(window), |button| {
                button.child(sidebar::outline().when(id == "invite-primary", |outline| {
                    outline.inset(px(-3.)).rounded(px(11.))
                }))
            })
            .track_focus(focus)
            .h(px(32.))
            .px(px(14.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .text_size(px(13.))
            .line_height(px(18.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|style| style.opacity(0.92))
            .active(|style| style.opacity(0.84))
    }

    fn picker(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let selected = self.selected();
        let segments = div()
            .id("invite-duration")
            .relative()
            .when(
                self.keyboard && self.duration_focus.is_focused(window),
                |segments| segments.child(sidebar::outline()),
            )
            .track_focus(&self.duration_focus)
            .flex()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(palette::LINE_2))
            .bg(rgb(palette::WINDOW))
            .children(
                ["1 hour", "1 day", "7 days", "Custom"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, label)| {
                        div()
                            .id(("invite-time", index))
                            .h(px(30.))
                            .flex_1()
                            .min_w_0()
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if selected == index {
                                palette::TEXT
                            } else {
                                palette::MUTED
                            }))
                            .when(selected == index, |segment| segment.bg(rgb(palette::CHIP)))
                            .when(!self.pending, |segment| {
                                segment
                                    .cursor_pointer()
                                    .hover(|style| style.opacity(0.92))
                                    .active(|style| style.opacity(0.84))
                                    .on_click(cx.listener(move |sheet, _, window, cx| {
                                        sheet.keyboard = false;
                                        sheet.pick(index, window, cx);
                                    }))
                            })
                            .child(label)
                    }),
            );
        div()
            .child(label("Expires after"))
            .child(segments)
            .when(self.custom, |body| {
                body.child(
                    div()
                        .mt(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(div().w(px(72.)).flex_none().child(self.input.clone()))
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(16.))
                                .text_color(rgb(palette::MUTED))
                                .child("Hours, 1 to 168"),
                        ),
                )
            })
            .when(self.input.read(cx).invalid, |body| {
                body.children(self.error().map(|error| error.mt(px(8.))))
            })
    }

    fn code(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> Div {
        let Some(invitation) = &self.invitation else {
            return div();
        };
        let (id, title, line) = if index == 0 {
            (
                "copy-join",
                "For a friend who has Seer",
                invitation.join_line(),
            )
        } else {
            (
                "copy-install",
                "For a friend who needs Seer",
                invitation.install_line(),
            )
        };
        let copied = self.copied[index].is_some();
        let button = self
            .button(id, &self.copy_focus[index], window)
            .w(px(92.))
            .h(px(36.))
            .px(px(12.))
            .gap(px(8.))
            .justify_start()
            .bg(rgb(palette::CHIP))
            .on_click(cx.listener(move |sheet, _, _, cx| {
                sheet.keyboard = false;
                sheet.copy(index, cx);
            }))
            .child(
                div()
                    .relative()
                    .size(px(16.))
                    .flex_none()
                    .child(
                        chrome::icon("copy", palette::TEXT_2)
                            .opacity(1. - self.copy_motion[index].value()),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .opacity(self.copy_motion[index].value())
                            .child(chrome::icon("check", 0x4ade80)),
                    ),
            )
            .child(if copied { "Copied" } else { "Copy" });
        div().child(label(title)).child(
            div()
                .flex()
                .gap(px(8.))
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h(px(36.))
                        .px(px(11.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(palette::LINE_2))
                        .bg(rgb(palette::WINDOW))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .font_family("JetBrains Mono")
                        .text_size(px(12.))
                        .text_color(rgb(palette::TEXT_2))
                        .child(crate::sidebar::one_line(div()).child(line)),
                )
                .child(button),
        )
    }

    fn error(&self) -> Option<Div> {
        self.error.map(|error| {
            div()
                .debug_selector(|| "invite-error".to_owned())
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(rgb(0xf87171))
                .child(error)
        })
    }

    fn footer(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let ready = self.invitation.is_some();
        let text = if ready {
            "Done"
        } else if self.pending {
            "Creating..."
        } else if self.error.is_some() && !self.input.read(cx).invalid {
            "Try again"
        } else {
            "Create invite"
        };
        div()
            .mt(px(20.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .text_color(rgb(palette::MUTED))
                    .when_some(self.invitation.as_ref(), |note, invitation| {
                        let hours = invitation.expires_in_secs / 3600;
                        let expires = if hours >= 24 && hours.is_multiple_of(24) {
                            format!("{} day{}", hours / 24, if hours == 24 { "" } else { "s" })
                        } else {
                            format!("{hours} hour{}", if hours == 1 { "" } else { "s" })
                        };
                        note.child(format!("Works once. Expires in {expires}."))
                    }),
            )
            .child(
                self.button("invite-primary", &self.primary_focus, window)
                    .bg(rgb(palette::TEXT))
                    .text_color(rgb(palette::ON_ACCENT))
                    .when(self.pending, |button| button.opacity(0.55).cursor_default())
                    .on_click(cx.listener(|sheet, _, window, cx| {
                        sheet.keyboard = false;
                        if sheet.invitation.is_some() {
                            sheet.close(false, cx);
                        } else {
                            sheet.create(window, cx);
                        }
                    }))
                    .child(text),
            )
    }
}

fn label(text: &'static str) -> Div {
    div()
        .mb(px(8.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette::MUTED))
        .child(text)
}

impl Render for InviteSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.tick(window, cx);
        let body = div()
            .mt(px(16.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .when(self.invitation.is_none(), |body| {
                body.child(self.picker(window, cx))
            })
            .when(self.invitation.is_some(), |body| {
                body.child(self.code(0, window, cx))
                    .child(self.code(1, window, cx))
            })
            .when(!self.input.read(cx).invalid, |body| {
                body.children(self.error())
            });
        div()
            .id("invite-overlay")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.))
            .bg(rgba(0x0000008c))
            .opacity(self.motion.value())
            .text_left()
            .font_family("Inter")
            .text_size(px(13.))
            .line_height(relative(16. / 13.))
            .text_color(rgb(palette::TEXT))
            .capture_key_down(
                cx.listener(|sheet, event: &KeyDownEvent, window, cx| sheet.key(event, window, cx)),
            )
            .capture_key_up(|event, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .id("invite-sheet")
                    .w(px(460.))
                    .max_w_full()
                    .p(px(20.))
                    .relative()
                    .rounded(px(14.))
                    .border_1()
                    .border_color(rgb(palette::LINE_2))
                    .bg(rgb(palette::RAISED))
                    .shadow(crate::menu::shadow(24., 64.))
                    .on_mouse_down_out(cx.listener(|sheet, _, _, cx| {
                        sheet.close(false, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .pr(px(36.))
                            .text_size(px(15.))
                            .line_height(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Invite a person"),
                    )
                    .child(
                        self.button("invite-close", &self.close_focus, window)
                            .absolute()
                            .top(px(14.))
                            .right(px(14.))
                            .w(px(32.))
                            .px(px(0.))
                            .on_click(cx.listener(|sheet, _, _, cx| {
                                sheet.keyboard = false;
                                sheet.close(false, cx);
                            }))
                            .child(chrome::icon("close", palette::FAINT)),
                    )
                    .child(body)
                    .child(self.footer(window, cx)),
            )
    }
}
