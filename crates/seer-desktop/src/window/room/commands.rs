use super::*;

impl SeerWindow {
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
        if pick.is_some_and(|action| !self.room.actions().contains(&action)) {
            return;
        }
        self.finish_name(true, false, window, cx);
        self.open_room(true, window, cx);
        if let Some(action) = pick {
            self.pick_room_action(action, true, window, cx);
        }
    }
}
