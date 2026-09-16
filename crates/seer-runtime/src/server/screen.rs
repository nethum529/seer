use std::io;
use std::sync::Arc;

use seer_core::TerminalFrame;
use seer_core::frame_diff::{self, FrameDiff};
use seer_core::proto::{ServerMsg, cells_floor};

use super::connection::Connection;
use super::writer;
use crate::UserSession;

impl Connection {
    pub(super) fn whole(
        &mut self,
        session: &UserSession,
        pane: &str,
    ) -> io::Result<Option<Arc<[u8]>>> {
        let Some(host) = session.pane_hosts.get(pane) else {
            return Ok(None);
        };
        let frame = self
            .watches
            .get(pane)
            .and_then(|size| host.view(*size))
            .unwrap_or_else(|| host.frame());
        let seq = self.next_seq(pane);
        if seq > 0 {
            self.baselines.insert(pane.to_owned(), frame.clone());
        }
        cells(session, pane, seq, frame).map(Some)
    }

    pub(super) fn update(
        &mut self,
        session: &UserSession,
        message: &ServerMsg,
        shared: &Arc<[u8]>,
    ) -> io::Result<Arc<[u8]>> {
        let ServerMsg::Cells { pane, frame, .. } = message else {
            return Ok(Arc::clone(shared));
        };
        let size = self.watches.get(pane);
        let view = size.and_then(|size| session.pane_hosts.get(pane)?.view(*size));
        if !(self.read_only && size.is_some()) {
            return match view {
                Some(view) => cells(session, pane, 0, view),
                None => Ok(Arc::clone(shared)),
            };
        }
        let seq = self.next_seq(pane);
        let at_pane_size = view.is_none();
        let current = view.unwrap_or_else(|| frame.clone());
        let diff = self
            .baselines
            .remove(pane)
            .filter(|baseline| baseline.modes.alt_screen == current.modes.alt_screen)
            .and_then(|baseline| frame_diff::diff(&baseline, &current));
        let output = match diff {
            Some(diff) => capped(
                session,
                pane,
                seq,
                &current,
                diff,
                at_pane_size.then_some(shared),
            )?,
            None => cells(session, pane, seq, current.clone())?,
        };
        self.baselines.insert(pane.clone(), current);
        Ok(output)
    }

    fn next_seq(&mut self, pane: &str) -> u64 {
        if !(self.read_only && self.watches.contains_key(pane)) {
            return 0;
        }
        let seq = self.seqs.entry(pane.to_owned()).or_insert(0);
        *seq += 1;
        *seq
    }
}

// The diff goes out only when it is smaller than the whole screen. At the
// pane size the whole screen is the shared encoding plus the seq digits.
// At a viewer size the floor of a Cells of this shape decides first, and
// the view is encoded only when the diff might lose.
fn capped(
    session: &UserSession,
    pane: &str,
    seq: u64,
    current: &TerminalFrame,
    diff: FrameDiff,
    shared: Option<&Arc<[u8]>>,
) -> io::Result<Arc<[u8]>> {
    let Ok(encoded) = writer::encode(&ServerMsg::CellsDiff {
        user: session.user.clone(),
        pane: pane.to_owned(),
        seq,
        diff,
    }) else {
        return cells(session, pane, seq, current.clone());
    };
    let bound = match shared {
        Some(shared) => shared.len() + seq.to_string().len() - 1,
        None => cells_floor(&session.user, pane, seq, current)?,
    };
    if encoded.len() < bound {
        return Ok(encoded);
    }
    let full = cells(session, pane, seq, current.clone())?;
    Ok(if encoded.len() < full.len() {
        encoded
    } else {
        full
    })
}

fn cells(
    session: &UserSession,
    pane: &str,
    seq: u64,
    frame: TerminalFrame,
) -> io::Result<Arc<[u8]>> {
    writer::encode(&ServerMsg::Cells {
        user: session.user.clone(),
        pane: pane.to_owned(),
        frame,
        seq,
    })
}
