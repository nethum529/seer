use seer_core::TerminalFrame;
use seer_core::frame_diff::{self, FrameDiff};
use seer_core::proto::{ServerMsg, cells_floor, codec};
use std::io;

pub(crate) struct Measure {
    pub(crate) changed_cells: usize,
    pub(crate) shift: i32,
    pub(crate) diff_cells: usize,
    pub(crate) diff_bytes: usize,
    pub(crate) sent_bytes: usize,
    pub(crate) full: bool,
    pub(crate) apply_ok: bool,
    pub(crate) full_bytes: usize,
    pub(crate) floor_ok: bool,
}

pub(crate) struct Header<'a> {
    pub(crate) user: &'a str,
    pub(crate) pane: &'a str,
    pub(crate) seq: u64,
}

// A whole screen arrived. The scroll diff against the previous frame shows
// what a diff would have cost, a check on the cap rule of the runtime.
// Every diff is applied to the previous frame and compared with the current
// one, so a wrong diff counts as a failure instead of a saving.
pub(crate) fn whole(
    previous: Option<&TerminalFrame>,
    current: &TerminalFrame,
    header: &Header<'_>,
    wire: usize,
) -> io::Result<Measure> {
    let floor_ok = cells_floor(header.user, header.pane, header.seq, current)? <= wire;
    let Some((previous, diff)) = previous
        .and_then(|previous| frame_diff::diff(previous, current).map(|diff| (previous, diff)))
    else {
        return Ok(Measure {
            changed_cells: 0,
            shift: 0,
            diff_cells: 0,
            diff_bytes: 0,
            sent_bytes: wire,
            full: true,
            apply_ok: true,
            full_bytes: wire,
            floor_ok,
        });
    };
    let apply_ok = frame_diff::apply(previous, &diff).is_ok_and(|frame| frame == *current);
    let changed_cells = changed_in_place(previous, current);
    let (shift, diff_cells) = (diff.shift, diff.cells.len());
    let diff_bytes = encoded_len(&ServerMsg::CellsDiff {
        user: header.user.to_owned(),
        pane: header.pane.to_owned(),
        seq: header.seq,
        diff,
    })?;
    Ok(Measure {
        changed_cells,
        shift,
        diff_cells,
        diff_bytes,
        sent_bytes: wire,
        full: true,
        apply_ok,
        full_bytes: wire,
        floor_ok,
    })
}

// A diff arrived. The frame it gives is returned, or None when it does not
// apply to the held one at the number before it.
pub(crate) fn applied(
    previous: Option<(&TerminalFrame, u64)>,
    diff: &FrameDiff,
    header: &Header<'_>,
    wire: usize,
) -> io::Result<(Measure, Option<TerminalFrame>)> {
    let current = previous.and_then(|(previous, held_seq)| {
        frame_diff::apply_next(previous, held_seq, header.seq, diff)
    });
    let mut measure = Measure {
        changed_cells: 0,
        shift: diff.shift,
        diff_cells: diff.cells.len(),
        diff_bytes: wire,
        sent_bytes: wire,
        full: false,
        apply_ok: current.is_some(),
        full_bytes: 0,
        floor_ok: true,
    };
    if let (Some((previous, _)), Some(current)) = (previous, current.as_ref()) {
        measure.changed_cells = changed_in_place(previous, current);
        measure.full_bytes = encoded_len(&ServerMsg::Cells {
            user: header.user.to_owned(),
            pane: header.pane.to_owned(),
            frame: current.clone(),
            seq: header.seq,
        })?;
        measure.floor_ok =
            cells_floor(header.user, header.pane, header.seq, current)? <= measure.full_bytes;
    }
    Ok((measure, current))
}

fn encoded_len(message: &ServerMsg) -> io::Result<usize> {
    let mut encoded = Vec::new();
    codec::encode(&mut encoded, message)?;
    Ok(encoded.len())
}

fn changed_in_place(previous: &TerminalFrame, current: &TerminalFrame) -> usize {
    previous
        .rows
        .iter()
        .zip(&current.rows)
        .map(|(old, new)| old.iter().zip(new).filter(|(a, b)| a != b).count())
        .sum()
}
