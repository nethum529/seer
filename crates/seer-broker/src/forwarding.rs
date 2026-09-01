use std::io;
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

use crate::registry::PersonRecord;
use crate::server::BrokerState;

pub(crate) fn forward(
    client: TcpStream,
    owner: &PersonRecord,
    broker: &BrokerState,
) -> io::Result<()> {
    let mut coordinator = Coordinator::new(client, owner, broker)?;
    let result = coordinator.run();
    result.and(coordinator.close())
}

struct Coordinator<'a> {
    client: TcpStream,
    client_reader: Option<JoinHandle<()>>,
    owner: &'a str,
    owner_is_admin: bool,
    broker: &'a BrokerState,
    event_sender: Sender<Event>,
    events: Receiver<Event>,
    runtime: Option<RuntimeConnection>,
    peeking: bool,
}

impl<'a> Coordinator<'a> {
    fn new(
        client: TcpStream,
        owner: &'a PersonRecord,
        broker: &'a BrokerState,
    ) -> io::Result<Self> {
        let (event_sender, events) = mpsc::channel();
        let client_reader = client.try_clone()?;
        let runtime = connect_runtime(broker, &owner.user_id, None, &event_sender)?;
        let client_reader = spawn_client_reader(client_reader, event_sender.clone());
        Ok(Self {
            client,
            client_reader: Some(client_reader),
            owner: &owner.user_id,
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
            ClientMsg::Invite if self.owner_is_admin => {
                codec::encode(&mut self.client, &self.broker.invite()?)?;
            }
            ClientMsg::Invite => {
                codec::encode(
                    &mut self.client,
                    &ServerMsg::Refused {
                        reason: "owner access required".into(),
                    },
                )?;
            }
            ClientMsg::ListPeople => {
                codec::encode(&mut self.client, &self.broker.people()?)?;
            }
            ClientMsg::Peek { ref user, .. } if self.user_exists(user)? => {
                self.switch_runtime(user, Some(&message))?;
                self.peeking = true;
            }
            ClientMsg::Peek { user, .. } => {
                eprintln!("broker dropped Peek for unknown user: {user}");
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
                codec::encode(&mut self.client, &message)?;
                Ok(Action::Continue)
            }
            Err(_) if self.peeking => {
                self.stop_peek()?;
                Ok(Action::Continue)
            }
            Err(_) => Ok(Action::Stop),
        }
    }

    fn user_exists(&self, user_id: &str) -> io::Result<bool> {
        self.broker.registry().person_exists(user_id)
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

    fn stop_peek(&mut self) -> io::Result<()> {
        self.peeking = false;
        self.switch_runtime(self.owner, None)
    }

    fn switch_runtime(&mut self, user: &str, first: Option<&ClientMsg>) -> io::Result<()> {
        self.close_runtime()?;
        self.runtime = Some(connect_runtime(
            self.broker,
            user,
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
        let _ = self.client.shutdown(std::net::Shutdown::Both);
        let runtime_result = self.close_runtime();
        let client_result = match self.client_reader.take() {
            Some(reader) => join_reader(reader),
            None => Ok(()),
        };
        runtime_result.and(client_result)
    }
}

struct RuntimeConnection {
    stream: UnixStream,
    identity: Arc<()>,
    reader: JoinHandle<()>,
}

fn connect_runtime(
    broker: &BrokerState,
    user: &str,
    first: Option<&ClientMsg>,
    sender: &Sender<Event>,
) -> io::Result<RuntimeConnection> {
    let mut stream = broker.runtimes().connect(user)?;
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
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        join_reader(self.reader)
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

fn spawn_client_reader(mut stream: TcpStream, sender: Sender<Event>) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    if sender.send(Event::Client(Ok(message))).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Event::Client(Err(error)));
                    return;
                }
            }
        }
    })
}

fn spawn_runtime_reader(
    mut stream: UnixStream,
    identity: Arc<()>,
    sender: Sender<Event>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    let event = Event::Runtime {
                        identity: Arc::clone(&identity),
                        result: Ok(message),
                    };
                    if sender.send(event).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Event::Runtime {
                        identity: Arc::clone(&identity),
                        result: Err(error),
                    });
                    return;
                }
            }
        }
    })
}

fn join_reader(reader: JoinHandle<()>) -> io::Result<()> {
    reader
        .join()
        .map_err(|_| io::Error::other("forward reader thread panicked"))
}

#[cfg(test)]
mod tests;
