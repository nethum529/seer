use seer_core::proto::{ServerMsg, codec};
use seer_core::{Cell, Color, TerminalFrame};
use std::io;

pub(crate) struct Diff {
    pub(crate) changed_cells: usize,
    pub(crate) changed_rows: usize,
    pub(crate) row_diff_bytes: usize,
    pub(crate) cell_diff_bytes: usize,
    pub(crate) shift: i32,
    pub(crate) scroll_cells: usize,
    pub(crate) scroll_diff_bytes: usize,
}

// Three estimates of a change-only update, all built with the same JSON
// codec as today's frames. Row diff: only the rows with a changed cell, each
// row run-length encoded like today, plus a list of row indexes. Cell diff:
// one (row, column, cell) triple per changed cell. Scroll diff: the cell
// diff after the previous frame is moved by the number of rows that gives
// the fewest changed cells, plus a moved-by field. All carry the same header
// as today (user, pane, cursor, modes). Every estimate is capped at the
// bytes of today's frame: a real design sends the full frame when the diff
// is larger.
pub(crate) fn diff(
    previous: Option<&TerminalFrame>,
    current: &TerminalFrame,
    user: &str,
    pane: &str,
    frame_bytes: usize,
) -> io::Result<Diff> {
    let same_shape = previous.is_some_and(|last| same_shape(last, current));
    let cells = shifted_changes(previous.filter(|_| same_shape), current, 0);
    let changed_cells = cells.len();
    let mut changed_rows: Vec<u16> = cells.iter().map(|(row, _, _)| *row).collect();
    changed_rows.dedup();
    let rows = changed_rows
        .iter()
        .map(|index| current.rows[usize::from(*index)].clone())
        .collect();
    let row_message = message(user, pane, current, rows);
    let row_diff_bytes = (encoded_len(&row_message)? + json_len(&changed_rows)?).min(frame_bytes);
    let header = encoded_len(&message(user, pane, current, Vec::new()))?;
    let cell_diff_bytes = (header + json_len(&cells)?).min(frame_bytes);
    let (shift, scroll_cells) = previous
        .filter(|_| same_shape)
        .map_or((0, changed_cells), |last| {
            best_shift(last, current, changed_cells)
        });
    let scroll_diff_bytes = if shift == 0 {
        cell_diff_bytes
    } else {
        let moved: Vec<(u16, u16, &Cell)> = shifted_changes(previous, current, shift);
        (header + json_len(&Moved { moved: shift })? + json_len(&moved)?).min(frame_bytes)
    };
    Ok(Diff {
        changed_cells,
        changed_rows: changed_rows.len(),
        row_diff_bytes,
        cell_diff_bytes,
        shift,
        scroll_cells,
        scroll_diff_bytes,
    })
}

#[derive(serde::Serialize)]
struct Moved {
    moved: i32,
}

// A positive shift means the screen scrolled up: current row r was previous
// row r + shift. Row 0 of the search is the unshifted cell diff. A row that
// the shift exposes is compared to blank cells, because a terminal scroll
// fills the exposed rows with blanks.
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

fn best_shift(last: &TerminalFrame, current: &TerminalFrame, unshifted: usize) -> (i32, usize) {
    let rows = i32::try_from(current.rows.len()).unwrap_or(0);
    let mut best = (0, unshifted);
    for shift in (1 - rows..rows).filter(|shift| *shift != 0) {
        let count = shifted_changes(Some(last), current, shift).len();
        if count < best.1 {
            best = (shift, count);
        }
    }
    best
}

fn shifted_changes<'a>(
    previous: Option<&TerminalFrame>,
    current: &'a TerminalFrame,
    shift: i32,
) -> Vec<(u16, u16, &'a Cell)> {
    let mut cells = Vec::new();
    for (row_index, row) in current.rows.iter().enumerate() {
        let source = usize::try_from(i32::try_from(row_index).unwrap_or(0) + shift).ok();
        let last_row = previous.map(|last| source.and_then(|index| last.rows.get(index)));
        for (column, cell) in row.iter().enumerate() {
            let before = last_row.map(|last| last.map_or(&BLANK, |row| &row[column]));
            if before.is_some_and(|before| before == cell) {
                continue;
            }
            cells.push((as_u16(row_index), as_u16(column), cell));
        }
    }
    cells
}

fn same_shape(last: &TerminalFrame, current: &TerminalFrame) -> bool {
    last.rows.len() == current.rows.len()
        && last
            .rows
            .iter()
            .zip(&current.rows)
            .all(|(a, b)| a.len() == b.len())
}

fn message(user: &str, pane: &str, current: &TerminalFrame, rows: Vec<Vec<Cell>>) -> ServerMsg {
    ServerMsg::Cells {
        user: user.into(),
        pane: pane.into(),
        frame: TerminalFrame {
            rows,
            cursor: current.cursor,
            modes: current.modes,
        },
    }
}

fn encoded_len(message: &ServerMsg) -> io::Result<usize> {
    let mut out = Vec::new();
    codec::encode(&mut out, message)?;
    Ok(out.len())
}

fn json_len<T: serde::Serialize>(value: &T) -> io::Result<usize> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(io::Error::other)
}

fn as_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}
