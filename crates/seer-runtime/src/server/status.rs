use std::io;
use std::os::unix::net::UnixStream;
use std::time::Instant;

use seer_core::TerminalCapabilities;

use seer_core::proto::{ClientMsg, ServerMsg, codec};

use super::{SharedSession, lock};
use crate::user_session::validate_capabilities;

pub(super) fn handle_status_query(
    stream: &mut UnixStream,
    shared: &SharedSession,
) -> io::Result<()> {
    let status = shared.status()?;
    codec::encode(stream, &status)
}

impl SharedSession {
    pub(super) fn record_capabilities(
        &self,
        connection_id: u64,
        capabilities: TerminalCapabilities,
    ) -> io::Result<bool> {
        if let Err(error) = validate_capabilities(capabilities) {
            self.send_refused(connection_id, error.to_string())?;
            return Ok(false);
        }
        let mut connections = lock(&self.connections)?;
        let Some(connection) = connections.iter_mut().find(|c| c.id == connection_id) else {
            return Ok(true);
        };
        connection.capabilities = Some(capabilities);
        connection.last_active = Instant::now();
        Ok(false)
    }

    pub(super) fn record_input(&self, message: &ClientMsg) {
        if matches!(
            message,
            ClientMsg::TerminalInput { .. }
                | ClientMsg::GrantedInput { .. }
                | ClientMsg::GrantedMouse { .. }
        ) && let Ok(mut last_input) = self.last_input.lock()
        {
            *last_input = Instant::now();
        }
    }

    fn status(&self) -> io::Result<ServerMsg> {
        let active = self
            .targets()?
            .into_iter()
            .find(|target| target.active)
            .map(|target| (target.workspace, target.tab));
        let idle_secs = lock(&self.last_input)?.elapsed().as_secs();
        let windows = lock(&self.connections)?
            .iter()
            .filter(|connection| !connection.read_only)
            .filter_map(|connection| connection.pid)
            .collect();
        let session = lock(&self.session)?;
        let foreground = active.map_or_else(String::new, |(workspace, tab)| {
            session.foreground(&workspace, &tab)
        });
        Ok(ServerMsg::Status {
            tabs: session.tab_count(),
            foreground,
            idle_secs,
            windows: Some(windows),
            shells: Some(u32::try_from(session.pane_hosts.len()).unwrap_or(u32::MAX)),
        })
    }
}
