use std::collections::HashMap;

use ratatui::layout::Rect;
use seer_core::{Cell, Cursor, MouseTracking, Tab, TerminalFrame, Tree};

#[derive(Debug)]
pub(crate) struct ClientState {
    tree: Tree,
    frames: HashMap<String, TerminalFrame>,
    selected_workspace: Option<String>,
    selected_tab: Option<String>,
    focused: Option<String>,
    pane_areas: Vec<(String, Rect)>,
}

impl ClientState {
    pub(crate) fn new(tree: Tree) -> Self {
        let (selected_workspace, selected_tab) = first_selection(&tree)
            .map_or((None, None), |(workspace, tab)| {
                (Some(workspace), Some(tab))
            });
        let focused = selected_tab_in(
            &tree,
            selected_workspace.as_deref(),
            selected_tab.as_deref(),
        )
        .and_then(preferred_focus);
        Self {
            tree,
            frames: HashMap::new(),
            selected_workspace,
            selected_tab,
            focused,
            pane_areas: Vec::new(),
        }
    }

    pub(crate) fn replace_tree(&mut self, tree: Tree) -> bool {
        let previous_workspace = self.selected_workspace.clone();
        let previous_tab = self.selected_tab.clone();
        self.tree = tree;
        if self.visible_tab().is_none() {
            (self.selected_workspace, self.selected_tab) = first_selection(&self.tree)
                .map_or((None, None), |(workspace, tab)| {
                    (Some(workspace), Some(tab))
                });
        }
        let focus_is_valid = self
            .focused
            .as_deref()
            .is_some_and(|pane| self.visible_pane_ids().any(|id| id == pane));
        if !focus_is_valid {
            self.focused = self.visible_tab().and_then(preferred_focus);
        }
        previous_workspace != self.selected_workspace || previous_tab != self.selected_tab
    }

    pub(crate) fn apply_frame(&mut self, pane: String, frame: TerminalFrame) {
        self.frames.insert(pane, frame);
    }

    pub(crate) fn focus_number(&mut self, number: usize) -> Option<String> {
        let pane = self
            .visible_pane_ids()
            .nth(number.checked_sub(1)?)?
            .to_owned();
        self.focused = Some(pane.clone());
        Some(pane)
    }

    pub(crate) fn focused(&self) -> Option<&str> {
        self.focused.as_deref()
    }

    pub(crate) fn set_focus(&mut self, pane: String) {
        self.focused = Some(pane);
    }

    pub(crate) fn visible_tab(&self) -> Option<&Tab> {
        selected_tab_in(
            &self.tree,
            self.selected_workspace.as_deref(),
            self.selected_tab.as_deref(),
        )
    }

    pub(crate) fn selection(&self) -> Option<(&str, &str)> {
        Some((
            self.selected_workspace.as_deref()?,
            self.selected_tab.as_deref()?,
        ))
    }

    pub(crate) fn selected_workspace(&self) -> Option<&str> {
        self.selected_workspace.as_deref()
    }

    pub(crate) fn pane_rows(&self, pane: &str) -> &[Vec<Cell>] {
        self.frames
            .get(pane)
            .map_or(&[], |frame| frame.rows.as_slice())
    }

    pub(crate) fn pane_cursor(&self, pane: &str) -> Option<Cursor> {
        self.frames.get(pane).map(|frame| frame.cursor)
    }

    pub(crate) fn pane_mouse_tracking(&self, pane: &str) -> MouseTracking {
        self.frames
            .get(pane)
            .map_or(MouseTracking::None, |frame| frame.modes.mouse_tracking)
    }

    pub(crate) fn set_pane_areas(&mut self, areas: Vec<(String, Rect)>) {
        self.pane_areas = areas;
    }

    pub(crate) fn mouse_target(&self, column: u16, row: u16) -> Option<(String, u16, u16)> {
        self.pane_areas.iter().find_map(|(pane, area)| {
            let inside = column >= area.x
                && column < area.x.saturating_add(area.width)
                && row >= area.y
                && row < area.y.saturating_add(area.height);
            inside.then(|| (pane.clone(), column - area.x, row - area.y))
        })
    }

    fn visible_pane_ids(&self) -> impl Iterator<Item = &str> {
        self.visible_tab()
            .into_iter()
            .flat_map(|tab| tab.panes.iter())
            .map(|pane| pane.id.as_str())
    }
}

fn first_selection(tree: &Tree) -> Option<(String, String)> {
    tree.workspaces.iter().find_map(|workspace| {
        workspace
            .tabs
            .first()
            .map(|tab| (workspace.id.clone(), tab.id.clone()))
    })
}

fn selected_tab_in<'a>(
    tree: &'a Tree,
    workspace: Option<&str>,
    tab: Option<&str>,
) -> Option<&'a Tab> {
    tree.workspaces
        .iter()
        .find(|candidate| Some(candidate.id.as_str()) == workspace)?
        .tabs
        .iter()
        .find(|candidate| Some(candidate.id.as_str()) == tab)
}

fn preferred_focus(tab: &Tab) -> Option<String> {
    tab.layout
        .focused
        .as_ref()
        .filter(|focused| tab.panes.iter().any(|pane| pane.id == **focused))
        .cloned()
        .or_else(|| tab.panes.first().map(|pane| pane.id.clone()))
}

pub(crate) fn pane_rects(tab: &Tab, area: Rect) -> Vec<(String, Rect)> {
    seer_core::layout::rects(tab, area.width, area.height)
        .into_iter()
        .map(|rect| {
            (
                rect.pane,
                Rect::new(
                    area.x.saturating_add(rect.x),
                    area.y.saturating_add(rect.y),
                    rect.cols,
                    rect.rows,
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use seer_core::{PaneSize, SplitDirection, Tree};

    use super::pane_rects;

    #[test]
    fn pane_rects_use_shared_layout_with_area_offsets() {
        let mut tree = Tree::new();
        tree.create_workspace("main")
            .expect("workspace must be created");
        tree.create_tab("w1", "shell", PaneSize { cols: 80, rows: 24 })
            .expect("tab must be created");
        tree.split_pane("w1:p1", SplitDirection::Right)
            .expect("first split must be created");
        let tab = tree
            .split_pane("w1:p2", SplitDirection::Down)
            .expect("second split must be created");

        assert_eq!(
            pane_rects(&tab, Rect::new(10, 20, 81, 25)),
            vec![
                ("w1:p1".into(), Rect::new(10, 20, 41, 25)),
                ("w1:p2".into(), Rect::new(51, 20, 40, 13)),
                ("w1:p3".into(), Rect::new(51, 33, 40, 12)),
            ]
        );
    }
}
