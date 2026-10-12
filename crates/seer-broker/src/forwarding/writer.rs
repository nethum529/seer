use std::io::{self, Write};
use std::net::Shutdown;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_net::{Socket, Stream};

use super::transport::{Event, send_event};
use crate::grants::Permit;

const SEND_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_PENDING_BYTES: usize = 16 * 1024 * 1024 + 4;

pub(super) struct RuntimeWriter {
    queue: SyncSender<Frame>,
    pending: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

struct Frame {
    bytes: Vec<u8>,
    permit: Option<Arc<Permit>>,
    deadline: Instant,
    pending: Arc<AtomicUsize>,
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.pending.fetch_sub(self.bytes.len(), Ordering::AcqRel);
    }
}

impl RuntimeWriter {
    pub(super) fn spawn(
        stream: Socket,
        user: String,
        identity: Arc<()>,
        events: SyncSender<Event>,
    ) -> Self {
        let (queue, frames) = mpsc::sync_channel(64);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let thread = thread::spawn(move || {
            run(stream, frames, &user, &identity, &events, &worker_cancelled);
        });
        Self {
            queue,
            pending: Arc::new(AtomicUsize::new(0)),
            cancelled,
            thread,
        }
    }

    pub(super) fn send(&self, message: &ClientMsg, permit: Option<Arc<Permit>>) -> io::Result<()> {
        let mut bytes = Vec::new();
        codec::encode(&mut bytes, message)?;
        let size = bytes.len();
        let previous = self.pending.fetch_add(size, Ordering::AcqRel);
        if previous > MAX_PENDING_BYTES - size {
            self.pending.fetch_sub(size, Ordering::AcqRel);
            return Err(io::Error::other("runtime input queue is full"));
        }
        let frame = Frame {
            bytes,
            permit,
            deadline: Instant::now() + SEND_TIMEOUT,
            pending: Arc::clone(&self.pending),
        };
        self.queue
            .try_send(frame)
            .map_err(|error| io::Error::other(format!("runtime input queue unavailable: {error}")))
    }

    pub(super) fn close(self) -> io::Result<()> {
        self.cancelled.store(true, Ordering::Release);
        drop(self.queue);
        self.thread
            .join()
            .map_err(|_| io::Error::other("runtime writer thread panicked"))
    }
}

fn run(
    mut stream: Socket,
    frames: Receiver<Frame>,
    user: &str,
    identity: &Arc<()>,
    events: &SyncSender<Event>,
    cancelled: &AtomicBool,
) {
    while let Ok(frame) = frames.recv() {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        let result = write_frame(&mut stream, &frame);
        let message = match result {
            Ok(true) => continue,
            Ok(false) => Ok(ServerMsg::Refused {
                reason: "Control was revoked".into(),
            }),
            Err(error) => {
                let _ = stream.shutdown(Shutdown::Both);
                Err(error)
            }
        };
        let failed = message.is_err();
        let event = Event::Runtime {
            user: user.into(),
            identity: Arc::clone(identity),
            result: message,
        };
        if !send_event(events, event, cancelled) || failed {
            break;
        }
    }
}

fn write_frame(stream: &mut Socket, frame: &Frame) -> io::Result<bool> {
    let _active = if let Some(permit) = &frame.permit {
        let Some(active) = permit.start(stream)? else {
            return Ok(false);
        };
        Some(active)
    } else {
        None
    };
    let mut remaining = frame.bytes.as_slice();
    while !remaining.is_empty() {
        let timeout = frame.deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "runtime send timed out",
            ));
        }
        stream.set_write_timeout(Some(timeout))?;
        match stream.write(remaining) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => remaining = &remaining[written..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}
