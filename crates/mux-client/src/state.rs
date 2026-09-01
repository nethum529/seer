use std::collections::HashMap;

use mux_core::{Cell, LayoutNode, SplitDirection, Tab, Tree};
use ratatui::layout::Rect;

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

// TODO: Replace this function with mux_core::layout::rects after issue 41 merges.
pub(crate) fn pane_rects(tab: &Tab, area: Rect) -> Vec<(String, Rect)> {
    let mut rects = Vec::new();
    if let Some(root) = &tab.layout.root {
        collect_rects(root, area, &mut rects);
    }
    rects
}

fn collect_rects(node: &LayoutNode, area: Rect, rects: &mut Vec<(String, Rect)>) {
    match node {
        LayoutNode::Pane { pane } => rects.push((pane.clone(), area)),
        LayoutNode::Split {
            direction,
            first,
            second,
        } => {
            let (first_area, second_area) = split_area(area, *direction);
            collect_rects(first, first_area, rects);
            collect_rects(second, second_area, rects);
        }
    }
}

fn split_area(area: Rect, direction: SplitDirection) -> (Rect, Rect) {
    match direction {
        SplitDirection::Right => {
            let first_width = area.width / 2 + area.width % 2;
            let second_width = area.width / 2;
            (
                Rect::new(area.x, area.y, first_width, area.height),
                Rect::new(
                    area.x.saturating_add(first_width),
                    area.y,
                    second_width,
                    area.height,
                ),
            )
        }
        SplitDirection::Down => {
            let first_height = area.height / 2 + area.height % 2;
            let second_height = area.height / 2;
            (
                Rect::new(area.x, area.y, area.width, first_height),
                Rect::new(
                    area.x,
                    area.y.saturating_add(first_height),
                    area.width,
                    second_height,
                ),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use mux_core::{Cell, Color, PaneSize, Tree};

    use super::ClientState;

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
}
