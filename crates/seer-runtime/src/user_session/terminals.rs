use super::*;

pub(crate) struct SizeClaims {
    pub(crate) own: BTreeMap<String, PaneSize>,
    pub(crate) watched: BTreeMap<String, PaneSize>,
}

impl UserSession {
    pub(super) fn rename_tab(
        &mut self,
        workspace: &str,
        tab: &str,
        title: String,
    ) -> io::Result<Vec<ServerMsg>> {
        if title.chars().count() > 40 || title.chars().any(char::is_control) {
            return Err(invalid_input(
                "terminal names must have at most 40 printable characters".into(),
            ));
        }
        self.tab_mut(workspace, tab)?.title = title;
        persistence::persist(self)?;
        let mut messages = self.tree_message();
        let terminals = self.terminals();
        self.published_terminals.clone_from(&terminals);
        messages.push(ServerMsg::Terminals {
            user: self.user.clone(),
            terminals,
        });
        Ok(messages)
    }

    // A watcher never shrinks a PTY (issue 371) and gets a wrapped screen
    // (issue 406). A full screen app cannot be wrapped, so in the alt screen
    // the PTY grows on each axis to the largest full viewer watch and goes
    // back when the app leaves the alt screen or the last viewer stops
    // (issue 433). The owner then sees the top left part of the terminal.
    pub(crate) fn apply_claimed_sizes(
        &mut self,
        claims: &SizeClaims,
    ) -> io::Result<Vec<ServerMsg>> {
        let mut changed = false;
        for pane in self
            .tree
            .workspaces
            .iter_mut()
            .flat_map(|workspace| &mut workspace.tabs)
            .flat_map(|tab| &mut tab.panes)
        {
            let Some(host) = self.pane_hosts.get_mut(&pane.id) else {
                continue;
            };
            let claimed = claims.own.get(&pane.id).copied();
            if let Some(size) = claimed {
                host.remember_owner_size(size);
            }
            let own = claimed.unwrap_or(host.owner_size);
            let size = match claims.watched.get(&pane.id) {
                Some(watched) if host.alt_screen() => own.largest(*watched),
                _ => own,
            };
            if pane.size != size {
                seer_core::debug_log!(
                    "pty resize pane={} size={}x{} owner_size={}x{} claimed={}",
                    pane.id,
                    size.cols,
                    size.rows,
                    host.owner_size.cols,
                    host.owner_size.rows,
                    claimed.is_some()
                );
                host.resize_visible(size.cols, size.rows)?;
                pane.size = size;
                changed = true;
            }
        }
        Ok(if changed { self.snapshot() } else { Vec::new() })
    }

    // The server resizes to the selected size only, so a watched pane never shrinks to its tab rect first.
    pub(crate) fn record_viewport(
        &mut self,
        workspace: &str,
        tab: &str,
        cols: u16,
        rows: u16,
        claims: &SizeClaims,
    ) -> io::Result<Vec<ServerMsg>> {
        self.tab(workspace, tab)?;
        self.viewport = PaneSize { cols, rows };
        for (pane, size) in self.pane_rects(workspace, tab, cols, rows) {
            seer_core::debug_log!(
                "viewport tab={tab} size={cols}x{rows} pane={pane} owner_size={}x{}",
                size.cols,
                size.rows
            );
            if let Some(host) = self.pane_hosts.get_mut(&pane) {
                host.remember_owner_size(size);
            }
        }
        let applied = self.apply_claimed_sizes(claims)?;
        persistence::persist(self)?;
        Ok(if applied.is_empty() {
            self.snapshot()
        } else {
            applied
        })
    }

    pub(crate) fn pane_rects(
        &self,
        workspace: &str,
        tab: &str,
        cols: u16,
        rows: u16,
    ) -> Vec<(String, PaneSize)> {
        let Ok(tab) = self.tab(workspace, tab) else {
            return Vec::new();
        };
        rects(tab, cols, rows)
            .into_iter()
            .map(|pane_rect| {
                (
                    pane_rect.pane,
                    PaneSize {
                        cols: pane_rect.cols,
                        rows: pane_rect.rows,
                    },
                )
            })
            .collect()
    }

    pub(super) fn terminals(&self) -> Vec<TerminalInfo> {
        self.tree
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .flat_map(|tab| tab.panes.iter().map(move |pane| (tab, pane)))
            .map(|(tab, pane)| {
                let host = self.pane_hosts.get(&pane.id);
                let observation = host.map(PaneHost::observation).unwrap_or_default();
                let foreground = observation.name;
                let shell = observation.agent_state.is_none()
                    && (crate::agent_state::is_shell(&foreground)
                        || host.map(PaneHost::started_program) == Some(foreground.as_str()));
                TerminalInfo {
                    agent_state: observation.agent_state,
                    agent_state_since: observation.agent_state_since,
                    title: Some(tab.title.clone()),
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

    pub(super) fn granted_terminal_input(
        &mut self,
        workspace: &str,
        tab: &str,
        pane: &str,
        input: &TerminalInput,
        sender: String,
    ) -> io::Result<Vec<ServerMsg>> {
        self.validate_pane(workspace, tab, pane)?;
        self.pane_hosts
            .get_mut(pane)
            .ok_or_else(|| pane_host_not_found(pane))?
            .write_granted_terminal_input(input, sender)?;
        Ok(vec![ServerMsg::Terminals {
            user: self.user.clone(),
            terminals: self.terminals(),
        }])
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
