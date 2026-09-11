use super::*;

impl UserSession {
    pub(super) fn reap_shells(&mut self) -> Vec<ServerMsg> {
        let ended: Vec<_> = self
            .pane_hosts
            .iter_mut()
            .filter_map(|(pane, host)| match host.has_exited() {
                Ok(true) => Some(pane.clone()),
                Ok(false) => None,
                Err(error) => {
                    eprintln!("could not check shell: {error}");
                    None
                }
            })
            .collect();
        let mut messages = Vec::new();
        for pane in ended {
            let location = self.tree.workspaces.iter().find_map(|workspace| {
                workspace.tabs.iter().find_map(|tab| {
                    tab.panes
                        .iter()
                        .any(|candidate| candidate.id == pane)
                        .then(|| (workspace.id.clone(), tab.id.clone()))
                })
            });
            self.pane_hosts.remove(&pane);
            if let Some((workspace, tab)) = location {
                match self.remove_pane(&workspace, &tab, &pane) {
                    Ok(changed) => messages.extend(changed),
                    Err(error) => eprintln!("could not remove ended shell: {error}"),
                }
            }
        }
        messages
    }

    pub(super) fn remove_pane(
        &mut self,
        workspace: &str,
        tab: &str,
        pane: &str,
    ) -> io::Result<Vec<ServerMsg>> {
        let closed_tab = self.tree.close_pane(pane).map_err(tree_error)?;
        if closed_tab.panes.is_empty() {
            self.tree.close_tab(workspace, tab).map_err(tree_error)?;
        } else {
            self.resize_tab(workspace, tab)?;
        }
        persistence::persist(self)?;
        Ok(self.tree_message())
    }
}
