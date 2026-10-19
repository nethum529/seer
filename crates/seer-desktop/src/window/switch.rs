use super::*;

impl SeerWindow {
    pub(crate) fn switch_terminal_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.editing.is_some()
            || self.menu.is_some()
            || self.people_menu.is_some()
            || self.room.modal()
        {
            return false;
        }
        let Some(action) = shortcuts::action(Scope::Window, key, cx) else {
            return false;
        };
        let Content::Terminal(link, _) = &self.content else {
            return false;
        };
        let terminals = link.terminals();
        let now = terminals
            .iter()
            .position(|terminal| Some(terminal.pane.as_str()) == link.shown());
        let index = match action {
            Action::NextTerminal => sidebar::step("right", terminals.len(), now),
            Action::PreviousTerminal => sidebar::step("left", terminals.len(), now),
            Action::Terminal(number) => number.checked_sub(1).map(usize::from),
            _ => return false,
        };
        if let Some((index, terminal)) =
            index.and_then(|index| Some((index, terminals.get(index)?)))
        {
            let pane = terminal.pane.clone();
            self.keys.reveal(index);
            self.keys.mode = Mode::Keyboard;
            if link.shown() != Some(pane.as_str()) {
                self.select(&pane, cx);
            }
            window.focus(&self.input.focus);
            cx.notify();
        }
        true
    }
}
