use crate::PaneHost;
use portable_pty::CommandBuilder;
use seer_core::layout::{PaneRect, rects};
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{
    PaneSize, TERMINAL_PROTOCOL_VERSION, Tab, TerminalCapabilities, TerminalInput, Tree, TreeError,
};
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
            pane_hosts: BTreeMap::new(),
        }
    }

    pub fn apply(&mut self, msg: ClientMsg) -> io::Result<Vec<ServerMsg>> {
        match msg {
            ClientMsg::CreateTab { workspace } => self.create_tab(&workspace),
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
            | ClientMsg::DetachClient { .. }
            | ClientMsg::Peek { .. }
            | ClientMsg::StopPeek
            | ClientMsg::Detach => Ok(Vec::new()),
        }
    }

    pub fn poll(&mut self) -> io::Result<Vec<ServerMsg>> {
        let mut messages = Vec::new();
        for (pane, host) in &mut self.pane_hosts {
            if host.poll()? {
                messages.push(ServerMsg::Cells {
                    pane: pane.clone(),
                    frame: host.frame(),
                });
            }
        }
        Ok(messages)
    }

    #[must_use]
    pub(crate) fn snapshot(&self) -> Vec<ServerMsg> {
        self.snapshot_for(self.tree.clone())
    }

    #[must_use]
    pub(crate) fn snapshot_for(&self, tree: Tree) -> Vec<ServerMsg> {
        let cells = tree
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .flat_map(|tab| &tab.panes)
            .filter_map(|pane| {
                self.pane_hosts.get(&pane.id).map(|host| ServerMsg::Cells {
                    pane: pane.id.clone(),
                    frame: host.frame(),
                })
            })
            .collect::<Vec<_>>();
        let mut messages = vec![ServerMsg::Tree { tree }];
        messages.extend(cells);
        messages
    }

    pub(crate) fn ensure_first_shell(&mut self) -> io::Result<()> {
        if self
            .tree
            .workspaces
            .iter()
            .any(|workspace| !workspace.tabs.is_empty())
        {
            return Ok(());
        }
        let workspace = self.ensure_workspace()?;
        self.create_tab(&workspace).map(|_| ())
    }

    fn create_tab(&mut self, workspace: &str) -> io::Result<Vec<ServerMsg>> {
        let tab = self
            .tree
            .create_tab(workspace, DEFAULT_TAB_TITLE, self.viewport)
            .map_err(tree_error)?;
        let pane = tab
            .layout
            .focused
            .as_deref()
            .ok_or_else(|| invalid_data("new tab has no focused pane"))?;
        let pane_rect = find_rect(&tab, self.viewport, pane)?;
        let host = self.start_host(&pane_rect)?;

        self.pane_hosts.insert(pane.to_owned(), host);
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

    fn split_pane(
        &mut self,
        workspace: &str,
        tab: &str,
        direction: seer_core::SplitDirection,
    ) -> io::Result<Vec<ServerMsg>> {
        let focused = self.focused_pane(workspace, tab)?.to_owned();
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
        self.resize_tab(workspace, &tab.id)?;
        Ok(self.tree_message())
    }

    fn close_pane(&mut self, workspace: &str, tab: &str, pane: &str) -> io::Result<Vec<ServerMsg>> {
        self.validate_pane(workspace, tab, pane)?;
        let mut host = self
            .pane_hosts
            .remove(pane)
            .ok_or_else(|| pane_host_not_found(pane))?;
        if let Err(error) = host.kill() {
            self.pane_hosts.insert(pane.to_owned(), host);
            return Err(error);
        }
        let closed_tab = self.tree.close_pane(pane).map_err(tree_error)?;
        if closed_tab.panes.is_empty() {
            self.tree.close_tab(workspace, tab).map_err(tree_error)?;
        } else {
            self.resize_tab(workspace, tab)?;
        }
        Ok(self.tree_message())
    }

    fn focus_pane(&mut self, workspace: &str, tab: &str, pane: &str) -> io::Result<Vec<ServerMsg>> {
        self.validate_pane(workspace, tab, pane)?;
        self.tree.focus_pane(pane).map_err(tree_error)?;
        Ok(self.tree_message())
    }

    fn terminal_input(
        &mut self,
        workspace: &str,
        tab: &str,
        pane: &str,
        input: &TerminalInput,
    ) -> io::Result<Vec<ServerMsg>> {
        self.validate_pane(workspace, tab, pane)?;
        let host = self
            .pane_hosts
            .get_mut(pane)
            .ok_or_else(|| pane_host_not_found(pane))?;
        let changed = host.handle_input(input)?;
        Ok(changed
            .then(|| ServerMsg::Cells {
                pane: pane.to_owned(),
                frame: host.frame(),
            })
            .into_iter()
            .collect())
    }

    fn resize(
        &mut self,
        workspace: &str,
        tab: &str,
        cols: u16,
        rows: u16,
    ) -> io::Result<Vec<ServerMsg>> {
        self.tab(workspace, tab)?;
        self.viewport = PaneSize { cols, rows };
        self.resize_tab(workspace, tab)?;
        Ok(self.tree_message())
    }

    fn resize_tab(&mut self, workspace: &str, tab: &str) -> io::Result<()> {
        let pane_rects = {
            let tab = self.tab(workspace, tab)?;
            rects(tab, self.viewport.cols, self.viewport.rows)
        };

        for pane_rect in &pane_rects {
            self.pane_hosts
                .get_mut(&pane_rect.pane)
                .ok_or_else(|| pane_host_not_found(&pane_rect.pane))?
                .resize(pane_rect.cols, pane_rect.rows)?;
        }
        self.store_pane_sizes(workspace, tab, &pane_rects)
    }

    fn store_pane_sizes(
        &mut self,
        workspace: &str,
        tab: &str,
        pane_rects: &[PaneRect],
    ) -> io::Result<()> {
        let tab = self.tab_mut(workspace, tab)?;
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
        let mut command = CommandBuilder::new(&self.shell);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        PaneHost::start(command, pane_rect.cols, pane_rect.rows)
    }

    fn focused_pane(&self, workspace: &str, tab: &str) -> io::Result<&str> {
        self.tab(workspace, tab)?
            .layout
            .focused
            .as_deref()
            .ok_or_else(|| invalid_data("focused tab has no focused pane"))
    }

    fn tab(&self, workspace: &str, tab: &str) -> io::Result<&Tab> {
        let workspace = self
            .tree
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| tree_error(TreeError::WorkspaceNotFound(workspace.to_owned())))?;
        workspace
            .tabs
            .iter()
            .find(|candidate| candidate.id == tab)
            .ok_or_else(|| invalid_input(format!("tab not found: {tab}")))
    }

    fn tab_mut(&mut self, workspace: &str, tab: &str) -> io::Result<&mut Tab> {
        let workspace = self
            .tree
            .workspaces
            .iter_mut()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| tree_error(TreeError::WorkspaceNotFound(workspace.to_owned())))?;
        workspace
            .tabs
            .iter_mut()
            .find(|candidate| candidate.id == tab)
            .ok_or_else(|| invalid_input(format!("tab not found: {tab}")))
    }

    fn validate_pane(&self, workspace: &str, tab: &str, pane: &str) -> io::Result<()> {
        if self
            .tab(workspace, tab)?
            .panes
            .iter()
            .any(|candidate| candidate.id == pane)
        {
            Ok(())
        } else {
            Err(tree_error(TreeError::PaneNotFound(pane.to_owned())))
        }
    }

    pub(crate) fn selected_tree(&self, workspace: &str, tab: &str) -> io::Result<Tree> {
        self.tab(workspace, tab)?;
        let mut tree = self.tree.clone();
        tree.workspaces
            .retain(|candidate| candidate.id == workspace);
        for workspace in &mut tree.workspaces {
            workspace.tabs.retain(|candidate| candidate.id == tab);
        }
        Ok(tree)
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

fn invalid_input(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn validate_capabilities(capabilities: TerminalCapabilities) -> io::Result<()> {
    if capabilities.protocol_version == TERMINAL_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported terminal capability version",
        ))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
