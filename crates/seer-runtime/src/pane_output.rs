use seer_core::proto::codec::RawEvent;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::PaneGrid;
use crate::outbox::Outbox;
use crate::pty::{Answers, lock_mutex};
use unfinished::Unfinished;

mod unfinished;

// The same deadline as a slow connection write (server/writer.rs).
const RAW_WAIT: Duration = Duration::from_secs(2);
const RESTORE_CHUNK: usize = 1024 * 1024;

/// The grid of one pane and the connections that get its raw output. The
/// PTY reader feeds the grid and the connections under one lock, so a
/// connection sees the bytes and the size changes in the order the grid
/// saw them.
pub(crate) struct PaneOutput {
    pub(crate) grid: PaneGrid,
    sinks: Vec<Sink>,
    changed: bool,
    unfinished: Unfinished,
    held: VecDeque<Held>,
    marks: u64,
}

struct Sink {
    outbox: Arc<Outbox>,
    parsed: u64,
}

// Answers to queries wait until each window parsed the output before the
// query, so an answer means that the window shows that output.
struct Held {
    mark: u64,
    bytes: Vec<u8>,
    since: Instant,
}

pub(crate) type SharedOutput = Arc<Mutex<PaneOutput>>;

/// The output of a pane and the queue for the answers to queries.
#[derive(Clone)]
pub(crate) struct Output {
    shared: SharedOutput,
    answers: Answers,
}

impl Output {
    pub(crate) fn new(shared: SharedOutput, answers: Answers) -> Self {
        Self { shared, answers }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, PaneOutput> {
        lock_mutex(&self.shared)
    }

    /// Runs `step` and queues the answers it gives for the PTY.
    pub(crate) fn answer<T>(&self, step: impl FnOnce(&mut PaneOutput) -> (T, Vec<u8>)) -> T {
        answer(&self.answers, &self.shared, step)
    }

    pub(crate) fn parsed(&self, sink: &Arc<Outbox>, mark: u64) {
        self.answer(|pane| {
            if let Some(held) = pane.sinks.iter_mut().find(|held| held.is(sink)) {
                held.parsed = held.parsed.max(mark);
            }
            ((), pane.release())
        });
    }
}

impl Sink {
    fn is(&self, outbox: &Arc<Outbox>) -> bool {
        Arc::ptr_eq(&self.outbox, outbox)
    }
}

impl PaneOutput {
    pub(crate) fn shared(grid: PaneGrid) -> SharedOutput {
        Arc::new(Mutex::new(Self {
            grid,
            sinks: Vec::new(),
            changed: false,
            unfinished: Unfinished::default(),
            held: VecDeque::new(),
            marks: 0,
        }))
    }

    /// Ends a synchronized update after its timeout, and closes a window
    /// that did not parse up to a held answer in time. Gives whether the
    /// screen changed since the last call, and the answers to write.
    pub(crate) fn poll(&mut self) -> (bool, Vec<u8>) {
        if self.grid.end_expired_sync() {
            self.send(RawEvent::SyncEnd);
            self.changed = true;
        }
        if let Some(late) = self
            .held
            .front()
            .filter(|held| held.since.elapsed() >= RAW_WAIT)
        {
            for sink in self.sinks.iter().filter(|sink| sink.parsed < late.mark) {
                sink.outbox.close();
            }
        }
        let answers = self.hold();
        (std::mem::take(&mut self.changed), answers)
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        self.grid.resize(cols, rows);
        self.send(RawEvent::Size { cols, rows });
    }

    /// Sends the sink the whole state first, then every later byte. Gives
    /// the answers to write.
    pub(crate) fn attach(&mut self, outbox: Arc<Outbox>) -> Vec<u8> {
        if self.grid.end_sync() {
            self.send(RawEvent::SyncEnd);
            self.changed = true;
        }
        let answers = self.hold();
        let (cols, rows) = self.grid.size();
        outbox.push_raw(RawEvent::Start { cols, rows });
        for chunk in self.grid.restore().chunks(RESTORE_CHUNK) {
            outbox.push_output(chunk);
        }
        outbox.push_output(self.unfinished.bytes());
        self.sinks.push(Sink {
            outbox,
            parsed: self.marks,
        });
        answers
    }

    pub(crate) fn detach(&mut self, outbox: &Arc<Outbox>) -> Vec<u8> {
        self.sinks.retain(|sink| !sink.is(outbox));
        self.release()
    }

    fn send(&mut self, event: RawEvent) {
        self.sinks.retain(|sink| !sink.outbox.is_closed());
        for sink in &self.sinks {
            sink.outbox.push_raw(event.clone());
        }
    }

    // Holds the answers the grid gave until each window parsed the output
    // before them, and gives the answers that wait no more.
    fn hold(&mut self) -> Vec<u8> {
        let bytes = self.grid.take_replies();
        if !bytes.is_empty() {
            self.marks += 1;
            self.send(RawEvent::Mark(self.marks));
            self.held.push_back(Held {
                mark: self.marks,
                bytes,
                since: Instant::now(),
            });
        }
        self.release()
    }

    fn release(&mut self) -> Vec<u8> {
        self.sinks.retain(|sink| !sink.outbox.is_closed());
        let parsed = self.sinks.iter().map(|sink| sink.parsed).min();
        let mut answers = Vec::new();
        while let Some(held) = self
            .held
            .pop_front_if(|held| parsed.is_none_or(|parsed| held.mark <= parsed))
        {
            answers.extend(held.bytes);
        }
        answers
    }
}

// The answers go to the queue under the pane lock, so answers that two
// threads let go reach the program in order.
fn answer<T>(
    answers: &Answers,
    shared: &SharedOutput,
    step: impl FnOnce(&mut PaneOutput) -> (T, Vec<u8>),
) -> T {
    let mut pane = lock_mutex(shared);
    let (value, bytes) = step(&mut pane);
    if !bytes.is_empty() {
        let _ = answers.send(bytes);
    }
    value
}

/// The PTY reader of a pane. It waits until every connection has room, so
/// a fast program cannot run far ahead of the window that shows it. A
/// connection that takes no output for RAW_WAIT is closed.
pub(crate) fn reader(output: SharedOutput) -> impl FnMut(&[u8], &Answers) + Send + 'static {
    move |bytes, answers| {
        let sinks: Vec<_> = lock_mutex(&output)
            .sinks
            .iter()
            .map(|sink| Arc::clone(&sink.outbox))
            .collect();
        let deadline = Instant::now() + RAW_WAIT;
        for sink in &sinks {
            if !sink.wait_for_room(deadline) {
                sink.close();
            }
        }
        answer(answers, &output, |pane| {
            let shown = pane.grid.feed(bytes);
            pane.changed |= shown;
            pane.unfinished.follow(bytes);
            for sink in &pane.sinks {
                sink.outbox.push_output(bytes);
            }
            ((), pane.hold())
        });
    }
}
