use std::io;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_net::Stream;

use crate::attachments::{AttachmentGuard, ClientWriter, lock_writer};
use crate::registry::PersonRecord;
use crate::server::BrokerState;

// A burst of one poll tick can hold many pane messages; the queue and the deadline must be larger than one tick.
const EVENT_QUEUE_CAPACITY: usize = 64;
const CLIENT_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn forward<S>(client: S, owner: &PersonRecord, broker: &BrokerState) -> io::Result<()>
where
    S: Stream + Clone,
{
    let mut coordinator = Coordinator::new(client, owner, broker)?;
    let result = coordinator.run();
    result.and(coordinator.close())
}

struct Coordinator<'a> {
    client: ClientWriter,
    client_reader: Option<ReaderTask>,
    attachment: Option<AttachmentGuard<'a>>,
    client_id: String,
    owner: &'a str,
    owner_name: &'a str,
    owner_is_admin: bool,
    broker: &'a BrokerState,
    event_sender: SyncSender<Event>,
    events: Receiver<Event>,
    runtime: Option<RuntimeConnection>,
    peeking: bool,
}

impl<'a> Coordinator<'a> {
    fn new<S>(client: S, owner: &'a PersonRecord, broker: &'a BrokerState) -> io::Result<Self>
    where
        S: Stream + Clone,
    {
        client.set_write_timeout(Some(CLIENT_WRITE_TIMEOUT))?;
        let (event_sender, events) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let client_reader = client.clone();
        let client = Arc::new(Mutex::new(Box::new(client) as Box<dyn Stream>));
        let attachment = broker.attach_client(&owner.user_id, Arc::clone(&client))?;
        let client_id = attachment.client_id().to_owned();
        write_client(
            &client,
            &ServerMsg::Welcome {
                user_id: owner.user_id.clone(),
                name: owner.name.clone(),
                client_id: client_id.clone(),
                tree: Tree::new(),
            },
        )?;
        let runtime =
            match connect_runtime(broker, &owner.user_id, &owner.name, None, &event_sender) {
                Ok(runtime) => runtime,
                Err(error) => {
                    write_client(
                        &client,
                        &ServerMsg::Refused {
                            reason: error.to_string(),
                        },
                    )?;
                    return Err(error);
                }
            };
        let client_reader = spawn_client_reader(client_reader, event_sender.clone());
        Ok(Self {
            client,
            client_reader: Some(client_reader),
            attachment: Some(attachment),
            client_id,
            owner: &owner.user_id,
            owner_name: &owner.name,
            owner_is_admin: owner.is_owner,
            broker,
            event_sender,
            events,
            runtime: Some(runtime),
            peeking: false,
        })
    }

    fn run(&mut self) -> io::Result<()> {
        loop {
            let event = self
                .events
                .recv()
                .map_err(|_| io::Error::other("forward event channel closed"))?;
            let action = match event {
                Event::Client(result) => self.handle_client_result(result)?,
                Event::Runtime { identity, result } => {
                    self.handle_runtime_result(&identity, result)?
                }
            };
            if action == Action::Stop {
                return Ok(());
            }
        }
    }

    fn handle_client_result(&mut self, result: io::Result<ClientMsg>) -> io::Result<Action> {
        match result {
            Ok(message) => self.handle_client_message(message),
            Err(_) => Ok(Action::Stop),
        }
    }

    fn handle_client_message(&mut self, message: ClientMsg) -> io::Result<Action> {
        match message {
            ClientMsg::Invite { hours } if self.owner_is_admin => {
                self.write_client(&self.broker.invite(hours)?)?;
            }
            ClientMsg::Invite { .. } => {
                self.write_client(&ServerMsg::Refused {
                    reason: "owner access required".into(),
                })?;
            }
            ClientMsg::ListPeople => {
                self.write_client(&self.broker.people()?)?;
            }
            ClientMsg::DetachClient { client_id } if client_id.is_empty() => {
                let clients = self.broker.clients(self.owner, &self.client_id)?;
                self.write_client(&ServerMsg::Clients { clients })?;
            }
            ClientMsg::DetachClient { client_id } => {
                if !self.broker.detach_client(self.owner, &client_id)? {
                    eprintln!("broker refused DetachClient for user {}", self.owner);
                    self.write_client(&ServerMsg::Refused {
                        reason: "client does not belong to this person".into(),
                    })?;
                }
            }
            ClientMsg::Peek { ref user, .. } => {
                let Some(person) = self.person(user)? else {
                    eprintln!("broker dropped Peek for unknown user: {user}");
                    return Ok(Action::Continue);
                };
                self.switch_runtime(&person.user_id, &person.name, Some(&message))?;
                self.peeking = true;
            }
            ClientMsg::StopPeek if self.peeking => self.stop_peek()?,
            ClientMsg::StopPeek => {}
            ClientMsg::Input { .. } if self.peeking => {
                eprintln!("broker dropped Input while user {} peeks", self.owner);
            }
            message => self.send_to_runtime(&message)?,
        }
        Ok(Action::Continue)
    }

    fn handle_runtime_result(
        &mut self,
        identity: &Arc<()>,
        result: io::Result<ServerMsg>,
    ) -> io::Result<Action> {
        if !self.runtime_is(identity) {
            return Ok(Action::Continue);
        }
        match result {
            Ok(message) => {
                self.write_client(&message)?;
                Ok(Action::Continue)
            }
            Err(_) if self.peeking => {
                self.stop_peek()?;
                Ok(Action::Continue)
            }
            Err(_) => Ok(Action::Stop),
        }
    }

    fn person(&self, user_id: &str) -> io::Result<Option<PersonRecord>> {
        self.broker.registry().person(user_id)
    }

    fn runtime_is(&self, identity: &Arc<()>) -> bool {
        self.runtime
            .as_ref()
            .is_some_and(|runtime| Arc::ptr_eq(&runtime.identity, identity))
    }

    fn send_to_runtime(&mut self, message: &ClientMsg) -> io::Result<()> {
        let runtime = self
            .runtime
            .as_mut()
            .ok_or_else(|| io::Error::other("runtime connection is missing"))?;
        codec::encode(&mut runtime.stream, message)
    }

    fn write_client(&self, message: &ServerMsg) -> io::Result<()> {
        write_client(&self.client, message)
    }

    fn stop_peek(&mut self) -> io::Result<()> {
        self.peeking = false;
        self.switch_runtime(self.owner, self.owner_name, None)
    }

    fn switch_runtime(
        &mut self,
        user: &str,
        person_name: &str,
        first: Option<&ClientMsg>,
    ) -> io::Result<()> {
        self.close_runtime()?;
        self.runtime = Some(connect_runtime(
            self.broker,
            user,
            person_name,
            first,
            &self.event_sender,
        )?);
        Ok(())
    }

    fn close_runtime(&mut self) -> io::Result<()> {
        match self.runtime.take() {
            Some(runtime) => runtime.close(),
            None => Ok(()),
        }
    }

    fn close(&mut self) -> io::Result<()> {
        self.attachment.take();
        if let Some(reader) = &self.client_reader {
            reader.cancel();
        }
        if let Ok(client) = lock_writer(&self.client) {
            let _ = client.shutdown(std::net::Shutdown::Both);
        }
        let runtime_result = self.close_runtime();
        let client_result = match self.client_reader.take() {
            Some(reader) => reader.join(),
            None => Ok(()),
        };
        runtime_result.and(client_result)
    }
}

fn write_client(client: &ClientWriter, message: &ServerMsg) -> io::Result<()> {
    codec::encode(&mut *lock_writer(client)?, message)
}

struct RuntimeConnection {
    stream: UnixStream,
    identity: Arc<()>,
    reader: ReaderTask,
}

fn connect_runtime(
    broker: &BrokerState,
    user: &str,
    person_name: &str,
    first: Option<&ClientMsg>,
    sender: &SyncSender<Event>,
) -> io::Result<RuntimeConnection> {
    let mut stream = broker.runtimes().connect(user, person_name)?;
    if let Some(message) = first {
        codec::encode(&mut stream, message)?;
    }
    let identity = Arc::new(());
    let reader = spawn_runtime_reader(stream.try_clone()?, Arc::clone(&identity), sender.clone());
    Ok(RuntimeConnection {
        stream,
        identity,
        reader,
    })
}

impl RuntimeConnection {
    fn close(self) -> io::Result<()> {
        self.reader.cancel();
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        self.reader.join()
    }
}

impl Drop for Coordinator<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

enum Event {
    Client(io::Result<ClientMsg>),
    Runtime {
        identity: Arc<()>,
        result: io::Result<ServerMsg>,
    },
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Action {
    Continue,
    Stop,
}

fn spawn_client_reader<S: Stream>(mut stream: S, sender: SyncSender<Event>) -> ReaderTask {
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
    mut stream: UnixStream,
    identity: Arc<()>,
    sender: SyncSender<Event>,
) -> ReaderTask {
    ReaderTask::spawn(move |cancelled| {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    let event = Event::Runtime {
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

struct ReaderTask {
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

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn join(self) -> io::Result<()> {
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

#[cfg(test)]
mod tests;
