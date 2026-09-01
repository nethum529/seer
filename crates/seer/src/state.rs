use std::collections::HashMap;

use ratatui::layout::Rect;
use seer_core::{Cell, Tab, Tree};

#[derive(Debug)]
pub(crate) struct ClientState {
    tree: Tree,
    buffers: HashMap<String, Vec<Vec<Cell>>>,
    focused: Option<String>,
}

impl ClientState {
    pub(crate) fn new(tree: Tree) -> Self {
        let focused = preferred_focus(&tree);
        Self {
            tree,
            buffers: HashMap::new(),
            focused,
        }
    }

    pub(crate) fn replace_tree(&mut self, tree: Tree) {
        self.tree = tree;
        let focus_is_valid = self
            .focused
            .as_deref()
            .is_some_and(|pane| self.visible_pane_ids().any(|id| id == pane));
        if !focus_is_valid {
            self.focused = preferred_focus(&self.tree);
        }
    }

    pub(crate) fn apply_cells(&mut self, pane: String, rows: Vec<Vec<Cell>>) {
        self.buffers.insert(pane, rows);
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

    pub(crate) fn visible_tab(&self) -> Option<&Tab> {
        self.tree.workspaces.first()?.tabs.first()
    }

    pub(crate) fn pane_rows(&self, pane: &str) -> &[Vec<Cell>] {
        self.buffers.get(pane).map_or(&[], Vec::as_slice)
    }

    fn visible_pane_ids(&self) -> impl Iterator<Item = &str> {
        self.visible_tab()
            .into_iter()
            .flat_map(|tab| tab.panes.iter())
            .map(|pane| pane.id.as_str())
    }
}

fn preferred_focus(tree: &Tree) -> Option<String> {
    let tab = tree.workspaces.first()?.tabs.first()?;
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
    use seer_core::{Cell, Color, PaneSize, SplitDirection, Tree};

    use super::{ClientState, pane_rects};

    #[test]
    fn cells_replace_the_pane_buffer() {
        let mut tree = Tree::new();
        tree.create_workspace("main")
            .expect("workspace must be created");
        tree.create_tab("w1", "shell", PaneSize { cols: 80, rows: 24 })
            .expect("tab must be created");
        let mut state = ClientState::new(tree);
        let rows = vec![vec![Cell {
            character: 'A',
            fg: Color::Indexed(2),
            bg: Color::Default,
            bold: true,
            italic: false,
            underline: false,
            dim: false,
            inverse: false,
            hidden: false,
            strikeout: false,
        }]];

        state.apply_cells("w1:p1".into(), rows.clone());

        assert_eq!(state.pane_rows("w1:p1"), rows);
        assert!(state.pane_rows("w1:p2").is_empty());
    }

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
