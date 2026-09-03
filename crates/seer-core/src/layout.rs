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
