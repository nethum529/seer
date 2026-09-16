use crate::commands::Welcome;
use crate::state::ClientState;
use crossterm::event::{self, Event};
use seer_core::proto::{ServerMsg, codec};
use seer_net::Socket;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const ROOM_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Source {
    Local,
    Room,
}

pub(crate) struct Envelope {
    pub(crate) source: Source,
    pub(crate) message: io::Result<ServerMsg>,
}

pub(crate) type Events = Receiver<Envelope>;

// A room socket must never block the window: reads wait forever, writes are
// bounded so a stuck room drops the route instead of freezing local work.
pub(crate) fn prepare_room(room: &Socket) -> io::Result<()> {
    use seer_net::Stream;
    room.set_read_timeout(None)?;
    room.set_write_timeout(Some(ROOM_WRITE_TIMEOUT))
}

pub(crate) fn event_channel() -> (Events, mpsc::SyncSender<Envelope>) {
    let (sender, receiver) = mpsc::sync_channel(64);
    (receiver, sender)
}

pub(crate) fn spawn_reader(
    mut stream: Socket,
    source: Source,
    sender: mpsc::SyncSender<Envelope>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            let message = codec::decode(&mut stream);
            log_received(source, &message);
            let failed = message.is_err();
            if sender.send(Envelope { source, message }).is_err() || failed {
                break;
            }
        }
    })
}

#[cfg(debug_assertions)]
fn log_received(source: Source, message: &io::Result<ServerMsg>) {
    match message {
        Ok(ServerMsg::Cells { .. }) => {}
        Ok(message) => seer_core::debug_log!(
            "recv source={source:?} {}",
            seer_core::debug_log::server_summary(message)
        ),
        Err(error) => seer_core::debug_log!("recv source={source:?} error={error}"),
    }
}

#[cfg(not(debug_assertions))]
fn log_received(_source: Source, _message: &io::Result<ServerMsg>) {}

// Issue 429: crossterm reads the tty in a loop that never returns once the
// tty is hung up and every read gives end of file. That loop must not hold
// the window loop, so the tty has its own thread that ends with the process.
pub(crate) fn spawn_terminal_reader() -> Receiver<Event> {
    let (sender, receiver) = mpsc::sync_channel(64);
    thread::spawn(move || {
        while let Ok(event) = event::read()
            && sender.send(event).is_ok()
        {}
    });
    receiver
}

pub(crate) fn join_reader(reader: JoinHandle<()>) -> io::Result<()> {
    reader
        .join()
        .map_err(|_| io::Error::other("socket reader thread panicked"))
}

// Local work never waits for the room, so the first try also runs here.
pub(crate) struct Reconnects {
    server: crate::store::ServerEntry,
    sender: mpsc::SyncSender<Envelope>,
    rooms: Receiver<Socket>,
    requests: mpsc::SyncSender<Socket>,
    refusals: Receiver<String>,
    refusal_sender: mpsc::SyncSender<String>,
    stopped: Arc<AtomicBool>,
}

impl Reconnects {
    pub(crate) fn new(
        server: crate::store::ServerEntry,
        sender: mpsc::SyncSender<Envelope>,
    ) -> Self {
        let (requests, rooms) = mpsc::sync_channel(1);
        let (refusal_sender, refusals) = mpsc::sync_channel(1);
        Self {
            server,
            sender,
            rooms,
            requests,
            refusals,
            refusal_sender,
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn start(&self) {
        let server = self.server.clone();
        let rooms = self.requests.clone();
        let refusals = self.refusal_sender.clone();
        let stopped = Arc::clone(&self.stopped);
        thread::spawn(move || {
            let mut wait = Duration::from_secs(1);
            while !stopped.load(Ordering::Relaxed) {
                match crate::commands::hello(&server) {
                    Ok(Welcome::Accepted(room, _)) => {
                        if rooms.send(room).is_ok() {
                            return;
                        }
                    }
                    Ok(Welcome::Refused(reason)) => {
                        let _ = refusals.try_send(refusal_notice(&server, &reason));
                    }
                    Err(_) => {}
                }
                thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_secs(30));
            }
        });
    }

    pub(crate) fn take(&self, state: &mut ClientState) -> Option<Socket> {
        if let Some(notice) = self.refusals.try_iter().last() {
            state.set_room_notice(Some(notice));
        }
        let room = self.rooms.try_recv().ok()?;
        state.set_room_notice(None);
        Some(room)
    }

    pub(crate) fn adopt(&self, room: Socket) -> io::Result<JoinHandle<()>> {
        prepare_room(&room)?;
        Ok(spawn_reader(room, Source::Room, self.sender.clone()))
    }

    pub(crate) fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

fn refusal_notice(server: &crate::store::ServerEntry, reason: &str) -> String {
    let step =
        crate::version_skew::refusal_message(reason, crate::start::hosts_room(&server.endpoint));
    format!("{}: {step}", server.alias)
}
