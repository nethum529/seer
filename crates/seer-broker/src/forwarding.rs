use std::collections::{BTreeSet, HashMap};
use std::io;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TerminalInput, Tree};
use seer_net::Stream;

use crate::attachments::{AttachmentGuard, ClientWriter, lock_writer};
use crate::grants::LineMarker;
use crate::registry::PersonRecord;
use crate::server::BrokerState;

mod transport;
use transport::{Event, ReaderTask, RuntimeConnection, spawn_client_reader};

pub(crate) fn forward<S: Stream + Clone>(
    client: S,
    owner: &PersonRecord,
    broker: &BrokerState,
) -> io::Result<()> {
    let mut coordinator = Coordinator::new(client, owner, broker)?;
    let result = coordinator.run();
    result.and(coordinator.close())
}

struct Coordinator<'a> {
    client: ClientWriter,
    client_reader: Option<ReaderTask>,
    attachment: Option<AttachmentGuard<'a>>,
    client_id: String,
    owner: &'a PersonRecord,
    broker: &'a BrokerState,
    event_sender: SyncSender<Event>,
    events: Receiver<Event>,
    runtimes: HashMap<String, RuntimeConnection>,
    watches: BTreeSet<(String, String)>,
    lists: BTreeSet<String>,
    markers: HashMap<(String, String), LineMarker>,
}

impl<'a> Coordinator<'a> {
    fn new<S: Stream + Clone>(
        client: S,
        owner: &'a PersonRecord,
        broker: &'a BrokerState,
    ) -> io::Result<Self> {
        client.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
        let (event_sender, events) = mpsc::sync_channel(64);
        let reader = client.clone();
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
        write_client(&client, &broker.grants.message(&owner.user_id)?)?;
        let runtime = match RuntimeConnection::connect(broker, owner, true, &event_sender) {
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
        write_client(
            &client,
            &ServerMsg::Tree {
                tree: runtime.tree.clone(),
            },
        )?;
        let client_reader = spawn_client_reader(reader, event_sender.clone());
        Ok(Self {
            client,
            client_reader: Some(client_reader),
            attachment: Some(attachment),
            client_id,
            owner,
            broker,
            event_sender,
            events,
            runtimes: HashMap::from([(owner.user_id.clone(), runtime)]),
            watches: BTreeSet::new(),
            lists: BTreeSet::new(),
            markers: HashMap::new(),
        })
    }

    fn run(&mut self) -> io::Result<()> {
        loop {
            let event = self
                .events
                .recv()
                .map_err(|_| io::Error::other("forward event channel closed"))?;
            match event {
                Event::Client(Ok(ClientMsg::Detach)) | Event::Client(Err(_)) => return Ok(()),
                Event::Client(Ok(message)) => {
                    if let Err(error) = self.handle_client(message) {
                        self.write(&ServerMsg::Refused {
                            reason: error.to_string(),
                        })?;
                    }
                }
                Event::Runtime {
                    user,
                    identity,
                    result,
                } => {
                    if !self
                        .runtimes
                        .get(&user)
                        .is_some_and(|runtime| Arc::ptr_eq(&runtime.identity, &identity))
                    {
                        continue;
                    }
                    if let Ok(message) = result {
                        self.handle_runtime(&user, message)?;
                    } else if user == self.owner.user_id {
                        return Ok(());
                    } else {
                        if let Some(runtime) = self.runtimes.remove(&user) {
                            runtime.close()?;
                        }
                        self.write(&ServerMsg::Terminals {
                            user,
                            terminals: Vec::new(),
                        })?;
                    }
                }
            }
        }
    }

    fn handle_client(&mut self, message: ClientMsg) -> io::Result<()> {
        match message {
            ClientMsg::Watch { user, pane } => self.watch(&user, &pane),
            ClientMsg::Unwatch { user, pane } => {
                self.watches.remove(&(user, pane));
                Ok(())
            }
            ClientMsg::Terminals { user } => self.list(&user),
            ClientMsg::TypeInto { user, pane, bytes } => self.type_into(&user, &pane, bytes),
            ClientMsg::SetGrant { user, can_type } => self.set_grant(&user, can_type),
            ClientMsg::ListPeople => self.write(&self.broker.people()?),
            ClientMsg::Invite { hours } if self.owner.is_owner => {
                self.write(&self.broker.invite(hours)?)
            }
            ClientMsg::Invite { .. } => Err(io::Error::other("owner access required")),
            ClientMsg::DetachClient { client_id } => self.detach_client(&client_id),
            ClientMsg::QueryTargets { user } => self.query_targets(&user),
            message
                if message.is_mutating()
                    || matches!(message, ClientMsg::TerminalCapabilities { .. }) =>
            {
                self.runtime(&self.owner.user_id.clone())?.send(&message)
            }
            _ => Err(io::Error::other("unsupported client message")),
        }
    }

    fn runtime(&mut self, user: &str) -> io::Result<&mut RuntimeConnection> {
        if !self.runtimes.contains_key(user) {
            let person = self.person(user)?;
            let runtime =
                RuntimeConnection::connect(self.broker, &person, false, &self.event_sender)?;
            self.runtimes.insert(user.to_owned(), runtime);
        }
        self.runtimes
            .get_mut(user)
            .ok_or_else(|| io::Error::other("runtime connection is missing"))
    }

    fn person(&self, user: &str) -> io::Result<PersonRecord> {
        self.broker
            .registry()
            .person(user)?
            .ok_or_else(|| io::Error::other("person not found"))
    }

    fn watch(&mut self, user: &str, pane: &str) -> io::Result<()> {
        let runtime = self.runtime(user)?;
        runtime.location(pane)?;
        let frame = runtime.frames.get(pane).cloned();
        self.watches.insert((user.to_owned(), pane.to_owned()));
        if let Some(frame) = frame {
            self.write(&ServerMsg::Cells {
                user: user.into(),
                pane: pane.into(),
                frame,
            })?;
        }
        Ok(())
    }

    fn list(&mut self, user: &str) -> io::Result<()> {
        self.person(user)?;
        self.lists.insert(user.to_owned());
        let terminals = match self.runtime(user) {
            Ok(runtime) => runtime.terminals.clone(),
            Err(_) => Vec::new(),
        };
        self.write(&ServerMsg::Terminals {
            user: user.into(),
            terminals,
        })
    }

    fn set_grant(&self, user: &str, can_type: bool) -> io::Result<()> {
        self.person(user)?;
        self.broker
            .grants
            .set(&self.owner.user_id, user, can_type)?;
        self.broker.publish_grants()
    }

    fn type_into(&mut self, user: &str, pane: &str, bytes: Vec<u8>) -> io::Result<()> {
        let person = self.person(user)?;
        if !self.broker.grants.permits(user, &self.owner.user_id)? {
            self.markers.remove(&(user.into(), pane.into()));
            return Err(io::Error::other(format!(
                "{} has not let you type",
                person.name
            )));
        }
        let (workspace, tab) = self.runtime(user)?.location(pane)?;
        let text = self
            .markers
            .entry((user.into(), pane.into()))
            .or_default()
            .prefix(&self.owner.name, bytes)?;
        self.runtime(user)?.send(&ClientMsg::TerminalInput {
            workspace,
            tab,
            pane: pane.into(),
            input: TerminalInput::new(InputEvent::Text(text)),
        })
    }

    fn detach_client(&self, client_id: &str) -> io::Result<()> {
        if !client_id.is_empty() && !self.broker.detach_client(&self.owner.user_id, client_id)? {
            return Err(io::Error::other("client does not belong to this person"));
        }
        self.write(&ServerMsg::Clients {
            clients: self.broker.clients(&self.owner.user_id, &self.client_id)?,
        })
    }

    fn query_targets(&mut self, user: &str) -> io::Result<()> {
        let person = self.person(user)?;
        let mut stream = self
            .broker
            .runtimes()
            .connect_existing(user, &person.name)?;
        codec::encode(&mut stream, &ClientMsg::QueryTargets { user: user.into() })?;
        let message: ServerMsg = codec::decode(&mut stream)?;
        self.write(&message)
    }

    fn handle_runtime(&mut self, user: &str, message: ServerMsg) -> io::Result<()> {
        let Some(runtime) = self.runtimes.get_mut(user) else {
            return Ok(());
        };
        match message {
            ServerMsg::Tree { tree } => {
                runtime.tree = tree.clone();
                let panes: BTreeSet<_> = tree
                    .workspaces
                    .iter()
                    .flat_map(|w| &w.tabs)
                    .flat_map(|t| &t.panes)
                    .map(|p| p.id.as_str())
                    .collect();
                runtime
                    .frames
                    .retain(|pane, _| panes.contains(pane.as_str()));
                if user == self.owner.user_id {
                    self.write(&ServerMsg::Tree { tree })?;
                }
            }
            ServerMsg::Cells { pane, frame, .. } => {
                runtime.frames.insert(pane.clone(), frame.clone());
                if user == self.owner.user_id || self.watches.contains(&(user.into(), pane.clone()))
                {
                    self.write(&ServerMsg::Cells {
                        user: user.into(),
                        pane,
                        frame,
                    })?;
                }
            }
            ServerMsg::Terminals { terminals, .. } => {
                let workspace = runtime.tree.workspaces.first();
                let terminals: Vec<_> = terminals
                    .into_iter()
                    .filter(|t| {
                        workspace.is_some_and(|w| {
                            w.tabs
                                .iter()
                                .any(|tab| tab.panes.iter().any(|p| p.id == t.pane))
                        })
                    })
                    .collect();
                runtime.terminals.clone_from(&terminals);
                if user == self.owner.user_id || self.lists.contains(user) {
                    self.write(&ServerMsg::Terminals {
                        user: user.into(),
                        terminals,
                    })?;
                }
            }
            ServerMsg::Refused { reason } => self.write(&ServerMsg::Refused { reason })?,
            _ => {}
        }
        Ok(())
    }

    fn write(&self, message: &ServerMsg) -> io::Result<()> {
        write_client(&self.client, message)
    }

    fn close(&mut self) -> io::Result<()> {
        self.attachment.take();
        if let Some(reader) = &self.client_reader {
            reader.cancel();
        }
        if let Ok(client) = lock_writer(&self.client) {
            let _ = client.shutdown(std::net::Shutdown::Both);
        }
        for (_, runtime) in self.runtimes.drain() {
            runtime.close()?;
        }
        if let Some(reader) = self.client_reader.take() {
            reader.join()?;
        }
        Ok(())
    }
}

impl Drop for Coordinator<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn write_client(client: &ClientWriter, message: &ServerMsg) -> io::Result<()> {
    codec::encode(&mut *lock_writer(client)?, message)
}
