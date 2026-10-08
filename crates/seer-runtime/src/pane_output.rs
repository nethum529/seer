use seer_core::proto::codec::RawEvent;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::PaneGrid;
use crate::outbox::Outbox;
use crate::pty::lock_mutex;

// The same deadline as a slow connection write (server/writer.rs).
const RAW_WAIT: Duration = Duration::from_secs(2);
const RESTORE_CHUNK: usize = 1024 * 1024;

/// The grid of one pane and the connections that get its raw output. The
/// PTY reader feeds the grid and the connections under one lock, so a
/// connection sees the bytes and the size changes in the order the grid
/// saw them.
pub(crate) struct PaneOutput {
    pub(crate) grid: PaneGrid,
    sinks: Vec<Arc<Outbox>>,
    changed: bool,
}

pub(crate) type SharedOutput = Arc<Mutex<PaneOutput>>;

impl PaneOutput {
    pub(crate) fn shared(grid: PaneGrid) -> SharedOutput {
        Arc::new(Mutex::new(Self {
            grid,
            sinks: Vec::new(),
            changed: false,
        }))
    }

    /// Ends a synchronized update after its timeout. Gives whether the
    /// screen changed since the last call, and the answers to write back.
    pub(crate) fn poll(&mut self) -> (bool, Vec<u8>) {
        if self.grid.end_expired_sync() {
            self.send(RawEvent::SyncEnd);
            self.changed = true;
        }
        self.sinks.retain(|sink| !sink.is_closed());
        (std::mem::take(&mut self.changed), self.grid.take_replies())
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        self.grid.resize(cols, rows);
        self.send(RawEvent::Size { cols, rows });
    }

    /// Sends the sink the whole state first, then every later byte. Gives
    /// the answers to write back.
    pub(crate) fn attach(&mut self, sink: Arc<Outbox>) -> Vec<u8> {
        if self.grid.end_sync() {
            self.send(RawEvent::SyncEnd);
            self.changed = true;
        }
        let (cols, rows) = self.grid.size();
        sink.push_raw(RawEvent::Start { cols, rows });
        for chunk in self.grid.restore().chunks(RESTORE_CHUNK) {
            sink.push_output(chunk);
        }
        self.sinks.push(sink);
        self.grid.take_replies()
    }

    pub(crate) fn detach(&mut self, sink: &Arc<Outbox>) {
        self.sinks.retain(|held| !Arc::ptr_eq(held, sink));
    }

    fn send(&mut self, event: RawEvent) {
        self.sinks.retain(|sink| !sink.is_closed());
        for sink in &self.sinks {
            sink.push_raw(event.clone());
        }
    }
}

/// The PTY reader of a pane. It waits until every connection has room, so
/// a fast program cannot run far ahead of the window that shows it. A
/// connection that takes no output for RAW_WAIT is closed.
pub(crate) fn reader(output: SharedOutput) -> impl FnMut(&[u8]) -> Vec<u8> + Send + 'static {
    move |bytes| {
        let sinks = lock_mutex(&output).sinks.clone();
        let deadline = Instant::now() + RAW_WAIT;
        for sink in &sinks {
            if !sink.wait_for_room(deadline) {
                sink.close();
            }
        }
        let mut pane = lock_mutex(&output);
        let shown = pane.grid.feed(bytes);
        pane.changed |= shown;
        for sink in &pane.sinks {
            sink.push_output(bytes);
        }
        pane.grid.take_replies()
    }
}
