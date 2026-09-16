use seer_core::TerminalFrame;
use seer_core::frame_diff::{self, FrameDiff};
use seer_core::proto::codec;
use std::io;

pub(crate) struct Measure {
    pub(crate) changed_cells: usize,
    pub(crate) shift: i32,
    pub(crate) diff_cells: usize,
    pub(crate) diff_bytes: usize,
    pub(crate) sent_bytes: usize,
    pub(crate) full: bool,
    pub(crate) apply_ok: bool,
}

// The same header as a Cells message, so the bytes match the message that
// a runtime would send for this diff.
#[derive(serde::Serialize)]
enum Message<'a> {
    CellsDiff {
        user: &'a str,
        pane: &'a str,
        diff: &'a FrameDiff,
    },
}

// Runs the scroll diff on the previous and the current frame and encodes it
// like a message. The cap rule from docs/research/23-screen-data.md: the
// full frame is sent when the diff message is not smaller. Every diff is
// applied to the previous frame and compared with the current one, so a
// wrong diff counts as a failure instead of a saving.
pub(crate) fn measure(
    previous: Option<&TerminalFrame>,
    current: &TerminalFrame,
    user: &str,
    pane: &str,
    frame_bytes: usize,
) -> io::Result<Measure> {
    let Some((previous, diff)) = previous
        .and_then(|previous| frame_diff::diff(previous, current).map(|diff| (previous, diff)))
    else {
        return Ok(full_frame(frame_bytes));
    };
    let mut encoded = Vec::new();
    codec::encode(
        &mut encoded,
        &Message::CellsDiff {
            user,
            pane,
            diff: &diff,
        },
    )?;
    let diff_bytes = encoded.len();
    Ok(Measure {
        changed_cells: changed_in_place(previous, current),
        shift: diff.shift,
        diff_cells: diff.cells.len(),
        diff_bytes,
        sent_bytes: diff_bytes.min(frame_bytes),
        full: diff_bytes >= frame_bytes,
        apply_ok: frame_diff::apply(previous, &diff).is_ok_and(|frame| frame == *current),
    })
}

fn full_frame(frame_bytes: usize) -> Measure {
    Measure {
        changed_cells: 0,
        shift: 0,
        diff_cells: 0,
        diff_bytes: 0,
        sent_bytes: frame_bytes,
        full: true,
        apply_ok: true,
    }
}

fn changed_in_place(previous: &TerminalFrame, current: &TerminalFrame) -> usize {
    previous
        .rows
        .iter()
        .zip(&current.rows)
        .map(|(old, new)| old.iter().zip(new).filter(|(a, b)| a != b).count())
        .sum()
}
