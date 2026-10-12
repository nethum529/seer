use super::writer::RuntimeWriter;
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
    reader: Option<ReaderTask>,
    writer: Option<RuntimeWriter>,
}

impl RuntimeConnection {
    // The broker asks; the runtime opens and writes first, so this side reads
    // the readiness reply before anything else.
    pub(super) fn connect(
        broker: &BrokerState,
        person: &PersonRecord,
        sender: &SyncSender<Event>,
    ) -> io::Result<Self> {
        let stream = broker.runtimes().open(&person.user_id)?;
        let result = Self::setup(stream.clone(), person, sender);
        if result.is_err() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        result
    }

    fn setup(
        mut stream: Socket,
        person: &PersonRecord,
        sender: &SyncSender<Event>,
    ) -> io::Result<Self> {
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        codec::encode(&mut stream, &ClientMsg::ObserveRuntime)?;
        let ServerMsg::RuntimeReady { .. } = codec::decode(&mut stream)? else {
            return Err(io::Error::other("runtime did not report readiness"));
        };
        let ServerMsg::Tree { tree } = codec::decode(&mut stream)? else {
            return Err(io::Error::other("expected runtime tree"));
        };
        let ServerMsg::Terminals { terminals, .. } = codec::decode(&mut stream)? else {
            return Err(io::Error::other("expected runtime terminals"));
        };
        stream.set_read_timeout(None)?;
        let identity = Arc::new(());
        let mut connection = Self {
            stream,
            identity,
            tree,
            frames: HashMap::new(),
            terminals,
            reader: None,
            writer: None,
        };
        connection.reader = Some(spawn_runtime_reader(
            connection.stream.clone(),
            person.user_id.clone(),
            Arc::clone(&connection.identity),
            sender.clone(),
        ));
        connection.writer = Some(RuntimeWriter::spawn(
            connection.stream.clone(),
            person.user_id.clone(),
            Arc::clone(&connection.identity),
            sender.clone(),
        ));
        Ok(connection)
    }

    pub(super) fn send(&mut self, message: &ClientMsg) -> io::Result<()> {
        self.writer()?.send(message, None)
    }

    pub(super) fn restore_watch(&mut self, message: &ClientMsg) -> io::Result<()> {
        self.writer()?.restore_watch(message)
    }

    pub(super) fn send_granted(
        &mut self,
        message: &ClientMsg,
        permit: Arc<crate::grants::Permit>,
    ) -> io::Result<()> {
        self.writer()?.send(message, Some(permit))
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
    pub(super) fn close(mut self) -> io::Result<()> {
        self.stop()
    }

    fn writer(&self) -> io::Result<&RuntimeWriter> {
        self.writer
            .as_ref()
            .ok_or_else(|| io::Error::other("runtime connection is closed"))
    }

    fn stop(&mut self) -> io::Result<()> {
        if let Some(reader) = &self.reader {
            reader.cancel();
        }
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        let written = self.writer.take().map_or(Ok(()), RuntimeWriter::close);
        let read = self.reader.take().map_or(Ok(()), ReaderTask::join);
        written.and(read)
    }
}

impl Drop for RuntimeConnection {
    fn drop(&mut self) {
        let _ = self.stop();
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

pub(super) fn send_event(sender: &SyncSender<Event>, event: Event, cancelled: &AtomicBool) -> bool {
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
