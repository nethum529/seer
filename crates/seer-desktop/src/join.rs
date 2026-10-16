use gpui::prelude::*;
use gpui::{
    Context, Entity, EventEmitter, FocusHandle, FontWeight, IntoElement, KeyDownEvent, Render,
    StatefulInteractiveElement, Subscription, Task, Window, div, px, rgb,
};
use seer::{Invitation, JoinError, ServerEntry};

use crate::{
    chrome,
    name_input::{Finished, NameInput},
    palette,
};

pub(crate) struct Joined(pub(crate) ServerEntry);

pub(crate) struct Join {
    field: Entity<NameInput>,
    invitation: Option<Invitation>,
    error: Option<JoinError>,
    submit: FocusHandle,
    back: FocusHandle,
    busy: bool,
    _field: Subscription,
    task: Option<Task<()>>,
}

impl EventEmitter<Joined> for Join {}

impl Join {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| NameInput::form(String::new(), true, "seer join SEER2-...", cx));
        window.focus(&field.read(cx).focus);
        let subscription = cx.subscribe_in(&field, window, Self::finished);
        Self {
            field,
            invitation: None,
            error: None,
            submit: cx.focus_handle().tab_stop(true),
            back: cx.focus_handle().tab_stop(true),
            busy: false,
            _field: subscription,
            task: None,
        }
    }

    pub(crate) fn open_link(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.accept(Invitation::parse(line), window, cx);
    }

    pub(crate) fn joining(&self) -> bool {
        self.busy
    }

    fn field(&mut self, text: String, code: bool, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = if code {
            "seer join SEER2-..."
        } else {
            "Your name"
        };
        self.field = cx.new(|cx| NameInput::form(text, code, placeholder, cx));
        self._field = cx.subscribe_in(&self.field, window, Self::finished);
        window.focus(&self.field.read(cx).focus);
    }

    fn finished(
        &mut self,
        _: &Entity<NameInput>,
        event: &Finished,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            Finished::Save => self.submit(window, cx),
            Finished::Cancel => self.back(window, cx),
            Finished::Changed => {
                self.error = None;
                cx.notify();
            }
            Finished::SaveOutside => {}
        }
    }

    fn accept(
        &mut self,
        invite: Result<Invitation, JoinError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match invite {
            Ok(invite) => {
                self.invitation = Some(invite);
                self.error = None;
                self.field(
                    seer::system_user_name().unwrap_or_default(),
                    false,
                    window,
                    cx,
                );
            }
            Err(error) => {
                self.invitation = None;
                self.error = Some(error);
                self.field(String::new(), true, window, cx);
            }
        }
        cx.notify();
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.invitation.is_none() {
            return;
        }
        self.invitation = None;
        self.error = None;
        self.field(String::new(), true, window, cx);
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let text = self.field.read(cx).text.clone();
        let Some(invitation) = self.invitation.clone() else {
            match Invitation::parse(&text) {
                Ok(invite) => self.accept(Ok(invite), window, cx),
                Err(error) => {
                    self.error = Some(error);
                    cx.notify();
                }
            }
            return;
        };
        self.busy = true;
        self.error = None;
        let join = cx
            .background_executor()
            .spawn(async move { invitation.join(&text) });
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let result = join.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok(server) => {
                        view.invitation = None;
                        view.field(String::new(), false, window, cx);
                        cx.emit(Joined(server));
                    }
                    Err(error) => {
                        if matches!(error, JoinError::InvalidSeat) {
                            view.invitation = None;
                            view.field(String::new(), true, window, cx);
                        }
                        view.error = Some(error);
                        window.focus(&view.field.read(cx).focus);
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl Render for Join {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.invitation.is_some();
        let busy = self.busy;
        self.field.update(cx, |input, cx| {
            if input.invalid != self.error.is_some() || input.disabled != busy {
                input.invalid = self.error.is_some();
                input.disabled = busy;
                cx.notify();
            }
        });
        let label = if busy {
            "Joining..."
        } else if matches!(self.error, Some(JoinError::Unreachable)) {
            "Try again"
        } else if name {
            "Join room"
        } else {
            "Continue"
        };
        let actions = div()
            .flex()
            .justify_end()
            .gap(px(8.))
            .mt(px(20.))
            .when(name, |row| {
                row.child(
                    button("join-back", "Back", &self.back, false, busy, window)
                        .on_click(cx.listener(|view, _, window, cx| view.back(window, cx)))
                        .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                                view.back(window, cx);
                                cx.stop_propagation();
                            }
                        })),
                )
            })
            .child(
                button("join-submit", label, &self.submit, true, busy, window)
                    .when(busy, |button| button.opacity(0.6))
                    .on_click(cx.listener(|view, _, window, cx| view.submit(window, cx)))
                    .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                            view.submit(window, cx);
                            cx.stop_propagation();
                        }
                    })),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(chrome::bar(cx).pl(px(20.)).child(chrome::lights()))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px(px(24.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(360.))
                            .max_w_full()
                            .mt(px((f32::from(window.viewport_size().height) - 44.) * 0.32))
                            .child(
                                div()
                                    .mb(px(24.))
                                    .font_family("Inter Display")
                                    .text_size(px(22.))
                                    .line_height(px(28.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::label::text(if name {
                                        "Choose your name"
                                    } else {
                                        "Join a room"
                                    })),
                            )
                            .when(!name, |form| {
                                form.child(
                                    div()
                                        .mb(px(8.))
                                        .text_size(px(12.))
                                        .line_height(px(16.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(palette::MUTED))
                                        .child(crate::label::text("Invite from the host")),
                                )
                            })
                            .child(self.field.clone())
                            .children(self.error.as_ref().map(|error| {
                                div()
                                    .id("join-error")
                                    .debug_selector(|| "join-error".into())
                                    .mt(px(8.))
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(rgb(0xf87171))
                                    .child(crate::label::text(error_text(error, window)))
                            }))
                            .child(actions),
                    ),
            )
    }
}

fn error_text(error: &JoinError, window: &Window) -> String {
    let mut text = error.to_string();
    let run = gpui::TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: rgb(0xf87171).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(text.clone().into(), px(12.), &[run], None);
    let width = (window.viewport_size().width - px(48.)).min(px(360.));
    if line.width <= width {
        return text;
    }
    let midpoint = f32::from(line.width) / 2.;
    let distance = |at| (f32::from(line.x_for_index(at)) - midpoint).abs();
    let at = text
        .char_indices()
        .filter(|(_, ch)| *ch == ' ')
        .map(|(at, _)| at)
        .min_by(|left, right| distance(*left).total_cmp(&distance(*right)));
    if let Some(at) = at {
        text.replace_range(at..at + 1, "\n");
    }
    text
}

fn button(
    id: &'static str,
    label: &'static str,
    focus: &FocusHandle,
    primary: bool,
    disabled: bool,
    window: &Window,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(|| id.into())
        .track_focus(focus)
        .h(px(32.))
        .px(px(14.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .font_weight(FontWeight::MEDIUM)
        .bg(rgb(if primary {
            palette::ACCENT
        } else {
            palette::CHIP
        }))
        .text_color(rgb(if primary {
            palette::ON_ACCENT
        } else {
            palette::TEXT
        }))
        .when(!disabled, |button| {
            button
                .hover(|button| button.opacity(0.92))
                .active(|button| button.opacity(0.84))
        })
        .border_1()
        .border_color(if focus.is_focused(window) {
            rgb(palette::FAINT).into()
        } else {
            gpui::transparent_black()
        })
        .child(crate::label::text(label))
}
