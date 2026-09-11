use seer_core::TerminalCapabilities;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::UserSession;
use crate::user_session::validate_capabilities;

mod connection;
mod room_link;
mod size_lease;
mod status;
mod util;
mod writer;

use connection::{
    Connection, ReportedViewport, evict_connection, grant_next_owner, reported_viewport,
};
use room_link::{spawn_connection, start_room};
use util::{connection_closed, lock, remove_stale_socket, stop_after_snapshot_failure};

const POLL_INTERVAL: Duration = Duration::from_millis(5);
const DETACHED_POLL_INTERVAL: Duration = Duration::from_millis(100);
const SIZE_LEASE_TIMEOUT: Duration = Duration::from_secs(30);

pub fn bind(path: &Path) -> io::Result<UnixListener> {
    remove_stale_socket(path)?;
    UnixListener::bind(path)
}

pub fn serve(listener: UnixListener, session: UserSession) -> io::Result<()> {
    serve_with_generation(listener, session, String::new(), None)
}

pub(crate) fn serve_with_generation(
    listener: UnixListener,
    session: UserSession,
    generation: String,
    room: Option<crate::room::RoomConfig>,
) -> io::Result<()> {
    let shared = Arc::new(SharedSession::new(session));
    start_poll_driver(Arc::clone(&shared))?;
    let ids = Arc::new(AtomicU64::new(0));
    if let Some(room) = room {
        start_room(room, &generation, &shared, &ids)?;
    }
    loop {
        let (stream, _) = listener.accept()?;
        spawn_connection(stream, &shared, &ids, &generation, false)?;
    }
}

fn connection_loop(
    stream: &mut UnixStream,
    shared: &SharedSession,
    connection_id: u64,
) -> io::Result<()> {
    loop {
        let Ok(message) = codec::decode(stream) else {
            return Ok(());
        };
        if handle_message(shared, connection_id, message)? {
            return Ok(());
        }
    }
}

fn handle_message(
    shared: &SharedSession,
    connection_id: u64,
    message: ClientMsg,
) -> io::Result<bool> {
    seer_core::debug_log!(
        "recv conn={connection_id} {}",
        seer_core::debug_log::client_summary(&message)
    );
    match message {
        ClientMsg::Detach => Ok(true),
        ClientMsg::Watch {
            pane, cols, rows, ..
        } => shared.watch_size(
            connection_id,
            &pane,
            Some(seer_core::PaneSize { cols, rows }),
        ),
        ClientMsg::Unwatch { pane, .. } => shared.watch_size(connection_id, &pane, None),
        ClientMsg::Resize {
            workspace,
            tab,
            cols,
            rows,
        } => shared.client_resize(connection_id, &workspace, &tab, cols, rows),
        ClientMsg::TerminalCapabilities { capabilities } => {
            shared.record_capabilities(connection_id, capabilities)
        }
        message if message.is_mutating() || matches!(message, ClientMsg::GrantedInput { .. }) => {
            shared.record_input(&message);
            shared.dispatch_input(connection_id, message)
        }
        _ => Ok(false),
    }
}

fn start_poll_driver(shared: Arc<SharedSession>) -> io::Result<()> {
    thread::Builder::new()
        .name("runtime-poll".into())
        .spawn(move || poll_driver(&shared))?;
    Ok(())
}

fn poll_driver(shared: &SharedSession) {
    loop {
        if let Err(error) = shared
            .poll_and_broadcast()
            .and_then(|_| shared.wait_for_poll())
        {
            eprintln!("runtime poll error: {error}");
            return;
        }
    }
}

struct SharedSession {
    session: Mutex<UserSession>,
    poll_wake: Condvar,
    connections: Mutex<Vec<Connection>>,
    lease: Mutex<()>,
    last_input: Mutex<Instant>,
}

impl SharedSession {
    fn new(session: UserSession) -> Self {
        Self {
            session: Mutex::new(session),
            poll_wake: Condvar::new(),
            connections: Mutex::new(Vec::new()),
            lease: Mutex::new(()),
            last_input: Mutex::new(Instant::now()),
        }
    }

    fn add_connection(&self, id: u64, stream: UnixStream) -> io::Result<()> {
        let _lease = lock(&self.lease)?;
        let messages = {
            let mut session = lock(&self.session)?;
            match session.ensure_first_shell() {
                Ok(()) => session.snapshot(),
                Err(error) if crate::persistence::is_fatal(&error) => {
                    stop_after_snapshot_failure(&error)
                }
                Err(error) => return Err(error),
            }
        };
        let mut connection = Connection::new(id, stream)?;
        if !connection.send_messages(&messages)? {
            return Err(connection_closed());
        }
        let mut connections = lock(&self.connections)?;
        let owner_stale = connections
            .iter()
            .any(|c| c.size_owner && c.last_active + SIZE_LEASE_TIMEOUT <= Instant::now());
        if owner_stale {
            for owner in connections.iter_mut() {
                owner.size_owner = false;
            }
            connection.size_owner = true;
        } else {
            connection.size_owner = !connections.iter().any(|c| c.size_owner);
        }
        seer_core::debug_log!(
            "attach conn={id} size_owner={} connections={}",
            connection.size_owner,
            connections.len() + 1
        );
        connections.push(connection);
        self.poll_wake.notify_one();
        Ok(())
    }

    fn remove_connection(&self, id: u64) -> io::Result<()> {
        let _lease = lock(&self.lease)?;
        let owner_removed = {
            let mut connections = lock(&self.connections)?;
            let removed_owner = connections.iter().any(|c| c.id == id && c.size_owner);
            connections.retain(|connection| connection.id != id);
            seer_core::debug_log!(
                "detach conn={id} was_size_owner={removed_owner} connections={}",
                connections.len()
            );
            removed_owner
        };
        if owner_removed {
            self.recover_locked()?;
        }
        self.flush_messages(&[])
    }

    fn recover(&self) -> io::Result<()> {
        let _lease = lock(&self.lease)?;
        self.recover_locked()
    }

    fn recover_locked(&self) -> io::Result<()> {
        let adopt = {
            let mut connections = lock(&self.connections)?;
            if connections.iter().any(|connection| connection.size_owner) {
                None
            } else {
                grant_next_owner(&mut connections)
            }
        };
        if let Some(viewport) = adopt {
            self.adopt_locked(viewport)?;
        }
        Ok(())
    }

    fn adopt_locked(&self, viewport: ReportedViewport) -> io::Result<()> {
        let messages = match self.record_viewport(
            &viewport.workspace,
            &viewport.tab,
            viewport.cols,
            viewport.rows,
        ) {
            Ok(messages) => messages,
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => return Ok(()),
            Err(error) => return Err(error),
        };
        self.flush_messages(&messages)
    }

    fn record_viewport(
        &self,
        workspace: &str,
        tab: &str,
        cols: u16,
        rows: u16,
    ) -> io::Result<Vec<ServerMsg>> {
        let sizes = Self::visible_sizes(&lock(&self.connections)?);
        let mut session = lock(&self.session)?;
        match session.record_viewport(workspace, tab, cols, rows, &sizes) {
            Ok(messages) => Ok(messages),
            Err(error) if crate::persistence::is_fatal(&error) => {
                stop_after_snapshot_failure(&error)
            }
            Err(error) => Err(error),
        }
    }

    fn refresh_read_only(&self, id: u64) -> io::Result<Option<bool>> {
        let mut connections = lock(&self.connections)?;
        let Some(connection) = connections.iter_mut().find(|c| c.id == id) else {
            return Ok(None);
        };
        connection.last_active = Instant::now();
        Ok(Some(connection.read_only))
    }

    fn dispatch_input(&self, connection_id: u64, message: ClientMsg) -> io::Result<bool> {
        let _lease = lock(&self.lease)?;
        let Some(read_only) = self.refresh_read_only(connection_id)? else {
            return Ok(true);
        };
        if read_only && !matches!(message, ClientMsg::GrantedInput { .. }) {
            seer_core::debug_log!("input dropped conn={connection_id} reason=read-only");
            eprintln!("runtime dropped read-only message: {message:?}");
            return Ok(false);
        }
        // Arbitrate before the input reaches the PTY so the first keystroke or click sees the right viewport.
        if let Some(pane) = size_lease::claimed_pane(&message)
            && self.claim_pane(connection_id, pane)?
        {
            self.flush_messages(&[])?;
        }
        let applied = self.apply(message);
        match applied {
            Ok(messages) => {
                seer_core::debug_log!("input forwarded conn={connection_id}");
                self.flush_messages(&messages)?;
                Ok(false)
            }
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
                seer_core::debug_log!("input refused conn={connection_id} reason={error}");
                drop(_lease);
                self.send_refused(connection_id, error.to_string())?;
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    fn record_capabilities(
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

    fn client_resize(
        &self,
        id: u64,
        workspace: &str,
        tab: &str,
        cols: u16,
        rows: u16,
    ) -> io::Result<bool> {
        let _lease = lock(&self.lease)?;
        let pane_rects = lock(&self.session)?.pane_rects(workspace, tab, cols, rows);
        {
            let mut connections = lock(&self.connections)?;
            let Some(position) = connections.iter().position(|c| c.id == id) else {
                return Ok(false);
            };
            if connections[position].read_only {
                return Ok(false);
            }
            connections[position].last_active = Instant::now();
            let now = Instant::now();
            // Watch carries the real content geometry, so a resize only reclaims the panes it already watches.
            for (pane, _) in &pane_rects {
                if connections[position].watches.contains_key(pane) {
                    connections[position].claimed.insert(pane.clone(), now);
                }
            }
            let lease_vacant = !connections.iter().any(|c| c.size_owner);
            let owner_stale = connections
                .iter()
                .any(|c| c.size_owner && c.last_active + SIZE_LEASE_TIMEOUT <= Instant::now());
            if !connections[position].size_owner && !lease_vacant && !owner_stale {
                seer_core::debug_log!("resize conn={id} size={cols}x{rows} deferred=not-owner");
                connections[position].viewport =
                    Some(reported_viewport(workspace, tab, cols, rows));
                drop(connections);
                self.flush_messages(&[])?;
                return Ok(false);
            }
        }
        seer_core::debug_log!("resize conn={id} size={cols}x{rows} applied=owner");
        let applied = self.record_viewport(workspace, tab, cols, rows);
        match applied {
            Ok(messages) => {
                let mut connections = lock(&self.connections)?;
                for connection in connections.iter_mut() {
                    connection.size_owner = connection.id == id;
                }
                if let Some(connection) = connections.iter_mut().find(|c| c.id == id) {
                    connection.viewport = Some(reported_viewport(workspace, tab, cols, rows));
                }
                drop(connections);
                self.flush_messages(&messages)?;
                Ok(false)
            }
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
                drop(_lease);
                self.send_refused(id, error.to_string())?;
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    fn send_refused(&self, id: u64, reason: String) -> io::Result<()> {
        self.send_to(id, &ServerMsg::Refused { reason })
    }

    fn send_to(&self, id: u64, message: &ServerMsg) -> io::Result<()> {
        let output = writer::encode(message)?;
        let owner_lost = {
            let mut connections = lock(&self.connections)?;
            let Some(position) = connections
                .iter()
                .position(|connection| connection.id == id)
            else {
                return Err(connection_closed());
            };
            if connections[position].send(output) {
                return Ok(());
            }
            evict_connection(&mut connections, position)
        };
        if owner_lost {
            self.recover()?;
        }
        Err(connection_closed())
    }

    #[cfg(test)]
    fn apply_and_broadcast(&self, message: ClientMsg) -> io::Result<()> {
        let messages = self.apply(message)?;
        self.broadcast(&messages)
    }

    fn apply(&self, message: ClientMsg) -> io::Result<Vec<ServerMsg>> {
        let mut session = lock(&self.session)?;
        match session.apply(message) {
            Ok(messages) => Ok(messages),
            Err(error) if crate::persistence::is_fatal(&error) => {
                stop_after_snapshot_failure(&error)
            }
            Err(error) => Err(error),
        }
    }

    fn wait_for_poll(&self) -> io::Result<()> {
        let connections = lock(&self.connections)?;
        let interval = if connections.is_empty() {
            DETACHED_POLL_INTERVAL
        } else {
            POLL_INTERVAL
        };
        let _wait = self
            .poll_wake
            .wait_timeout(connections, interval)
            .map_err(|_| io::Error::other("runtime poll lock is poisoned"))?;
        Ok(())
    }

    fn poll_and_broadcast(&self) -> io::Result<(Vec<ServerMsg>, bool)> {
        let _lease = lock(&self.lease)?;
        let messages = lock(&self.session)?.poll();
        let has_connections = !lock(&self.connections)?.is_empty();
        if has_connections {
            self.flush_messages(&messages)?;
        }
        Ok((messages, has_connections))
    }

    #[cfg(test)]
    fn broadcast(&self, messages: &[ServerMsg]) -> io::Result<()> {
        let _lease = lock(&self.lease)?;
        self.flush_messages(messages)
    }

    fn flush_messages(&self, messages: &[ServerMsg]) -> io::Result<()> {
        let adopt = {
            let mut session = lock(&self.session)?;
            let mut connections = lock(&self.connections)?;
            let owner_present = connections.iter().any(|connection| connection.size_owner);
            let sizes = Self::visible_sizes(&connections);
            let mut all_messages = messages.to_vec();
            all_messages.extend(session.apply_visible_sizes(&sizes)?);
            let encoded = all_messages
                .iter()
                .map(writer::encode)
                .collect::<io::Result<Vec<_>>>()?;
            let bye = writer::encode(&ServerMsg::Bye {
                reason: "peek target closed".into(),
            })?;
            let mut position = 0;
            while position < connections.len() {
                if connections[position].send_projection(&session, &all_messages, &encoded, &bye)? {
                    position += 1;
                } else {
                    evict_connection(&mut connections, position);
                }
            }
            if owner_present && !connections.iter().any(|connection| connection.size_owner) {
                grant_next_owner(&mut connections)
            } else {
                None
            }
        };
        if let Some(viewport) = adopt {
            self.adopt_locked(viewport)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn connection_count(&self) -> usize {
        self.connections.lock().map(|c| c.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
