use crate::{LayoutNode, SplitDirection, Tab};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneRect {
    pub pane: String,
    pub cols: u16,
    pub rows: u16,
    pub x: u16,
    pub y: u16,
}

#[must_use]
pub fn rects(tab: &Tab, cols: u16, rows: u16) -> Vec<PaneRect> {
    let mut pane_rects = Vec::with_capacity(tab.panes.len());
    if let Some(root) = tab.layout.root.as_ref() {
        collect_rects(root, Area::new(cols, rows), &mut pane_rects);
    }
    pane_rects
}

#[derive(Clone, Copy)]
struct Area {
    cols: u16,
    rows: u16,
    x: u16,
    y: u16,
}

impl Area {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            x: 0,
            y: 0,
        }
    }
}

fn collect_rects(node: &LayoutNode, area: Area, pane_rects: &mut Vec<PaneRect>) {
    match node {
        LayoutNode::Pane { pane } => pane_rects.push(PaneRect {
            pane: pane.clone(),
            cols: area.cols,
            rows: area.rows,
            x: area.x,
            y: area.y,
        }),
        LayoutNode::Split {
            direction,
            first,
            second,
        } => {
            let (first_area, second_area) = split_area(area, *direction);
            collect_rects(first, first_area, pane_rects);
            collect_rects(second, second_area, pane_rects);
        }
    }
}

fn split_area(area: Area, direction: SplitDirection) -> (Area, Area) {
    match direction {
        SplitDirection::Right => {
            let second_cols = area.cols / 2;
            let first_cols = area.cols - second_cols;
            (
                Area {
                    cols: first_cols,
                    ..area
                },
                Area {
                    cols: second_cols,
                    x: area.x + first_cols,
                    ..area
                },
            )
        }
        SplitDirection::Down => {
            let second_rows = area.rows / 2;
            let first_rows = area.rows - second_rows;
            (
                Area {
                    rows: first_rows,
                    ..area
                },
                Area {
                    rows: second_rows,
                    y: area.y + first_rows,
                    ..area
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PaneSize, Tree};

    const SIZE: PaneSize = PaneSize { cols: 80, rows: 24 };

    fn tab_with_one_pane() -> (Tree, Tab) {
        let mut tree = Tree::new();
        tree.create_workspace("main")
            .expect("workspace creation must succeed");
        let tab = tree
            .create_tab("w1", "shell", SIZE)
            .expect("tab creation must succeed");
        (tree, tab)
    }

    #[test]
    fn lays_out_one_pane() {
        let (_, tab) = tab_with_one_pane();

        assert_eq!(
            rects(&tab, 81, 25),
            vec![PaneRect {
                pane: "w1:p1".into(),
                cols: 81,
                rows: 25,
                x: 0,
                y: 0,
            }]
        );
    }

    #[test]
    fn splits_right_and_gives_the_first_pane_the_extra_column() {
        let (mut tree, _) = tab_with_one_pane();
        let tab = tree
            .split_pane("w1:p1", SplitDirection::Right)
            .expect("pane split must succeed");

        assert_eq!(
            rects(&tab, 81, 25),
            vec![
                PaneRect {
                    pane: "w1:p1".into(),
                    cols: 41,
                    rows: 25,
                    x: 0,
                    y: 0,
                },
                PaneRect {
                    pane: "w1:p2".into(),
                    cols: 40,
                    rows: 25,
                    x: 41,
                    y: 0,
                },
            ]
        );
    }

    #[test]
    fn splits_down_and_gives_the_first_pane_the_extra_row() {
        let (mut tree, _) = tab_with_one_pane();
        let tab = tree
            .split_pane("w1:p1", SplitDirection::Down)
            .expect("pane split must succeed");

        assert_eq!(
            rects(&tab, 81, 25),
            vec![
                PaneRect {
                    pane: "w1:p1".into(),
                    cols: 81,
                    rows: 13,
                    x: 0,
                    y: 0,
                },
                PaneRect {
                    pane: "w1:p2".into(),
                    cols: 81,
                    rows: 12,
                    x: 0,
                    y: 13,
                },
            ]
        );
    }

    #[test]
    fn lays_out_nested_splits() {
        let (mut tree, _) = tab_with_one_pane();
        tree.split_pane("w1:p1", SplitDirection::Right)
            .expect("pane split must succeed");
        let tab = tree
            .split_pane("w1:p2", SplitDirection::Down)
            .expect("pane split must succeed");

        assert_eq!(
            rects(&tab, 81, 25),
            vec![
                PaneRect {
                    pane: "w1:p1".into(),
                    cols: 41,
                    rows: 25,
                    x: 0,
                    y: 0,
                },
                PaneRect {
                    pane: "w1:p2".into(),
                    cols: 40,
                    rows: 13,
                    x: 41,
                    y: 0,
                },
                PaneRect {
                    pane: "w1:p3".into(),
                    cols: 40,
                    rows: 12,
                    x: 41,
                    y: 13,
                },
            ]
        );
    }
}
