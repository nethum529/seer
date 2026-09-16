//! A scroll diff between two terminal frames of the same shape.
//!
//! The diff is a type, not a message. The caller sends the full frame when
//! the encoded diff message is not smaller than the encoded full frame.
//! That cap needs message bytes, so it lives where messages are built.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Cell, Color, Cursor, TerminalFrame, TerminalModes};

#[cfg(test)]
mod tests;

/// A positive `shift` of N means the screen scrolled up: new row r was old
/// row r + N. A row that the shift exposes starts blank. Then `cells` are
/// set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FrameDiff {
    pub cols: u16,
    pub rows: u16,
    pub shift: i32,
    pub cells: Vec<(u16, u16, Cell)>,
    pub cursor: Cursor,
    pub modes: TerminalModes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplyError {
    ShapeMismatch,
    CellOutOfRange,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ShapeMismatch => "the diff is for a frame of another shape",
            Self::CellOutOfRange => "the diff sets a cell outside the frame",
        })
    }
}

impl std::error::Error for ApplyError {}

const BLANK: Cell = Cell {
    character: ' ',
    fg: Color::Default,
    bg: Color::Default,
    bold: false,
    italic: false,
    underline: false,
    dim: false,
    inverse: false,
    hidden: false,
    strikeout: false,
};

/// `None` when a frame is empty or not rectangular, or the shapes differ.
#[must_use]
pub fn diff(previous: &TerminalFrame, current: &TerminalFrame) -> Option<FrameDiff> {
    let (cols, rows) = shape(previous)?;
    if shape(current)? != (cols, rows) {
        return None;
    }
    let cols = u16::try_from(cols).ok()?;
    let rows = u16::try_from(rows).ok()?;
    let mut shift = 0;
    let mut cells = changes(previous, current, 0);
    // A shift of rows exposes every row, so that diff is the non-blank
    // cells of the new frame: the fast output case of doc 23.
    let mut candidates = vec![best_shift(previous, current, rows), i32::from(rows)];
    candidates.dedup();
    for candidate in candidates.into_iter().filter(|candidate| *candidate != 0) {
        let shifted = changes(previous, current, candidate);
        if shifted.len() < cells.len() {
            shift = candidate;
            cells = shifted;
        }
    }
    Some(FrameDiff {
        cols,
        rows,
        shift,
        cells,
        cursor: current.cursor,
        modes: current.modes,
    })
}

pub fn apply(previous: &TerminalFrame, diff: &FrameDiff) -> Result<TerminalFrame, ApplyError> {
    let (cols, rows) = shape(previous).ok_or(ApplyError::ShapeMismatch)?;
    if (cols, rows) != (usize::from(diff.cols), usize::from(diff.rows)) {
        return Err(ApplyError::ShapeMismatch);
    }
    let mut out: Vec<Vec<Cell>> = (0..rows)
        .map(|row| {
            source_row(rows, row, diff.shift)
                .map_or_else(|| vec![BLANK; cols], |source| previous.rows[source].clone())
        })
        .collect();
    for (row, column, cell) in &diff.cells {
        let slot = out
            .get_mut(usize::from(*row))
            .and_then(|row| row.get_mut(usize::from(*column)))
            .ok_or(ApplyError::CellOutOfRange)?;
        *slot = cell.clone();
    }
    Ok(TerminalFrame {
        rows: out,
        cursor: diff.cursor,
        modes: diff.modes,
    })
}

/// The seq rule of R-411: a diff at `seq` applies only to the screen held at
/// `seq - 1`.
#[must_use]
pub fn apply_next(
    held: &TerminalFrame,
    held_seq: u64,
    seq: u64,
    diff: &FrameDiff,
) -> Option<TerminalFrame> {
    (held_seq.checked_add(1)? == seq).then(|| apply(held, diff).ok())?
}

fn shape(frame: &TerminalFrame) -> Option<(usize, usize)> {
    let cols = frame.rows.first()?.len();
    if cols == 0 || frame.rows.iter().any(|row| row.len() != cols) {
        return None;
    }
    Some((cols, frame.rows.len()))
}

fn source_row(rows: usize, row: usize, shift: i32) -> Option<usize> {
    let source = i64::try_from(row).ok()? + i64::from(shift);
    usize::try_from(source).ok().filter(|source| *source < rows)
}

fn changes(previous: &TerminalFrame, current: &TerminalFrame, shift: i32) -> Vec<(u16, u16, Cell)> {
    let rows = current.rows.len();
    let mut cells = Vec::new();
    for (row, new) in current.rows.iter().enumerate() {
        let old = source_row(rows, row, shift).map(|source| previous.rows[source].as_slice());
        for (column, cell) in new.iter().enumerate() {
            if old.map_or(&BLANK, |old| &old[column]) != cell {
                cells.push((as_u16(row), as_u16(column), cell.clone()));
            }
        }
    }
    cells
}

// Scores every shift by the cells it explains on a column sample, not by
// whole rows: a row that keeps its text but changes one column, like a
// line number column, still scores for the shift that explains its text.
// An exact count for every shift would be rows times cells compares per
// update, 1M at 200x50, so only the winner is counted exactly.
fn best_shift(previous: &TerminalFrame, current: &TerminalFrame, rows: u16) -> i32 {
    let step = previous.rows[0].len().div_ceil(16).max(1);
    let mut best = (0, sampled(previous, current, 0, step));
    for distance in 1..=i32::from(rows) {
        for shift in [distance, -distance] {
            let explained = sampled(previous, current, shift, step);
            if explained > best.1 {
                best = (shift, explained);
            }
        }
    }
    best.0
}

fn sampled(previous: &TerminalFrame, current: &TerminalFrame, shift: i32, step: usize) -> usize {
    let rows = current.rows.len();
    current
        .rows
        .iter()
        .enumerate()
        .map(|(row, new)| {
            let old = source_row(rows, row, shift).map(|source| previous.rows[source].as_slice());
            new.iter()
                .enumerate()
                .step_by(step)
                .filter(|(column, cell)| old.map_or(&BLANK, |old| &old[*column]) == *cell)
                .count()
        })
        .sum()
}

fn as_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}
