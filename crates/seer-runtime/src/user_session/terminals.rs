use super::*;

impl UserSession {
    // Only the owner's own windows size a PTY. A read-only watcher crops
    // the frame on its side, so one small window never shrinks the terminal
    // for everyone (issue 371).
    pub(crate) fn apply_claimed_sizes(
        &mut self,
        sizes: &BTreeMap<String, PaneSize>,
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
            let claimed = sizes.get(&pane.id).copied();
            if let Some(size) = claimed {
                host.remember_owner_size(size);
            }
            let size = claimed.unwrap_or(host.owner_size);
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
        sizes: &BTreeMap<String, PaneSize>,
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
        let applied = self.apply_claimed_sizes(sizes)?;
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
