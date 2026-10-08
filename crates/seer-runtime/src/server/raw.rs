use std::sync::Arc;

use super::connection::Connection;
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
        if self.raw_pane.as_deref() == Some(pane) {
            return;
        }
        self.unfollow_raw(session);
        if let Some(host) = session.pane_hosts.get(pane) {
            host.attach_raw(Arc::clone(&self.outbox));
            self.raw_pane = Some(pane.to_owned());
        }
    }

    pub(super) fn unfollow_raw(&mut self, session: &UserSession) {
        if let Some(pane) = self.raw_pane.take()
            && let Some(host) = session.pane_hosts.get(&pane)
        {
            host.detach_raw(&self.outbox);
        }
    }
}

// Some connection is open, and some connection needs cell frames.
pub(super) fn watchers(connections: &[Connection]) -> (bool, bool) {
    (
        !connections.is_empty(),
        connections.iter().any(|connection| !connection.raw()),
    )
}
