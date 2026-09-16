use crate::PaneHost;
use crate::persistence::{self, Store};
use crate::room::SECRET_VARS;
use crate::{shell_env, shell_exit};
use portable_pty::CommandBuilder;
use seer_core::layout::{PaneRect, rects};
use seer_core::proto::{ClientMsg, PeekTarget, ServerMsg, TerminalInfo};
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
    pub(crate) published_terminals: Vec<TerminalInfo>,
    pub tree: Tree,
    pub(crate) shell: String,
    pub(crate) viewport: PaneSize,
    pub(crate) pane_hosts: BTreeMap<String, PaneHost>,
    pub(crate) store: Option<Store>,
}

impl UserSession {
    #[must_use]
    pub fn new(user: impl Into<String>, shell: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            published_terminals: Vec::new(),
            tree: Tree::new(),
            shell: shell.into(),
            viewport: PaneSize {
                cols: DEFAULT_COLS,
                rows: DEFAULT_ROWS,
            },
            pane_hosts: BTreeMap::new(),
            store: None,
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
            ClientMsg::GrantedMouse {
                workspace,
                tab,
                pane,
                mouse,
                sender,
            } => self.granted_mouse(&workspace, &tab, &pane, mouse, sender),
            ClientMsg::GrantedInput {
                workspace,
                tab,
                pane,
                bytes,
                sender,
            } => self.granted_input(&workspace, &tab, &pane, &bytes, sender),
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
            | ClientMsg::Terminals { .. }
            | ClientMsg::SetAllGrants { .. }
            | ClientMsg::SetGrant { .. }
            | ClientMsg::MouseInto { .. }
            | ClientMsg::TypeInto { .. }
            | ClientMsg::Stop
            | ClientMsg::Leave
            | ClientMsg::Detach => Ok(Vec::new()),
        }
    }

    #[must_use]
    pub fn poll(&mut self) -> Vec<ServerMsg> {
        let mut messages: Vec<_> = self
            .pane_hosts
            .iter_mut()
            .filter_map(|(pane, host)| {
                host.poll().then(|| {
                    let frame = host.frame();
                    #[cfg(debug_assertions)]
                    seer_core::debug_log::transition(
                        &format!("frame pane={pane}"),
                        seer_core::debug_log::frame_summary(&frame),
                    );
                    ServerMsg::Cells {
                        user: self.user.clone(),
                        pane: pane.clone(),
                        frame,
                    }
                })
            })
            .collect();
        messages.extend(self.reap_shells());
        let terminals = self.terminals();
        if terminals != self.published_terminals {
            self.published_terminals.clone_from(&terminals);
            let message = ServerMsg::Terminals {
                user: self.user.clone(),
                terminals,
            };
            seer_core::debug_log!("{}", seer_core::debug_log::server_summary(&message));
            messages.push(message);
        }
        messages
    }

    #[must_use]
    pub(crate) fn tab_count(&self) -> u32 {
        let tabs: usize = self
            .tree
            .workspaces
            .iter()
            .map(|workspace| workspace.tabs.len())
            .sum();
        u32::try_from(tabs).unwrap_or(u32::MAX)
    }

    #[must_use]
    pub(crate) fn foreground(&self, workspace: &str, tab: &str) -> String {
        self.focused_pane(workspace, tab)
            .ok()
            .and_then(|pane| self.pane_hosts.get(pane))
            .map(PaneHost::foreground)
            .unwrap_or_default()
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
                    user: self.user.clone(),
                    pane: pane.id.clone(),
                    frame: host.frame(),
                })
            })
            .collect::<Vec<_>>();
        let mut messages = vec![ServerMsg::Tree { tree }];
        messages.extend(cells);
        messages.push(ServerMsg::Terminals {
            user: self.user.clone(),
            terminals: self.terminals(),
        });
        messages
    }

    #[must_use]
    pub(crate) fn targets(&self, active: Option<(&str, &str)>) -> Vec<PeekTarget> {
        self.tree
            .workspaces
            .iter()
            .flat_map(|workspace| {
                workspace.tabs.iter().map(|tab| PeekTarget {
                    workspace: workspace.id.clone(),
                    workspace_name: workspace.name.clone(),
                    tab: tab.id.clone(),
                    tab_title: tab.title.clone(),
                    active: active == Some((workspace.id.as_str(), tab.id.as_str())),
                })
            })
            .collect()
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
        persistence::persist(self)?;
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
        persistence::persist(self)?;
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
        self.remove_pane(workspace, tab, pane)
    }

    fn focus_pane(&mut self, workspace: &str, tab: &str, pane: &str) -> io::Result<Vec<ServerMsg>> {
        self.validate_pane(workspace, tab, pane)?;
        self.tree.focus_pane(pane).map_err(tree_error)?;
        persistence::persist(self)?;
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
                user: self.user.clone(),
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
        persistence::persist(self)?;
        Ok(self.tree_message())
    }

    fn resize_tab(&mut self, workspace: &str, tab: &str) -> io::Result<()> {
        let pane_rects = {
            let tab = self.tab(workspace, tab)?;
            rects(tab, self.viewport.cols, self.viewport.rows)
        };

        for pane_rect in &pane_rects {
            seer_core::debug_log!(
                "pty resize pane={} size={}x{} viewport={}x{}",
                pane_rect.pane,
                pane_rect.cols,
                pane_rect.rows,
                self.viewport.cols,
                self.viewport.rows
            );
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

    pub(crate) fn start_host(&self, pane_rect: &PaneRect) -> io::Result<PaneHost> {
        let mut command = CommandBuilder::new(&self.shell);
        shell_env::remove_session_vars(&mut command);
        shell_exit::install(&mut command, &self.shell)?;
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("SEER_USER_ID", &self.user);
        command.env("SEER_PANE", &pane_rect.pane);
        for name in SECRET_VARS {
            command.env_remove(name);
        }
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

pub(super) fn validate_capabilities(capabilities: TerminalCapabilities) -> io::Result<()> {
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

pub(crate) mod terminals;

mod lifecycle;
