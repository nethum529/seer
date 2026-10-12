use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};

use super::{Dimensions, PaneGrid, Scroll, display_offset};

impl PaneGrid {
    pub fn start_selection(&mut self, column: u16, row: u16, right: bool, clicks: usize) {
        let kind = match clicks {
            2 => SelectionType::Semantic,
            3.. => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        self.terminal.selection = Some(Selection::new(
            kind,
            self.selection_point(column, row),
            side(right),
        ));
    }

    pub fn update_selection(&mut self, column: u16, row: u16, right: bool) {
        let point = self.selection_point(column, row);
        if let Some(selection) = &mut self.terminal.selection {
            selection.update(point, side(right));
        }
    }

    #[must_use]
    pub fn selection_text(&self) -> Option<String> {
        self.terminal.selection_to_string()
    }

    #[must_use]
    pub fn selection_spans(&self) -> Vec<(usize, usize, usize)> {
        let Some(range) = self
            .terminal
            .selection
            .as_ref()
            .and_then(|s| s.to_range(&self.terminal))
        else {
            return Vec::new();
        };
        let offset = display_offset(self.terminal.grid());
        (0..self.terminal.screen_lines())
            .filter_map(|row| {
                let line = Line(row as i32 - offset);
                let start = if line == range.start.line {
                    range.start.column.0
                } else {
                    0
                };
                let end = if line == range.end.line {
                    range.end.column.0 + 1
                } else {
                    self.terminal.columns()
                };
                (line >= range.start.line && line <= range.end.line).then_some((row, start, end))
            })
            .collect()
    }

    pub fn input_started(&mut self) -> bool {
        let changed =
            self.terminal.selection.is_some() || self.terminal.grid().display_offset() != 0;
        self.terminal.selection = None;
        self.terminal.scroll_display(Scroll::Bottom);
        changed
    }

    fn selection_point(&self, column: u16, row: u16) -> Point {
        Point::new(
            Line(i32::from(row) - display_offset(self.terminal.grid())),
            Column(usize::from(column)),
        )
    }
}

fn side(right: bool) -> Side {
    if right { Side::Right } else { Side::Left }
}
