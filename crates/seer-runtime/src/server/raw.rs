use std::io;
use std::sync::Arc;

use super::connection::Connection;
use super::{SharedSession, lock};
use crate::UserSession;

impl Connection {
    /// One of the owner's own windows that parses the PTY bytes itself.
    pub(super) fn raw(&self) -> bool {
        !self.read_only
            && self
                .capabilities
                .is_some_and(|capabilities| capabilities.raw_output)
    }

    // The window sends Watch again on each resize. A second attach would
    // send the whole scrollback again, so the pane it follows stays.
    pub(super) fn follow_raw(&mut self, session: &UserSession, pane: &str) {
        if self.raw_pane() == Some(pane) {
            return;
        }
        self.unfollow_raw(session);
        if let Some(host) = session.pane_hosts.get(pane) {
            let output = host.attach_raw(Arc::clone(&self.outbox));
            self.raw_pane = Some((pane.to_owned(), output));
        }
    }

    pub(super) fn unfollow_raw(&mut self, session: &UserSession) {
        if let Some((pane, _)) = self.raw_pane.take()
            && let Some(host) = session.pane_hosts.get(&pane)
        {
            host.detach_raw(&self.outbox);
        }
    }

    pub(super) fn raw_pane(&self) -> Option<&str> {
        self.raw_pane.as_ref().map(|(pane, _)| pane.as_str())
    }
}

impl SharedSession {
    pub(super) fn parsed(&self, id: u64, mark: u64) -> io::Result<bool> {
        let followed = lock(&self.connections)?
            .iter()
            .find(|connection| connection.id == id)
            .and_then(|connection| {
                let (_, output) = connection.raw_pane.as_ref()?;
                Some((output.clone(), Arc::clone(&connection.outbox)))
            });
        if let Some((output, outbox)) = followed {
            output.parsed(&outbox, mark);
        }
        Ok(false)
    }
}

// Some connection is open, and some connection needs cell frames.
pub(super) fn watchers(connections: &[Connection]) -> (bool, bool) {
    (
        !connections.is_empty(),
        connections.iter().any(|connection| !connection.raw()),
    )
}
