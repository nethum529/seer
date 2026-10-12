use seer_core::TerminalFrame;
use seer_core::frame_diff::{FrameDiff, apply_next};

use super::{Shown, Terminal, lock};

pub(crate) fn replace(terminal: &Terminal, frame: TerminalFrame, seq: u64) {
    let mut shown = lock(terminal);
    // Raw output can arrive before the main thread handles a queued cell frame.
    if !matches!(*shown, Some(Shown::Raw(_))) {
        *shown = Some(Shown::Frame {
            frame,
            seq,
            fresh: true,
        });
    }
}

pub(crate) fn apply(terminal: &Terminal, seq: u64, diff: &FrameDiff) -> bool {
    let mut shown = lock(terminal);
    let next = match &*shown {
        Some(Shown::Frame {
            frame, seq: held, ..
        }) => apply_next(frame, *held, seq, diff),
        Some(Shown::Raw(_)) => return true,
        None => None,
    };
    if let Some(frame) = next {
        *shown = Some(Shown::Frame {
            frame,
            seq,
            fresh: true,
        });
        return true;
    }
    false
}
