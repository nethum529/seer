use crate::registry::PersonRecord;
use crate::server::BrokerState;
use seer_core::proto::{ClientMsg, ServerMsg, TerminalInfo, codec};
use seer_core::{TerminalFrame, Tree};
use seer_net::{Socket, Stream};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub(super) struct RuntimeConnection {
    pub(super) stream: Socket,
    pub(super) identity: Arc<()>,
    pub(super) tree: Tree,
    pub(super) frames: HashMap<String, TerminalFrame>,
    pub(super) terminals: Vec<TerminalInfo>,
    reader: ReaderTask,
}

impl RuntimeConnection {
    // The broker asks; the runtime opens and writes first, so this side reads
    // the readiness reply before anything else.
    pub(super) fn connect(
        broker: &BrokerState,
        person: &PersonRecord,
        sender: &SyncSender<Event>,
    ) -> io::Result<Self> {
        let mut stream = broker.runtimes().open(&person.user_id)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        codec::encode(&mut stream, &ClientMsg::ObserveRuntime)?;
        let ServerMsg::RuntimeReady { .. } = codec::decode(&mut stream)? else {
            return Err(io::Error::other("runtime did not report readiness"));
        };
        let ServerMsg::Tree { tree } = codec::decode(&mut stream)? else {
            return Err(io::Error::other("expected runtime tree"));
        };
        stream.set_read_timeout(None)?;
        let identity = Arc::new(());
        let reader = spawn_runtime_reader(
            stream.clone(),
            person.user_id.clone(),
            Arc::clone(&identity),
            sender.clone(),
        );
        Ok(Self {
            stream,
            identity,
            tree,
            frames: HashMap::new(),
            terminals: Vec::new(),
            reader,
        })
    }

    pub(super) fn send(&mut self, message: &ClientMsg) -> io::Result<()> {
        codec::encode(&mut self.stream, message)
    }
    pub(super) fn location(&self, pane: &str) -> io::Result<(String, String)> {
        self.tree
            .workspaces
            .iter()
            .find_map(|w| {
                w.tabs
                    .iter()
                    .find(|t| t.panes.iter().any(|p| p.id == pane))
                    .map(|t| (w.id.clone(), t.id.clone()))
            })
            .ok_or_else(|| io::Error::other("terminal not found"))
    }
    pub(super) fn close(self) -> io::Result<()> {
        self.reader.cancel();
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        self.reader.join()
    }
}

pub(super) enum Event {
    Client(io::Result<ClientMsg>),
    Runtime {
        user: String,
        identity: Arc<()>,
        result: io::Result<ServerMsg>,
    },
}

pub(super) fn spawn_client_reader<S: Stream>(
    mut stream: S,
    sender: SyncSender<Event>,
) -> ReaderTask {
    ReaderTask::spawn(move |cancelled| {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    if !send_event(&sender, Event::Client(Ok(message)), cancelled) {
                        return;
                    }
                }
                Err(error) => {
                    send_event(&sender, Event::Client(Err(error)), cancelled);
                    return;
                }
            }
        }
    })
}

fn spawn_runtime_reader(
    mut stream: Socket,
    user: String,
    identity: Arc<()>,
    sender: SyncSender<Event>,
) -> ReaderTask {
    ReaderTask::spawn(move |cancelled| {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    let event = Event::Runtime {
                        user: user.clone(),
                        identity: Arc::clone(&identity),
                        result: Ok(message),
                    };
                    if !send_event(&sender, event, cancelled) {
                        return;
                    }
                }
                Err(error) => {
                    send_event(
                        &sender,
                        Event::Runtime {
                            user: user.clone(),
                            identity: Arc::clone(&identity),
                            result: Err(error),
                        },
                        cancelled,
                    );
                    return;
                }
            }
        }
    })
}

pub(super) struct ReaderTask {
    thread: JoinHandle<()>,
    cancelled: Arc<AtomicBool>,
}

impl ReaderTask {
    fn spawn(worker: impl FnOnce(&AtomicBool) + Send + 'static) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let thread = thread::spawn(move || worker(&worker_cancelled));
        Self { thread, cancelled }
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(super) fn join(self) -> io::Result<()> {
        join_reader(self.thread)
    }
}

fn send_event(sender: &SyncSender<Event>, event: Event, cancelled: &AtomicBool) -> bool {
    let mut pending = event;
    loop {
        match sender.try_send(pending) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(event)) => pending = event,
        }
        if cancelled.load(Ordering::Acquire) {
            return false;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn join_reader(reader: JoinHandle<()>) -> io::Result<()> {
    reader
        .join()
        .map_err(|_| io::Error::other("forward reader thread panicked"))
}
