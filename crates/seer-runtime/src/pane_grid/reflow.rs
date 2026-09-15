use alacritty_terminal::grid::{Dimensions, Grid, GridCell, Row};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as AlacrittyCell, Flags};
use seer_core::Cell;
use std::ops::RangeInclusive;

use super::map_cell;

pub(super) struct View {
    pub(super) rows: Vec<Vec<Cell>>,
    pub(super) cursor: Option<(u16, u16)>,
}

// Same rule as alacritty's own resize. A shorter view cuts the screen from
// the bottom and keeps the cursor in sight. A taller view pulls history in
// above the screen and keeps the bottom row at the bottom.
pub(super) fn view(grid: &Grid<AlacrittyCell>, cols: usize, rows: usize) -> View {
    let mut walker = Walker::new(grid, cols);
    walker.collect(|line, _| line >= 0);
    let screen_rows = walker.collected.len();
    if screen_rows > rows {
        let cursor_row = walker
            .cursor_at
            .map_or(0, |(index, _)| screen_rows - 1 - index);
        let start = (cursor_row + 1).saturating_sub(rows);
        let cursor = walker
            .cursor_at
            .map(|(_, column)| ((cursor_row - start) as u16, column as u16));
        let mut out = walker.collected;
        out.reverse();
        out.drain(..start);
        out.truncate(rows);
        return View { rows: out, cursor };
    }
    walker.collect(|_, collected| collected < rows);
    let visible = walker.collected.len().min(rows);
    let mut out: Vec<Vec<Cell>> = walker.collected.drain(..visible).rev().collect();
    out.resize_with(rows, || vec![blank(); cols]);
    let cursor = walker
        .cursor_at
        .filter(|(index, _)| *index < visible)
        .map(|(index, column)| ((visible - 1 - index) as u16, column as u16));
    View { rows: out, cursor }
}

// Walks the grid from the bottom of the screen upwards. `collected` holds
// the bottom row first, `cursor_at` is an index into it.
struct Walker<'a> {
    grid: &'a Grid<AlacrittyCell>,
    cols: usize,
    floor: i32,
    line: i32,
    collected: Vec<Vec<Cell>>,
    cursor_at: Option<(usize, usize)>,
}

impl<'a> Walker<'a> {
    fn new(grid: &'a Grid<AlacrittyCell>, cols: usize) -> Self {
        Self {
            grid,
            cols,
            floor: -(grid.history_size() as i32),
            line: grid.screen_lines() as i32 - 1,
            collected: Vec::new(),
            cursor_at: None,
        }
    }

    fn collect(&mut self, wanted: impl Fn(i32, usize) -> bool) {
        while self.line >= self.floor && wanted(self.line, self.collected.len()) {
            let mut top = self.line;
            while top > self.floor && wraps(&self.grid[Line(top - 1)]) {
                top -= 1;
            }
            let logical = LogicalLine::new(self.grid, top..=self.line);
            let (chunks, chunk_cursor) = logical.wrap(self.cols);
            if let Some((chunk, column)) = chunk_cursor {
                self.cursor_at = Some((self.collected.len() + chunks.len() - 1 - chunk, column));
            }
            self.collected.extend(chunks.into_iter().rev());
            self.line = top - 1;
        }
    }
}

struct LogicalLine {
    cells: Vec<(Cell, Flags)>,
    cursor: Option<usize>,
    occupied: usize,
}

impl LogicalLine {
    fn new(grid: &Grid<AlacrittyCell>, lines: RangeInclusive<i32>) -> Self {
        let point = grid.cursor.point;
        let mut cells = Vec::new();
        let mut cursor = None;
        let mut occupied = 0;
        for line in lines {
            let row = &grid[Line(line)];
            if line == point.line.0 {
                cursor = Some(cells.len() + point.column.0);
            }
            for column in 0..row.len() {
                let cell = &row[Column(column)];
                if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                cells.push((map_cell(cell), cell.flags));
                if !cell.is_empty() {
                    occupied = cells.len();
                }
            }
        }
        Self {
            cells,
            cursor,
            occupied,
        }
    }

    fn wrap(&self, cols: usize) -> (Vec<Vec<Cell>>, Option<(usize, usize)>) {
        let length = self
            .occupied
            .max(self.cursor.map_or(0, |offset| offset + 1))
            .min(self.cells.len());
        let mut chunks = Vec::new();
        let mut cursor = None;
        let mut start = 0;
        loop {
            let mut end = (start + cols).min(length);
            if end < length && end - start > 1 && self.cells[end - 1].1.contains(Flags::WIDE_CHAR) {
                end -= 1;
            }
            let mut row: Vec<Cell> = self.cells[start..end]
                .iter()
                .map(|(cell, _)| cell.clone())
                .collect();
            row.resize_with(cols, blank);
            if let Some(offset) = self.cursor
                && (start..end).contains(&offset)
            {
                cursor = Some((chunks.len(), offset - start));
            }
            chunks.push(row);
            if end >= length {
                break;
            }
            start = end;
        }
        (chunks, cursor)
    }
}

fn wraps(row: &Row<AlacrittyCell>) -> bool {
    row.last()
        .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE))
}

fn blank() -> Cell {
    map_cell(&AlacrittyCell::default())
}
