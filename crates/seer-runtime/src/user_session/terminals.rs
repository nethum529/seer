use super::*;

impl UserSession {
    pub(super) fn terminals(&self) -> Vec<TerminalInfo> {
        self.tree
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .flat_map(|tab| &tab.panes)
            .map(|pane| {
                let foreground = self
                    .pane_hosts
                    .get(&pane.id)
                    .map(PaneHost::foreground)
                    .unwrap_or_default();
                let shell = foreground.is_empty()
                    || matches!(
                        foreground.as_str(),
                        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "nu"
                    )
                    || std::path::Path::new(&self.shell)
                        .file_name()
                        .and_then(|name| name.to_str())
                        == Some(foreground.as_str());
                TerminalInfo {
                    last_typist: self
                        .pane_hosts
                        .get(&pane.id)
                        .and_then(PaneHost::last_typist),
                    pane: pane.id.clone(),
                    name: if shell { "shell".into() } else { foreground },
                    state: if shell { "idle" } else { "busy" }.into(),
                    cols: pane.size.cols,
                    rows: pane.size.rows,
                }
            })
            .collect()
    }

    pub(super) fn granted_input(
        &mut self,
        workspace: &str,
        tab: &str,
        pane: &str,
        bytes: &[u8],
        sender: String,
    ) -> io::Result<Vec<ServerMsg>> {
        self.validate_pane(workspace, tab, pane)?;
        self.pane_hosts
            .get_mut(pane)
            .ok_or_else(|| pane_host_not_found(pane))?
            .write_granted(bytes, sender)?;
        Ok(vec![ServerMsg::Terminals {
            user: self.user.clone(),
            terminals: self.terminals(),
        }])
    }
}
