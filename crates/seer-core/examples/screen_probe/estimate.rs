use seer_core::proto::{ServerMsg, codec};
use seer_core::{Cell, TerminalFrame};
use std::io;

pub(crate) struct Diff {
    pub(crate) changed_cells: usize,
    pub(crate) changed_rows: usize,
    pub(crate) row_diff_bytes: usize,
    pub(crate) cell_diff_bytes: usize,
}

// Two estimates of a change-only update, both built with the same JSON codec
// as today's frames. Row diff: only the rows with a changed cell, each row
// run-length encoded like today, plus a list of row indexes. Cell diff: one
// (row, column, cell) triple per changed cell. Both carry the same header as
// today (user, pane, cursor, modes).
pub(crate) fn diff(
    previous: Option<&TerminalFrame>,
    current: &TerminalFrame,
    user: &str,
    pane: &str,
) -> io::Result<Diff> {
    let same_shape = previous.is_some_and(|last| same_shape(last, current));
    let mut changed_cells = 0;
    let mut changed_rows = Vec::new();
    let mut cells: Vec<(u16, u16, &Cell)> = Vec::new();
    for (row_index, row) in current.rows.iter().enumerate() {
        let last_row = previous
            .filter(|_| same_shape)
            .map(|last| &last.rows[row_index]);
        let mut row_changed = false;
        for (column, cell) in row.iter().enumerate() {
            if last_row.is_some_and(|last| last[column] == *cell) {
                continue;
            }
            row_changed = true;
            changed_cells += 1;
            cells.push((as_u16(row_index), as_u16(column), cell));
        }
        if row_changed {
            changed_rows.push(as_u16(row_index));
        }
    }
    let rows = changed_rows
        .iter()
        .map(|index| current.rows[usize::from(*index)].clone())
        .collect();
    let row_message = message(user, pane, current, rows);
    let row_diff_bytes = encoded_len(&row_message)? + json_len(&changed_rows)?;
    let header = message(user, pane, current, Vec::new());
    let cell_diff_bytes = encoded_len(&header)? + json_len(&cells)?;
    Ok(Diff {
        changed_cells,
        changed_rows: changed_rows.len(),
        row_diff_bytes,
        cell_diff_bytes,
    })
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
