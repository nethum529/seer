use std::io;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender};
use std::time::Instant;

use seer_core::TerminalCapabilities;
use seer_core::proto::{ClientMsg, PeekTarget, ServerMsg, codec};

use super::{SharedSession, connection_loop, lock, writer};

pub(super) fn handle_connection(
    mut stream: UnixStream,
    shared: &SharedSession,
    connection_id: u64,
    generation: String,
) -> io::Result<()> {
    codec::encode(&mut stream, &ServerMsg::RuntimeReady { generation })?;
    let Ok(first) = codec::decode(&mut stream) else {
        return Ok(());
    };
    match first {
        ClientMsg::AttachRuntime => shared.add_connection(connection_id, stream.try_clone()?)?,
        ClientMsg::QueryTargets { .. } => {
            return handle_target_query(&mut stream, shared, connection_id);
        }
        ClientMsg::QueryStatus => return super::status::handle_status_query(&mut stream, shared),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected runtime attach",
            ));
        }
    }
    let result = connection_loop(&mut stream, shared, connection_id);
    shared.remove_connection(connection_id)?;
    let _ = stream.shutdown(std::net::Shutdown::Both);
    result
}

// A burst of one poll tick can hold many pane messages; the queue and the deadline must be larger than one tick.
const OUTPUT_QUEUE_CAPACITY: usize = 64;

#[derive(Clone)]
pub(super) struct ReportedViewport {
    pub(super) workspace: String,
    pub(super) tab: String,
    pub(super) cols: u16,
    pub(super) rows: u16,
}

pub(super) struct Connection {
    pub(super) id: u64,
    pub(super) output: SyncSender<Arc<[u8]>>,
    pub(super) stream: UnixStream,
    pub(super) capabilities: Option<TerminalCapabilities>,
    pub(super) viewport: Option<ReportedViewport>,
    pub(super) read_only: bool,
    pub(super) size_owner: bool,
    pub(super) last_active: Instant,
}

impl Connection {
    pub(super) fn new(id: u64, stream: UnixStream) -> io::Result<Self> {
        let writer = stream.try_clone()?;
        let (output, queued) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
        writer::spawn(id, writer, queued)?;
        Ok(Self {
            id,
            output,
            stream,
            capabilities: None,
            viewport: None,
            read_only: false,
            size_owner: false,
            last_active: Instant::now(),
        })
    }

    pub(super) fn send(&self, output: Arc<[u8]>) -> bool {
        self.output.try_send(output).is_ok()
    }

    pub(super) fn send_messages(&self, messages: &[ServerMsg]) -> io::Result<bool> {
        for message in messages {
            if !self.send(writer::encode(message)?) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

pub(super) fn reported_viewport(
    workspace: &str,
    tab: &str,
    cols: u16,
    rows: u16,
) -> ReportedViewport {
    ReportedViewport {
        workspace: workspace.to_owned(),
        tab: tab.to_owned(),
        cols,
        rows,
    }
}

pub(super) fn grant_next_owner(connections: &mut [Connection]) -> Option<ReportedViewport> {
    let position = connections
        .iter()
        .position(|connection| !connection.read_only)?;
    connections[position].size_owner = true;
    connections[position].viewport.clone()
}

pub(super) fn evict_connection(connections: &mut Vec<Connection>, position: usize) -> bool {
    let removed_owner = connections[position].size_owner;
    connections.remove(position);
    removed_owner
}

pub(super) fn handle_target_query(
    stream: &mut UnixStream,
    shared: &SharedSession,
    connection_id: u64,
) -> io::Result<()> {
    let _ = connection_id;
    shared.write_targets(stream)
}

impl SharedSession {
    fn write_targets(&self, stream: &mut UnixStream) -> io::Result<()> {
        codec::encode(
            stream,
            &ServerMsg::Targets {
                targets: self.targets()?,
            },
        )
    }

    pub(super) fn targets(&self) -> io::Result<Vec<PeekTarget>> {
        let active = lock(&self.connections)?
            .iter()
            .find(|connection| connection.size_owner)
            .and_then(|connection| connection.viewport.as_ref())
            .map(|viewport| (viewport.workspace.clone(), viewport.tab.clone()));
        let session = lock(&self.session)?;
        Ok(session.targets(
            active
                .as_ref()
                .map(|(workspace, tab)| (workspace.as_str(), tab.as_str())),
        ))
    }
}
