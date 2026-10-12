use gpui::{AnyElement, StatefulInteractiveElement};

use super::paint::action_label;
use super::*;
use crate::menu;

impl SeerWindow {
    pub(super) fn room_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sheet = self.room.sheet.as_ref()?;
        let room = &sheet.room;
        let (title, body) = confirm_copy(sheet.action, room);
        let width = 400_f32.min(f32::from(window.viewport_size().width) - 32.);
        let action = sheet.action;
        let card = div()
            .id("room-confirm")
            .debug_selector(|| "room-confirm".into())
            .relative()
            .w(px(width))
            .p(px(20.))
            .rounded(px(14.))
            .border_1()
            .border_color(rgb(palette::LINE_2))
            .bg(rgb(palette::RAISED))
            .shadow(menu::shadow(24., 64.))
            .text_size(px(13.))
            .line_height(px(18.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(15.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(div().mt(px(8.)).text_color(rgb(palette::MUTED)).child(body))
            .children(self.room.error.as_ref().map(|error| {
                div()
                    .mt(px(8.))
                    .text_size(px(12.))
                    .text_color(rgb(palette::DANGER))
                    .child(error.clone())
            }))
            .child(
                div()
                    .mt(px(20.))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(
                            "room-cancel",
                            "Cancel",
                            &sheet.cancel,
                            false,
                            self.keys.mode,
                            window,
                        )
                        .on_click(cx.listener(
                            |view, event, window, cx| {
                                if matches!(event, gpui::ClickEvent::Mouse(_)) {
                                    view.close_room_sheet(false, window, cx);
                                }
                            },
                        )),
                    )
                    .child(
                        button(
                            "room-confirm-action",
                            if self.room.pending {
                                "Working..."
                            } else {
                                action_label(action)
                            },
                            &sheet.confirm,
                            true,
                            self.keys.mode,
                            window,
                        )
                        .when(self.room.pending, |button| button.opacity(0.6))
                        .on_click(cx.listener(
                            move |view, event, window, cx| {
                                if matches!(event, gpui::ClickEvent::Mouse(_)) && view.room.modal()
                                {
                                    view.run_room_action(action, window, cx);
                                }
                            },
                        )),
                    ),
            );
        Some(
            div()
                .id("room-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p(px(16.))
                .bg(gpui::rgba(palette::SCRIM))
                .opacity(sheet.motion.value())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, window, cx| {
                        view.close_room_sheet(false, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(card)
                .into_any_element(),
        )
    }
}

fn button(
    id: &'static str,
    text: &'static str,
    focus: &FocusHandle,
    destructive: bool,
    mode: Mode,
    window: &Window,
) -> Stateful<Div> {
    let ring = mode == Mode::Keyboard && focus.is_focused(window);
    div()
        .id(id)
        .debug_selector(move || id.into())
        .relative()
        .track_focus(focus)
        .h(px(32.))
        .px(px(14.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .font_weight(FontWeight::MEDIUM)
        .bg(rgb(if destructive {
            palette::DANGER
        } else {
            palette::CHIP
        }))
        .text_color(rgb(if destructive {
            palette::ON_ACCENT
        } else {
            palette::TEXT
        }))
        .when(ring, |button| {
            button.child(sidebar::outline().when(destructive, |outline| {
                outline
                    .top(px(-3.))
                    .bottom(px(-3.))
                    .left(px(-3.))
                    .right(px(-3.))
                    .rounded(px(11.))
            }))
        })
        .hover(|button| button.opacity(0.92))
        .active(|button| button.opacity(0.84))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(text)
}

fn confirm_copy(action: RoomAction, room: &RoomSummary) -> (String, String) {
    let name = chrome::capitalize(room.room_name());
    let (people, lose, are) = match room.people.len() {
        0 => ("other people".to_owned(), "lose", "are"),
        2 => ("1 person".to_owned(), "loses", "is"),
        count => (format!("{} people", count.saturating_sub(1)), "lose", "are"),
    };
    match action {
        RoomAction::Stop => (
            format!("Stop {name}?"),
            format!(
                "{} {lose} the room. Their own terminals keep running. Your terminals on this computer stop too.",
                chrome::capitalize(&people)
            ),
        ),
        RoomAction::Leave => (
            format!("Leave {name}?"),
            "Your terminals on this computer stop. The room stays open for the others.".into(),
        ),
        RoomAction::StartNew | RoomAction::Restore => (
            "Start a new room?".into(),
            format!("The {people} of {name} {are} removed."),
        ),
    }
}
