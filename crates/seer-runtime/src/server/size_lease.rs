use std::collections::BTreeMap;
use std::io;
use std::os::unix::net::UnixStream;
use std::time::Instant;

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, MouseKind, PaneSize};

use super::{SharedSession, connection::Connection, lock, writer};

impl SharedSession {
    pub(super) fn watch_size(
        &self,
        id: u64,
        pane: &str,
        size: Option<PaneSize>,
    ) -> io::Result<bool> {
        let lease = lock(&self.lease)?;
        let (valid, current) = {
            let session = lock(&self.session)?;
            let valid = session.pane_hosts.contains_key(pane)
                && size.is_none_or(|size| size.cols > 0 && size.rows > 0);
            let current = session.pane_hosts.get(pane).map(|host| ServerMsg::Cells {
                user: session.user.clone(),
                pane: pane.to_owned(),
                frame: host.frame(),
            });
            (valid, current)
        };
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
                    // A new watcher must see the screen as it stands. A quiet
                    // terminal produces nothing to poll, so send it here.
                    if let Some(current) = &current {
                        connection.send(writer::encode(current)?);
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
}

pub(super) fn claimed_pane(message: &ClientMsg) -> Option<&str> {
    match message {
        ClientMsg::TerminalInput { pane, input, .. } => {
            claims_size(&input.event).then_some(pane.as_str())
        }
        ClientMsg::GrantedInput { pane, .. }
        | ClientMsg::GrantedMouse { pane, .. }
        | ClientMsg::FocusPane { pane, .. } => Some(pane),
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

pub(super) fn own_sizes(connections: &[Connection]) -> BTreeMap<String, PaneSize> {
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

// Nested terminal servers may not inherit Seer markers. Without a pane,
// only a single local client can be selected.
pub(super) fn handle_exit_client(
    stream: &mut UnixStream,
    shared: &SharedSession,
    pane: &str,
) -> io::Result<()> {
    let bye = ServerMsg::Bye {
        reason: "detached".into(),
    };
    let reply = match shared.exit_target(pane)? {
        Some(id) if shared.send_to(id, &bye).is_ok() => bye,
        Some(_) => refused("the Seer client has already left"),
        None => refused("cannot select one Seer client; run seer exit in the outer Seer shell"),
    };
    codec::encode(stream, &reply)
}

fn refused(reason: &str) -> ServerMsg {
    ServerMsg::Refused {
        reason: reason.into(),
    }
}

impl SharedSession {
    fn exit_target(&self, pane: &str) -> io::Result<Option<u64>> {
        if !pane.is_empty() && !lock(&self.session)?.pane_hosts.contains_key(pane) {
            return Ok(None);
        }
        let connections = lock(&self.connections)?;
        let local: Vec<&Connection> = connections
            .iter()
            .filter(|connection| !connection.read_only)
            .collect();
        let claimed = local
            .iter()
            .filter_map(|connection| connection.claimed.get(pane).map(|at| (*at, connection.id)))
            .max_by_key(|(at, _)| *at)
            .map(|(_, id)| id);
        Ok(claimed.or(match local.as_slice() {
            [only] => Some(only.id),
            _ => None,
        }))
    }
}
