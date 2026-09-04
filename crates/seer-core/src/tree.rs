use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    pub workspaces: Vec<Workspace>,
    next_workspace_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub tabs: Vec<Tab>,
    next_tab_id: u64,
    next_pane_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub panes: Vec<Pane>,
    pub layout: Layout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pane {
    pub id: String,
    pub size: PaneSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneSize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub root: Option<LayoutNode>,
    pub focused: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutNode {
    Pane {
        pane: String,
    },
    Split {
        direction: SplitDirection,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    Right,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreeError {
    WorkspaceNotFound(String),
    TabNotFound(String),
    PaneNotFound(String),
    IdExhausted,
}

impl Display for TreeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkspaceNotFound(id) => write!(formatter, "workspace not found: {id}"),
            Self::TabNotFound(id) => write!(formatter, "tab not found: {id}"),
            Self::PaneNotFound(id) => write!(formatter, "pane not found: {id}"),
            Self::IdExhausted => formatter.write_str("tree id space is exhausted"),
        }
    }
}

impl Error for TreeError {}

impl Default for Tree {
    fn default() -> Self {
        Self {
            workspaces: Vec::new(),
            next_workspace_id: 1,
        }
    }
}

impl Tree {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_workspace(&mut self, name: impl Into<String>) -> Result<Workspace, TreeError> {
        let number = take_id(&mut self.next_workspace_id)?;
        let workspace = Workspace {
            id: format!("w{number}"),
            name: name.into(),
            tabs: Vec::new(),
            next_tab_id: 1,
            next_pane_id: 1,
        };
        self.workspaces.push(workspace.clone());
        Ok(workspace)
    }

    pub fn create_tab(
        &mut self,
        workspace_id: &str,
        title: impl Into<String>,
        size: PaneSize,
    ) -> Result<Tab, TreeError> {
        let workspace = self.workspace_mut(workspace_id)?;
        let tab_number = take_id(&mut workspace.next_tab_id)?;
        let pane_number = take_id(&mut workspace.next_pane_id)?;
        let pane = Pane {
            id: format!("{}:p{pane_number}", workspace.id),
            size,
        };
        let tab = Tab {
            id: format!("{}:t{tab_number}", workspace.id),
            title: title.into(),
            panes: vec![pane.clone()],
            layout: Layout::from_pane(&pane.id),
        };
        workspace.tabs.push(tab.clone());
        Ok(tab)
    }

    pub fn split_pane(
        &mut self,
        pane_id: &str,
        direction: SplitDirection,
    ) -> Result<Tab, TreeError> {
        let (workspace_index, tab_index, pane_index) = self.pane_location(pane_id)?;
        let workspace = &mut self.workspaces[workspace_index];
        if !workspace.tabs[tab_index].layout.contains(pane_id) {
            return Err(TreeError::PaneNotFound(pane_id.to_owned()));
        }

        let pane_number = take_id(&mut workspace.next_pane_id)?;
        let pane = Pane {
            id: format!("{}:p{pane_number}", workspace.id),
            size: workspace.tabs[tab_index].panes[pane_index].size,
        };
        let tab = &mut workspace.tabs[tab_index];
        tab.layout.split(pane_id, &pane.id, direction);
        tab.panes.push(pane);
        Ok(tab.clone())
    }

    pub fn close_pane(&mut self, pane_id: &str) -> Result<Tab, TreeError> {
        let (workspace_index, tab_index, pane_index) = self.pane_location(pane_id)?;
        let tab = &mut self.workspaces[workspace_index].tabs[tab_index];
        if !tab.layout.remove(pane_id) {
            return Err(TreeError::PaneNotFound(pane_id.to_owned()));
        }

        tab.panes.remove(pane_index);
        Ok(tab.clone())
    }

    pub fn close_tab(&mut self, workspace_id: &str, tab_id: &str) -> Result<Tab, TreeError> {
        let workspace = self.workspace_mut(workspace_id)?;
        let tab_index = workspace
            .tabs
            .iter()
            .position(|tab| tab.id == tab_id)
            .ok_or_else(|| TreeError::TabNotFound(tab_id.to_owned()))?;
        Ok(workspace.tabs.remove(tab_index))
    }

    pub fn focus_pane(&mut self, pane_id: &str) -> Result<Tab, TreeError> {
        let (workspace_index, tab_index, _) = self.pane_location(pane_id)?;
        let tab = &mut self.workspaces[workspace_index].tabs[tab_index];
        if !tab.layout.contains(pane_id) {
            return Err(TreeError::PaneNotFound(pane_id.to_owned()));
        }

        tab.layout.focused = Some(pane_id.to_owned());
        Ok(tab.clone())
    }

    fn workspace_mut(&mut self, workspace_id: &str) -> Result<&mut Workspace, TreeError> {
        self.workspaces
            .iter_mut()
            .find(|workspace| workspace.id == workspace_id)
            .ok_or_else(|| TreeError::WorkspaceNotFound(workspace_id.to_owned()))
    }

    fn pane_location(&self, pane_id: &str) -> Result<(usize, usize, usize), TreeError> {
        for (workspace_index, workspace) in self.workspaces.iter().enumerate() {
            for (tab_index, tab) in workspace.tabs.iter().enumerate() {
                if let Some(pane_index) = tab.panes.iter().position(|pane| pane.id == pane_id) {
                    return Ok((workspace_index, tab_index, pane_index));
                }
            }
        }
        Err(TreeError::PaneNotFound(pane_id.to_owned()))
    }
}

impl Layout {
    fn from_pane(pane_id: &str) -> Self {
        Self {
            root: Some(LayoutNode::Pane {
                pane: pane_id.to_owned(),
            }),
            focused: Some(pane_id.to_owned()),
        }
    }

    fn contains(&self, pane_id: &str) -> bool {
        self.root
            .as_ref()
            .is_some_and(|root| root.contains(pane_id))
    }

    fn split(&mut self, pane_id: &str, new_pane_id: &str, direction: SplitDirection) {
        if let Some(root) = self.root.as_mut() {
            root.split(pane_id, new_pane_id, direction);
        }
        self.focused = Some(new_pane_id.to_owned());
    }

    fn remove(&mut self, pane_id: &str) -> bool {
        let Some(root) = self.root.take() else {
            return false;
        };
        if !root.contains(pane_id) {
            self.root = Some(root);
            return false;
        }

        let was_focused = self.focused.as_deref() == Some(pane_id);
        let (new_root, sibling) = root.remove(pane_id);
        self.root = new_root;
        if was_focused {
            self.focused = sibling.or_else(|| self.root.as_ref().map(LayoutNode::first_pane));
        }
        true
    }
}

impl LayoutNode {
    fn contains(&self, pane_id: &str) -> bool {
        match self {
            Self::Pane { pane } => pane == pane_id,
            Self::Split { first, second, .. } => {
                first.contains(pane_id) || second.contains(pane_id)
            }
        }
    }

    fn first_pane(&self) -> String {
        match self {
            Self::Pane { pane } => pane.clone(),
            Self::Split { first, .. } => first.first_pane(),
        }
    }

    fn split(&mut self, pane_id: &str, new_pane_id: &str, direction: SplitDirection) -> bool {
        match self {
            Self::Pane { pane } if pane == pane_id => {
                let first = Self::Pane { pane: pane.clone() };
                let second = Self::Pane {
                    pane: new_pane_id.to_owned(),
                };
                *self = Self::Split {
                    direction,
                    first: Box::new(first),
                    second: Box::new(second),
                };
                true
            }
            Self::Pane { .. } => false,
            Self::Split { first, second, .. } => {
                first.split(pane_id, new_pane_id, direction)
                    || second.split(pane_id, new_pane_id, direction)
            }
        }
    }

    fn remove(self, pane_id: &str) -> (Option<Self>, Option<String>) {
        match self {
            Self::Pane { .. } => (None, None),
            Self::Split {
                direction,
                first,
                second,
            } if first.contains(pane_id) => {
                let (new_first, sibling) = first.remove(pane_id);
                match new_first {
                    Some(first) => (
                        Some(Self::Split {
                            direction,
                            first: Box::new(first),
                            second,
                        }),
                        sibling,
                    ),
                    None => {
                        let sibling = second.first_pane();
                        (Some(*second), Some(sibling))
                    }
                }
            }
            Self::Split {
                direction,
                first,
                second,
            } => {
                let (new_second, sibling) = second.remove(pane_id);
                match new_second {
                    Some(second) => (
                        Some(Self::Split {
                            direction,
                            first,
                            second: Box::new(second),
                        }),
                        sibling,
                    ),
                    None => {
                        let sibling = first.first_pane();
                        (Some(*first), Some(sibling))
                    }
                }
            }
        }
    }
}

fn take_id(next_id: &mut u64) -> Result<u64, TreeError> {
    let id = *next_id;
    *next_id = next_id.checked_add(1).ok_or(TreeError::IdExhausted)?;
    Ok(id)
}

#[cfg(test)]
mod tests;

impl Tree {
    #[must_use]
    pub fn next_workspace_id(&self) -> u64 {
        self.next_workspace_id
    }
}

impl Workspace {
    #[must_use]
    pub fn next_tab_id(&self) -> u64 {
        self.next_tab_id
    }

    #[must_use]
    pub fn next_pane_id(&self) -> u64 {
        self.next_pane_id
    }
}
