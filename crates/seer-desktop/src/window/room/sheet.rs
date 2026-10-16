use gpui::{AnyElement, StatefulInteractiveElement};

use super::paint::action_label;
use super::*;
use crate::menu;

impl SeerWindow {
    pub(super) fn room_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sheet = self.room.sheet.as_ref()?;
        let (title, body) = match &sheet.action {
            Confirmation::Room(action, room) => confirm_copy(*action, room),
            Confirmation::Terminal { program, .. } => (
                "Close terminal?".into(),
                format!(
                    "Closing this terminal stops {}.",
                    chrome::capitalize(program)
                ),
            ),
        };
        let width = 400_f32.min(f32::from(window.viewport_size().width) - 32.);
        let label = match &sheet.action {
            Confirmation::Room(action, _) => action_label(*action),
            Confirmation::Terminal { .. } => "Close terminal",
        };
        let card = menu::sheet("room-confirm", width, cx)
            .p(px(20.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(15.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(crate::label::text(title)),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_color(rgb(palette::MUTED))
                    .child(crate::label::text(body)),
            )
            .children(self.room.error.as_ref().map(|error| {
                div()
                    .mt(px(8.))
                    .text_size(px(12.))
                    .text_color(rgb(palette::DANGER))
                    .child(crate::label::text(error.clone()))
            }))
            .child(
                div()
                    .mt(px(20.))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        menu::button(
                            "room-cancel",
                            "Cancel",
                            &sheet.cancel,
                            None,
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
                        menu::button(
                            "room-confirm-action",
                            if self.room.pending {
                                "Working..."
                            } else {
                                label
                            },
                            &sheet.confirm,
                            Some(palette::DANGER),
                            self.keys.mode,
                            window,
                        )
                        .when(self.room.pending, |button| button.opacity(0.6))
                        .on_click(cx.listener(
                            |view, event, window, cx| {
                                if matches!(event, gpui::ClickEvent::Mouse(_)) && view.room.modal()
                                {
                                    view.run_confirmation(window, cx);
                                }
                            },
                        )),
                    ),
            );
        Some(
            menu::scrim("room-scrim")
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

    pub(in crate::window) fn confirm_terminal(
        &mut self,
        pane: &str,
        program: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cancel = cx.focus_handle();
        let confirm = cx.focus_handle();
        window.focus(&cancel);
        self.room.error = None;
        self.room.sheet = Some(Sheet {
            action: Confirmation::Terminal {
                pane: pane.into(),
                program: program.into(),
            },
            return_focus: self.keys.tabs.clone(),
            cancel,
            confirm,
            motion: Motion::open(self.keys.mode == Mode::Keyboard, 200),
        });
    }

    pub(super) fn run_confirmation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = &self.room.sheet else {
            return;
        };
        match sheet.action.clone() {
            Confirmation::Room(action, _) => self.run_room_action(action, window, cx),
            Confirmation::Terminal { pane, .. } => {
                self.send_close(&pane, cx);
                self.close_room_sheet(self.keys.mode == Mode::Keyboard, window, cx);
            }
        }
    }
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
