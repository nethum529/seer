use super::*;

impl SeerWindow {
    pub(in crate::window) fn start_room(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.room.pending || self.room.modal() {
            return;
        }
        let Content::Start(start) = &self.content else {
            return;
        };
        let focus = start.host.clone();
        self.room.error = None;
        self.confirm_room_action(
            RoomAction::Start,
            focus,
            self.keys.mode == Mode::Keyboard,
            window,
            cx,
        );
    }

    pub(in crate::window) fn removed_from_room(&mut self, cx: &mut Context<Self>) {
        self.room.lost = false;
        self.room.pending = false;
        self.room.refresh = None;
        self.room.action = None;
        self.room.menu = None;
        self.room.sheet = None;
        self.room.error = None;
        self.people_menu = None;
        self.menu = None;
        self.invite = None;
        self.input.reset();
        self.content = Content::Message(REMOVED.into());
        self._messages = None;
        if let Some(room) = &mut self.room.current {
            room.state = RoomState::Unreachable;
            room.people.clear();
        }
        self.notify_room(cx);
    }

    pub(in crate::window) fn room_command(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.room.pending || self.room.modal() {
            return;
        }
        let pick = match action {
            Action::StartRoom => Some(MenuAction::Room(RoomAction::Start)),
            Action::Invite => Some(MenuAction::Invite),
            Action::RoomMenu => None,
            _ => return,
        };
        if pick.is_some_and(|action| !self.room_actions().contains(&action)) {
            return;
        }
        self.finish_name(true, false, window, cx);
        self.open_room(true, window, cx);
        if let Some(action) = pick {
            self.pick_room_action(action, true, window, cx);
        }
    }

    pub(crate) fn room_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.room.modal() {
            self.sheet_key(key, window, cx);
            return true;
        }
        if self.room.sheet.is_some() {
            return true;
        }
        if self.room.open() {
            self.menu_key(key, window, cx);
            return true;
        }
        if self.room.focus.is_focused(window)
            && matches!(key.key.as_str(), "enter" | "space" | "up" | "down")
        {
            self.open_room(true, window, cx);
            return true;
        }
        false
    }

    fn menu_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        self.keys.mode = Mode::Keyboard;
        let actions = self.room_actions();
        let Some(menu) = &mut self.room.menu else {
            return;
        };
        let selected = menu
            .selected
            .unwrap_or(0)
            .min(actions.len().saturating_sub(1));
        match key.key.as_str() {
            "escape" | "tab" => self.close_room_menu(true, true, window, cx),
            "enter" | "space" => {
                if let Some(action) = actions.get(selected) {
                    self.pick_room_action(*action, true, window, cx);
                }
            }
            "up" | "down" | "home" | "end" => {
                menu.selected = sidebar::step(&key.key, actions.len(), menu.selected);
            }
            _ => {}
        }
        self.notify_room(cx);
    }

    fn sheet_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        self.keys.mode = Mode::Keyboard;
        let Some(sheet) = &self.room.sheet else {
            return;
        };
        match key.key.as_str() {
            "escape" => self.close_room_sheet(true, window, cx),
            "tab" => {
                window.focus(if sheet.cancel.is_focused(window) {
                    &sheet.confirm
                } else {
                    &sheet.cancel
                });
            }
            "enter" | "space" if !self.room.pending => {
                if sheet.confirm.is_focused(window) {
                    self.run_confirmation(window, cx);
                } else {
                    self.close_room_sheet(true, window, cx);
                }
            }
            _ => {}
        }
        self.notify_room(cx);
    }
}
