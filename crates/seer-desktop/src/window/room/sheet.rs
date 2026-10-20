use gpui::{AnyElement, StatefulInteractiveElement};

use super::paint::action_label;
use super::*;
use crate::menu;

impl SeerWindow {
    pub(super) fn room_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sheet = self.room.sheet.as_ref()?;
        let (title, body) = match &sheet.action {
            Confirmation::Room(action, room) => confirm_copy(*action, room.as_deref()),
            Confirmation::Remove { name } => {
                let name = chrome::capitalize(name);
                (
                    format!("Remove {name}?"),
                    format!("{name} leaves the room. Their shells keep running on their computer."),
                )
            }
            Confirmation::ShareAll => (
                "Share control with everyone?".into(),
                "Every person in this room can type into your terminals.".into(),
            ),
            Confirmation::Terminal { program, .. } => (
                "Close terminal?".into(),
                format!(
                    "Closing this terminal stops {}.",
                    chrome::capitalize(program)
                ),
            ),
        };
        let label = match &sheet.action {
            Confirmation::Room(action, _) => action_label(*action),
            Confirmation::Terminal { .. } => "Close terminal",
            Confirmation::Remove { .. } => "Remove",
            Confirmation::ShareAll => "Share control",
        };
        let cancel = menu::button(
            "room-cancel",
            "Cancel",
            &sheet.cancel,
            None,
            self.keys.mode,
            window,
        )
        .on_click(cx.listener(|view, event, window, cx| {
            if matches!(event, gpui::ClickEvent::Mouse(_)) {
                view.close_room_sheet(false, window, cx);
            }
        }));
        let confirm = menu::button(
            "room-confirm-action",
            if self.room.pending {
                "Working..."
            } else {
                label
            },
            &sheet.confirm,
            Some(
                if matches!(
                    sheet.action,
                    Confirmation::Room(RoomAction::Start, _) | Confirmation::ShareAll
                ) {
                    palette::ACCENT
                } else {
                    palette::DANGER
                },
            ),
            self.keys.mode,
            window,
        )
        .when(self.room.pending, |button| button.opacity(0.6))
        .on_click(cx.listener(|view, event, window, cx| {
            if matches!(event, gpui::ClickEvent::Mouse(_)) && view.room.modal() {
                view.run_confirmation(window, cx);
            }
        }));
        let card = menu::confirm::card(
            menu::confirm::Copy {
                id: "room-confirm",
                title: &title,
                body: &body,
                error: self.room.error.as_deref(),
            },
            cancel,
            confirm,
            window,
            cx,
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

    pub(in crate::window) fn confirm_remove(
        &mut self,
        user: &str,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Content::Terminal(link, _) = &self.content else {
            return;
        };
        if self.room.pending
            || self.room.modal()
            || self.room.server.is_none()
            || !link.people.can_remove(user)
        {
            return;
        }
        let Some(person) = link
            .people
            .people
            .iter()
            .find(|person| person.user_id == user)
        else {
            return;
        };
        let name = person.name.clone();
        let cancel = cx.focus_handle();
        let confirm = cx.focus_handle();
        self.people_menu = None;
        self.keys.mode = if keyboard {
            Mode::Keyboard
        } else {
            Mode::Pointer
        };
        self.room.error = None;
        self.room.sheet = Some(Sheet {
            action: Confirmation::Remove { name },
            return_focus: self.keys.who.clone(),
            cancel: cancel.clone(),
            confirm,
            motion: Motion::open(keyboard, 200),
        });
        window.focus(&cancel);
        self.notify_room(cx);
    }

    fn run_remove(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.room.pending {
            return;
        }
        let Some(server) = self.room.server.clone() else {
            return;
        };
        self.room.pending = true;
        self.room.error = None;
        if let Some(sheet) = &self.room.sheet {
            window.focus(&sheet.confirm);
        }
        let work = cx
            .background_executor()
            .spawn(async move { seer::rooms::remove_person(&server, &name) });
        self.room.action = Some(cx.spawn_in(window, async move |view, cx| {
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.room.pending = false;
                match result {
                    Ok(()) => view.close_room_sheet(view.keys.mode == Mode::Keyboard, window, cx),
                    Err(error) => view.room.error = Some(error.to_string()),
                }
                view.notify_room(cx);
            });
        }));
        self.notify_room(cx);
    }

    pub(super) fn run_confirmation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = &self.room.sheet else {
            return;
        };
        match sheet.action.clone() {
            Confirmation::Room(action, _) => self.run_room_action(action, window, cx),
            Confirmation::Remove { name } => self.run_remove(name, window, cx),
            Confirmation::ShareAll => self.run_all_grants(true, window, cx),
            Confirmation::Terminal { pane, .. } => {
                self.send_close(&pane, cx);
                self.close_room_sheet(self.keys.mode == Mode::Keyboard, window, cx);
            }
        }
    }
}

fn confirm_copy(action: RoomAction, room: Option<&RoomSummary>) -> (String, String) {
    let Some(room) = room else {
        return (
            "Start a room?".into(),
            "You will host a room on this computer.".into(),
        );
    };
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
        RoomAction::Start | RoomAction::StartNew | RoomAction::Restore => (
            "Start a new room?".into(),
            format!("The {people} of {name} {are} removed."),
        ),
    }
}
