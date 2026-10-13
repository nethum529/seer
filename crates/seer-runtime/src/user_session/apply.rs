use super::*;

impl UserSession {
    pub fn apply(&mut self, msg: ClientMsg) -> io::Result<Vec<ServerMsg>> {
        match msg {
            ClientMsg::CreateTab { workspace } => self.create_tab(&workspace),
            ClientMsg::RenameTab {
                workspace,
                tab,
                title,
            } => self.rename_tab(&workspace, &tab, title),
            ClientMsg::SplitPane {
                workspace,
                tab,
                direction,
            } => self.split_pane(&workspace, &tab, direction),
            ClientMsg::ClosePane {
                workspace,
                tab,
                pane,
            } => self.close_pane(&workspace, &tab, &pane),
            ClientMsg::FocusPane {
                workspace,
                tab,
                pane,
            } => self.focus_pane(&workspace, &tab, &pane),
            ClientMsg::TerminalCapabilities { capabilities } => {
                validate_capabilities(capabilities).map(|()| Vec::new())
            }
            ClientMsg::GrantedMouse {
                workspace,
                tab,
                pane,
                mouse,
                sender,
            } => self.granted_terminal_input(
                &workspace,
                &tab,
                &pane,
                &TerminalInput::new(seer_core::InputEvent::Mouse(mouse)),
                sender,
            ),
            ClientMsg::GrantedInput {
                workspace,
                tab,
                pane,
                bytes,
                sender,
            } => self.granted_input(&workspace, &tab, &pane, &bytes, sender),
            ClientMsg::GrantedTerminalInput {
                workspace,
                tab,
                pane,
                input,
                sender,
            } => self.granted_terminal_input(&workspace, &tab, &pane, &input, sender),
            ClientMsg::TerminalInput {
                workspace,
                tab,
                pane,
                input,
            } => self.terminal_input(&workspace, &tab, &pane, &input),
            ClientMsg::Resize {
                workspace,
                tab,
                cols,
                rows,
            } => self.resize(&workspace, &tab, cols, rows),
            ClientMsg::Hello { .. }
            | ClientMsg::Join { .. }
            | ClientMsg::Invite { .. }
            | ClientMsg::ListPeople
            | ClientMsg::QueryStatus
            | ClientMsg::DetachClient { .. }
            | ClientMsg::ExitClient { .. }
            | ClientMsg::AttachRuntime
            | ClientMsg::ObserveRuntime
            | ClientMsg::PublishRuntime { .. }
            | ClientMsg::RuntimeStream { .. }
            | ClientMsg::QueryTargets { .. }
            | ClientMsg::Watch { .. }
            | ClientMsg::Unwatch { .. }
            | ClientMsg::Resync { .. }
            | ClientMsg::Parsed { .. }
            | ClientMsg::Terminals { .. }
            | ClientMsg::SetAllGrants { .. }
            | ClientMsg::SetGrant { .. }
            | ClientMsg::MouseInto { .. }
            | ClientMsg::TypeInto { .. }
            | ClientMsg::InputInto { .. }
            | ClientMsg::Stop
            | ClientMsg::Leave
            | ClientMsg::Detach => Ok(Vec::new()),
        }
    }
}
