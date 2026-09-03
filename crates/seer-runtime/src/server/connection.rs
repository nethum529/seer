use std::io;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender};
use std::time::Instant;

use seer_core::proto::ServerMsg;

use super::writer;

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
