use crate::PaneHost;
use portable_pty::CommandBuilder;
use seer_core::layout::{PaneRect, rects};
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{PaneSize, Tab, Tree, TreeError};
use std::collections::BTreeMap;
use std::io;

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_WORKSPACE: &str = "main";
const DEFAULT_TAB_TITLE: &str = "shell";

pub struct UserSession {
    pub user: String,
    pub tree: Tree,
    shell: String,
    viewport: PaneSize,
    focused_tab: Option<String>,
    pane_hosts: BTreeMap<String, PaneHost>,
}

impl UserSession {
    #[must_use]
    pub fn new(user: impl Into<String>, shell: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            tree: Tree::new(),
            shell: shell.into(),
            viewport: PaneSize {
                cols: DEFAULT_COLS,
                rows: DEFAULT_ROWS,
            },
            focused_tab: None,
            pane_hosts: BTreeMap::new(),
        }
    }

    pub fn apply(&mut self, msg: ClientMsg) -> io::Result<Vec<ServerMsg>> {
        match msg {
            ClientMsg::CreateTab => self.create_tab(),
            ClientMsg::SplitPane { direction } => self.split_pane(direction),
            ClientMsg::ClosePane { pane } => self.close_pane(&pane),
            ClientMsg::FocusPane { pane } => self.focus_pane(&pane),
            ClientMsg::Input { pane, bytes } => self.write_input(&pane, &bytes),
            ClientMsg::Resize { cols, rows } => self.resize(cols, rows),
            ClientMsg::Hello { .. }
            | ClientMsg::Join { .. }
            | ClientMsg::Invite
            | ClientMsg::ListPeople
            | ClientMsg::DetachClient { .. }
            | ClientMsg::Peek { .. }
            | ClientMsg::StopPeek
            | ClientMsg::Detach => Ok(Vec::new()),
        }
    }

    #[must_use]
    pub fn poll(&mut self) -> Vec<ServerMsg> {
        self.pane_hosts
            .iter_mut()
            .filter_map(|(pane, host)| {
                (host.poll() > 0).then(|| ServerMsg::Cells {
                    pane: pane.clone(),
                    rows: host.cells(),
                })
            })
            .collect()
    }

    fn create_tab(&mut self) -> io::Result<Vec<ServerMsg>> {
        let workspace_id = self.ensure_workspace()?;
        let tab = self
            .tree
            .create_tab(&workspace_id, DEFAULT_TAB_TITLE, self.viewport)
            .map_err(tree_error)?;
        let pane = tab
            .layout
            .focused
            .as_deref()
            .ok_or_else(|| invalid_data("new tab has no focused pane"))?;
        let pane_rect = find_rect(&tab, self.viewport, pane)?;
        let host = self.start_host(&pane_rect)?;

        self.pane_hosts.insert(pane.to_owned(), host);
        self.focused_tab = Some(tab.id);
        Ok(self.tree_message())
    }

    fn ensure_workspace(&mut self) -> io::Result<String> {
        if self.tree.workspaces.is_empty() {
            self.tree
                .create_workspace(DEFAULT_WORKSPACE)
                .map_err(tree_error)?;
        }
        self.tree
            .workspaces
            .first()
            .map(|workspace| workspace.id.clone())
            .ok_or_else(|| invalid_data("session has no workspace"))
    }

    fn split_pane(&mut self, direction: seer_core::SplitDirection) -> io::Result<Vec<ServerMsg>> {
        let focused = self.focused_pane()?.to_owned();
        let tab = self
            .tree
            .split_pane(&focused, direction)
            .map_err(tree_error)?;
        let new_pane = tab
            .layout
            .focused
            .as_deref()
            .ok_or_else(|| invalid_data("split tab has no focused pane"))?;
        let pane_rect = find_rect(&tab, self.viewport, new_pane)?;
        let host = self.start_host(&pane_rect)?;
        self.pane_hosts.insert(new_pane.to_owned(), host);
        self.resize_tab(&tab.id)?;
        Ok(self.tree_message())
    }

    fn close_pane(&mut self, pane: &str) -> io::Result<Vec<ServerMsg>> {
        let tab_id = self.tab_id_for_pane(pane)?.to_owned();
        let mut host = self
            .pane_hosts
            .remove(pane)
            .ok_or_else(|| pane_host_not_found(pane))?;
        if let Err(error) = host.kill() {
            self.pane_hosts.insert(pane.to_owned(), host);
            return Err(error);
        }
        self.tree.close_pane(pane).map_err(tree_error)?;
        self.resize_tab(&tab_id)?;
        Ok(self.tree_message())
    }

    fn focus_pane(&mut self, pane: &str) -> io::Result<Vec<ServerMsg>> {
        let tab = self.tree.focus_pane(pane).map_err(tree_error)?;
        self.focused_tab = Some(tab.id);
        Ok(self.tree_message())
    }

    fn write_input(&mut self, pane: &str, bytes: &[u8]) -> io::Result<Vec<ServerMsg>> {
        self.pane_hosts
            .get_mut(pane)
            .ok_or_else(|| pane_host_not_found(pane))?
            .write_input(bytes)?;
        Ok(Vec::new())
    }

    fn resize(&mut self, cols: u16, rows: u16) -> io::Result<Vec<ServerMsg>> {
        self.viewport = PaneSize { cols, rows };
        if let Some(tab_id) = self.focused_tab.clone() {
            self.resize_tab(&tab_id)?;
        }
        Ok(self.tree_message())
    }

    fn resize_tab(&mut self, tab_id: &str) -> io::Result<()> {
        let pane_rects = {
            let tab = self.tab(tab_id)?;
            rects(tab, self.viewport.cols, self.viewport.rows)
        };

        for pane_rect in &pane_rects {
            self.pane_hosts
                .get_mut(&pane_rect.pane)
                .ok_or_else(|| pane_host_not_found(&pane_rect.pane))?
                .resize(pane_rect.cols, pane_rect.rows)?;
        }
        self.store_pane_sizes(tab_id, &pane_rects)
    }

    fn store_pane_sizes(&mut self, tab_id: &str, pane_rects: &[PaneRect]) -> io::Result<()> {
        let tab = self.tab_mut(tab_id)?;
        for pane in &mut tab.panes {
            let pane_rect = pane_rects
                .iter()
                .find(|pane_rect| pane_rect.pane == pane.id)
                .ok_or_else(|| invalid_data("pane is missing from its layout"))?;
            pane.size = PaneSize {
                cols: pane_rect.cols,
                rows: pane_rect.rows,
            };
        }
        Ok(())
    }

    fn start_host(&self, pane_rect: &PaneRect) -> io::Result<PaneHost> {
        PaneHost::start(
            CommandBuilder::new(&self.shell),
            pane_rect.cols,
            pane_rect.rows,
        )
    }

    fn focused_pane(&self) -> io::Result<&str> {
        self.focused_tab()?
            .layout
            .focused
            .as_deref()
            .ok_or_else(|| invalid_data("focused tab has no focused pane"))
    }

    fn focused_tab(&self) -> io::Result<&Tab> {
        let tab_id = self
            .focused_tab
            .as_deref()
            .ok_or_else(|| invalid_data("session has no focused tab"))?;
        self.tab(tab_id)
    }

    fn tab(&self, tab_id: &str) -> io::Result<&Tab> {
        self.tree
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .find(|tab| tab.id == tab_id)
            .ok_or_else(|| invalid_data("focused tab is missing"))
    }

    fn tab_mut(&mut self, tab_id: &str) -> io::Result<&mut Tab> {
        self.tree
            .workspaces
            .iter_mut()
            .flat_map(|workspace| &mut workspace.tabs)
            .find(|tab| tab.id == tab_id)
            .ok_or_else(|| invalid_data("focused tab is missing"))
    }

    fn tab_id_for_pane(&self, pane: &str) -> io::Result<&str> {
        self.tree
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .find(|tab| tab.panes.iter().any(|candidate| candidate.id == pane))
            .map(|tab| tab.id.as_str())
            .ok_or_else(|| tree_error(TreeError::PaneNotFound(pane.to_owned())))
    }

    fn tree_message(&self) -> Vec<ServerMsg> {
        vec![ServerMsg::Tree {
            tree: self.tree.clone(),
        }]
    }
}

fn find_rect(tab: &Tab, viewport: PaneSize, pane: &str) -> io::Result<PaneRect> {
    rects(tab, viewport.cols, viewport.rows)
        .into_iter()
        .find(|pane_rect| pane_rect.pane == pane)
        .ok_or_else(|| invalid_data("pane is missing from its layout"))
}

fn tree_error(error: TreeError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error)
}

fn pane_host_not_found(pane: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("pane host not found: {pane}"),
    )
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
