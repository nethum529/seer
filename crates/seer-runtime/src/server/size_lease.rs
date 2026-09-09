use std::collections::BTreeMap;
use std::io;
use std::time::Instant;

use seer_core::proto::ClientMsg;
use seer_core::{InputEvent, MouseKind, PaneSize};

use super::{SharedSession, connection::Connection, lock};
use crate::user_session::VisibleSize;

impl SharedSession {
    pub(super) fn watch_size(
        &self,
        id: u64,
        pane: &str,
        size: Option<PaneSize>,
    ) -> io::Result<bool> {
        let lease = lock(&self.lease)?;
        let valid = lock(&self.session)?.pane_hosts.contains_key(pane)
            && size.is_none_or(|size| size.cols > 0 && size.rows > 0);
        if !valid {
            drop(lease);
            self.send_refused(id, "terminal or size is invalid".into())?;
            return Ok(false);
        }
        {
            let mut connections = lock(&self.connections)?;
            let Some(connection) = connections
                .iter_mut()
                .find(|connection| connection.id == id)
            else {
                return Ok(true);
            };
            match size {
                Some(size) => {
                    let reported = connection.watches.insert(pane.into(), size);
                    connection.watch_started = true;
                    connection.watch_ended = false;
                    if reported != Some(size) {
                        connection.claimed.insert(pane.into(), Instant::now());
                    }
                }
                None => {
                    connection.watches.remove(pane);
                    connection.claimed.remove(pane);
                    if connection.watches.is_empty() {
                        connection.watch_started = false;
                        connection.watch_ended = false;
                    }
                }
            }
        }
        self.flush_messages(&[])?;
        Ok(false)
    }

    pub(super) fn claim_pane(&self, id: u64, pane: &str) -> io::Result<bool> {
        let mut connections = lock(&self.connections)?;
        let before = own_sizes(&connections);
        let Some(connection) = connections
            .iter_mut()
            .find(|connection| connection.id == id && !connection.read_only)
        else {
            return Ok(false);
        };
        connection.claimed.insert(pane.into(), Instant::now());
        Ok(own_sizes(&connections) != before)
    }

    pub(super) fn visible_sizes(connections: &[Connection]) -> BTreeMap<String, VisibleSize> {
        let claimed = own_sizes(connections);
        let mut sizes: BTreeMap<String, VisibleSize> = BTreeMap::new();
        for connection in connections.iter().filter(|connection| connection.read_only) {
            for (pane, size) in &connection.watches {
                if claimed.contains_key(pane) {
                    continue;
                }
                sizes
                    .entry(pane.clone())
                    .and_modify(|smallest| {
                        smallest.size.cols = smallest.size.cols.min(size.cols);
                        smallest.size.rows = smallest.size.rows.min(size.rows);
                    })
                    .or_insert(VisibleSize {
                        size: *size,
                        own: false,
                    });
            }
        }
        sizes.extend(
            claimed
                .into_iter()
                .map(|(pane, size)| (pane, VisibleSize { size, own: true })),
        );
        sizes
    }
}

pub(super) fn claimed_pane(message: &ClientMsg) -> Option<&str> {
    match message {
        ClientMsg::TerminalInput { pane, input, .. } => {
            claims_size(&input.event).then_some(pane.as_str())
        }
        ClientMsg::GrantedInput { pane, .. } | ClientMsg::FocusPane { pane, .. } => Some(pane),
        _ => None,
    }
}

// Losing focus, pointer motion and scrollback happen without the person acting on the terminal.
fn claims_size(event: &InputEvent) -> bool {
    match event {
        InputEvent::Focus(gained) => *gained,
        InputEvent::Mouse(mouse) => mouse.kind != MouseKind::Moved,
        InputEvent::Scrollback { .. } => false,
        InputEvent::Key(_) | InputEvent::Text(_) | InputEvent::Paste(_) => true,
    }
}

fn own_sizes(connections: &[Connection]) -> BTreeMap<String, PaneSize> {
    let mut claimed: BTreeMap<String, (Instant, PaneSize)> = BTreeMap::new();
    for connection in connections
        .iter()
        .filter(|connection| !connection.read_only)
    {
        for (pane, size) in &connection.watches {
            let Some(at) = connection.claimed.get(pane).copied() else {
                continue;
            };
            claimed
                .entry(pane.clone())
                .and_modify(|current| {
                    if at > current.0 {
                        *current = (at, *size);
                    }
                })
                .or_insert((at, *size));
        }
    }
    claimed
        .into_iter()
        .map(|(pane, (_, size))| (pane, size))
        .collect()
}
