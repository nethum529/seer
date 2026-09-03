use crate::snapshot::{self, Snapshot};
use crate::PaneHost;
use portable_pty::CommandBuilder;
use seer_core::layout::{PaneRect, rects};
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{PaneSize, Tab, Tree, TreeError};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
    snapshot_path: Option<PathBuf>,
    revision: u64,
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
            snapshot_path: None,
            revision: 0,
        }
    }

    /// Loads the session snapshot from `snapshot_dir`, or starts an empty
    /// session when the directory is absent or the snapshot is unusable.
    ///
    /// A valid snapshot is restored with replacement shells. A missing,
    /// corrupt, unsupported, or inconsistent snapshot always starts a safe
    /// empty session instead of failing the runtime. The empty session keeps
    /// the snapshot path, so the first shell it creates is persisted.
    #[must_use]
    pub fn load_or_new(
        user: impl Into<String>,
        shell: impl Into<String>,
        snapshot_dir: Option<&Path>,
    ) -> Self {
        let user = user.into();
        let shell = shell.into();
        let Some(directory) = snapshot_dir else {
            return Self::new(user, shell);
        };
        if let Err(error) = fs::create_dir_all(directory) {
            eprintln!("runtime cannot open the snapshot directory: {error}");
            return Self::new(user, shell);
        }
        let path = snapshot::snapshot_path(directory);
        match snapshot::load(&path, &user) {
            Some(snapshot) => match Self::from_snapshot(&user, &shell, path.clone(), snapshot) {
                Ok(session) => session,
                Err(error) => {
                    eprintln!("runtime cannot restore the session snapshot: {error}");
                    Self::with_snapshot_path(user, shell, path)
                }
            },
            None => Self::with_snapshot_path(user, shell, path),
        }
    }

    fn with_snapshot_path(user: String, shell: String, path: PathBuf) -> Self {
        let mut session = Self::new(user, shell);
        session.snapshot_path = Some(path);
        session
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
            ClientMsg::Input {
                workspace,
                tab,
                pane,
                bytes,
            } => self.write_input(&workspace, &tab, &pane, &bytes),
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
                    rows: host.cells(),
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
        self.persist();
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
        self.persist();
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
        self.tree.close_pane(pane).map_err(tree_error)?;
        self.resize_tab(workspace, tab)?;
        self.persist();
        Ok(self.tree_message())
    }

    fn focus_pane(&mut self, workspace: &str, tab: &str, pane: &str) -> io::Result<Vec<ServerMsg>> {
        self.validate_pane(workspace, tab, pane)?;
        self.tree.focus_pane(pane).map_err(tree_error)?;
        self.persist();
        Ok(self.tree_message())
    }

    fn write_input(
        &mut self,
        workspace: &str,
        tab: &str,
        pane: &str,
        bytes: &[u8],
    ) -> io::Result<Vec<ServerMsg>> {
        self.validate_pane(workspace, tab, pane)?;
        self.pane_hosts
            .get_mut(pane)
            .ok_or_else(|| pane_host_not_found(pane))?
            .write_input(bytes)?;
        Ok(Vec::new())
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
        self.persist();
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
        PaneHost::start(
            CommandBuilder::new(&self.shell),
            pane_rect.cols,
            pane_rect.rows,
        )
    }

    fn from_snapshot(
        user: &str,
        shell: &str,
        path: PathBuf,
        snapshot: Snapshot,
    ) -> io::Result<Self> {
        let mut session = Self {
            user: user.to_owned(),
            tree: snapshot.tree,
            shell: shell.to_owned(),
            viewport: snapshot.viewport,
            pane_hosts: BTreeMap::new(),
            snapshot_path: Some(path),
            revision: snapshot.revision,
        };
        session.restore_hosts()?;
        Ok(session)
    }

    /// Spawns one replacement shell per restored pane.
    ///
    /// The snapshot file is validated before this runs, so every pane id is
    /// unique and every pane has a usable size. Restoring never calls
    /// `create_tab`, so it cannot add a duplicate first pane.
    fn restore_hosts(&mut self) -> io::Result<()> {
        let launches = self.pane_launches();
        for (pane, size) in launches {
            let host = PaneHost::start(CommandBuilder::new(&self.shell), size.cols, size.rows)?;
            self.pane_hosts.insert(pane, host);
        }
        Ok(())
    }

    fn pane_launches(&self) -> Vec<(String, PaneSize)> {
        self.tree
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .flat_map(|tab| &tab.panes)
            .map(|pane| (pane.id.clone(), pane.size))
            .collect()
    }

    /// Writes the current session to the snapshot file.
    ///
    /// Persistence is best effort: a failed write must never break a live
    /// mutation, so the error is only reported on stderr.
    fn persist(&mut self) {
        let Some(path) = &self.snapshot_path else {
            return;
        };
        self.revision = self.revision.saturating_add(1);
        let snapshot = Snapshot::capture(
            self.revision,
            &self.user,
            &self.shell,
            self.viewport,
            &self.tree,
        );
        if let Err(error) = snapshot::store(path, &snapshot) {
            eprintln!("runtime snapshot save failed: {error}");
        }
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

#[cfg(all(test, target_os = "linux"))]
mod tests;
