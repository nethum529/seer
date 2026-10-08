use seer_core::proto::codec::RawEvent;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use crate::pty::lock_mutex;

// A burst of one poll tick can hold many pane messages; the queue and the deadline must be larger than one tick.
const MESSAGE_LIMIT: usize = 64;
// The raw stream waits for the window at this many queued bytes, so a
// window is at most this far, plus one PTY read and the socket buffer,
// behind the runtime (docs/research/25-terminal-speed.md).
const RAW_LIMIT: usize = 64 * 1024;

/// The bytes that wait to go out on one connection, in order.
#[derive(Default)]
pub(crate) struct Outbox {
    queue: Mutex<Queue>,
    ready: Condvar,
    room: Condvar,
}

pub(crate) enum Outgoing {
    Message(Arc<[u8]>),
    Raw(RawEvent),
}

#[derive(Default)]
struct Queue {
    items: VecDeque<Outgoing>,
    messages: usize,
    raw: usize,
    closed: bool,
    writer_waits: bool,
}

impl Outbox {
    /// False when the connection is closed or too slow for the messages.
    pub(crate) fn push_message(&self, bytes: Arc<[u8]>) -> bool {
        let mut queue = lock_mutex(&self.queue);
        if queue.closed || queue.messages >= MESSAGE_LIMIT {
            return false;
        }
        queue.messages += 1;
        queue.items.push_back(Outgoing::Message(bytes));
        self.wake_writer(&mut queue);
        true
    }

    pub(crate) fn push_raw(&self, event: RawEvent) {
        let mut queue = lock_mutex(&self.queue);
        if queue.closed {
            return;
        }
        queue.items.push_back(Outgoing::Raw(event));
        self.wake_writer(&mut queue);
    }

    /// Raw output has no limit here. The PTY reader calls wait_for_room
    /// before it reads on. Output that the writer did not take yet gets
    /// the new bytes, because a fast program gives many small PTY reads.
    pub(crate) fn push_output(&self, bytes: &[u8]) {
        let mut queue = lock_mutex(&self.queue);
        if queue.closed {
            return;
        }
        queue.raw += bytes.len();
        match queue.items.back_mut() {
            Some(Outgoing::Raw(RawEvent::Output(queued)))
                if queued.len() + bytes.len() <= RAW_LIMIT =>
            {
                queued.extend_from_slice(bytes);
            }
            _ => queue
                .items
                .push_back(Outgoing::Raw(RawEvent::Output(bytes.to_vec()))),
        }
        self.wake_writer(&mut queue);
    }

    fn wake_writer(&self, queue: &mut Queue) {
        if std::mem::take(&mut queue.writer_waits) {
            self.ready.notify_one();
        }
    }

    /// False when the connection closed, or when it did not take the raw
    /// output before the deadline.
    pub(crate) fn wait_for_room(&self, deadline: Instant) -> bool {
        let mut queue = lock_mutex(&self.queue);
        while !queue.closed && queue.raw >= RAW_LIMIT {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            queue = match self.room.wait_timeout(queue, left) {
                Ok((queue, _)) => queue,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
        !queue.closed
    }

    /// The next item to write. None when the connection is closed.
    pub(crate) fn pop(&self) -> Option<Outgoing> {
        let mut queue = lock_mutex(&self.queue);
        loop {
            if queue.closed {
                return None;
            }
            if let Some(item) = queue.items.pop_front() {
                match &item {
                    Outgoing::Message(_) => queue.messages -= 1,
                    Outgoing::Raw(RawEvent::Output(bytes)) => {
                        queue.raw -= bytes.len();
                        self.room.notify_all();
                    }
                    Outgoing::Raw(_) => {}
                }
                return Some(item);
            }
            queue.writer_waits = true;
            queue = match self.ready.wait(queue) {
                Ok(queue) => queue,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
    }

    pub(crate) fn close(&self) {
        let mut queue = lock_mutex(&self.queue);
        queue.closed = true;
        queue.items.clear();
        self.ready.notify_all();
        self.room.notify_all();
    }

    pub(crate) fn is_closed(&self) -> bool {
        lock_mutex(&self.queue).closed
    }
}
