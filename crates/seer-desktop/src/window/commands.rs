use super::*;

impl SeerWindow {
    pub(crate) fn window_command_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match shortcuts::action(Scope::Window, key, cx) {
            Some(Action::Settings) => self.open_settings(true, window, cx),
            Some(Action::PersonMenu) => {
                if matches!(self.content, Content::Terminal(..)) && !self.room.modal() {
                    self.close_room_menu(true, false, window, cx);
                    self.open_people(true, window, cx);
                }
            }
            Some(action @ (Action::StartRoom | Action::Invite | Action::RoomMenu)) => {
                self.room_command(action, window, cx);
            }
            _ => return false,
        }
        true
    }
}
