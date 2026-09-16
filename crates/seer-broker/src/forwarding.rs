use std::collections::{BTreeSet, HashMap};
use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{PaneSize, Tree};
use seer_net::Stream;

use crate::attachments::{AttachmentGuard, ClientWriter, lock_writer};
use crate::registry::PersonRecord;
use crate::server::BrokerState;

mod input;
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
    watches: HashMap<(String, String), (PaneSize, bool)>,
    lists: BTreeSet<String>,
}

impl<'a> Coordinator<'a> {
    fn new<S: Stream + Clone>(
        client: S,
        owner: &'a PersonRecord,
        broker: &'a BrokerState,
    ) -> io::Result<Self> {
        client.set_nodelay(true)?;
        client.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
        let (event_sender, events) = mpsc::sync_channel(64);
        let reader = client.clone();
        let client = Arc::new(Mutex::new(Box::new(client) as Box<dyn Stream>));
        let mut initial = lock_writer(&client)?;
        let attachment = broker.attach_client(&owner.user_id, Arc::clone(&client))?;
        let client_id = attachment.client_id().to_owned();
        codec::encode(
            &mut *initial,
            &ServerMsg::Welcome {
                user_id: owner.user_id.clone(),
                name: owner.name.clone(),
                client_id: client_id.clone(),
                tree: Tree::new(),
            },
        )?;
        let setup = codec::encode(&mut *initial, &broker.grants.message(&owner.user_id)?);
        drop(initial);
        setup?;
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
            runtimes: HashMap::new(),
            watches: HashMap::new(),
            lists: BTreeSet::new(),
        })
    }

    fn run(&mut self) -> io::Result<()> {
        let mut refreshed = std::time::Instant::now();
        loop {
            if self
                .broker
                .registry()
                .person(&self.owner.user_id)?
                .is_none()
            {
                return Ok(());
            }
            self.remove_departed_runtimes()?;
            if refreshed.elapsed() >= std::time::Duration::from_secs(1) {
                let users: Vec<_> = self
                    .lists
                    .iter()
                    .filter(|user| !self.runtimes.contains_key(*user))
                    .cloned()
                    .collect();
                for user in users {
                    let _ = self.list(&user);
                }
                refreshed = std::time::Instant::now();
            }
            let event = match self.events.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("forward event channel closed"));
                }
            };
            if self.handle_event(event)? {
                return Ok(());
            }
        }
    }

    fn handle_event(&mut self, event: Event) -> io::Result<bool> {
        match event {
            Event::Client(Ok(ClientMsg::Detach)) | Event::Client(Err(_)) => return Ok(true),
            Event::Client(Ok(message)) => {
                seer_core::debug_log!(
                    "recv user={} client={} {}",
                    self.owner.user_id,
                    self.client_id,
                    seer_core::debug_log::client_summary(&message)
                );
                match self.client_event(message) {
                    Ok(ended) => return Ok(ended),
                    Err(error) => {
                        seer_core::debug_log!(
                            "refused user={} client={} reason={error}",
                            self.owner.user_id,
                            self.client_id
                        );
                        self.write(&ServerMsg::Refused {
                            reason: error.to_string(),
                        })?;
                    }
                }
            }
            Event::Runtime {
                user,
                identity,
                result,
            } => {
                if self
                    .runtimes
                    .get(&user)
                    .is_some_and(|runtime| Arc::ptr_eq(&runtime.identity, &identity))
                {
                    match result {
                        Ok(message) => self.handle_runtime(&user, message)?,
                        Err(_) => {
                            seer_core::debug_log!(
                                "runtime lost user={user} client={} error={:?}",
                                self.client_id,
                                result.as_ref().err()
                            );
                            self.close_runtime(&user)?;
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    fn client_event(&mut self, message: ClientMsg) -> io::Result<bool> {
        match message {
            ClientMsg::Stop => {
                self.broker.require_host(&self.owner.user_id)?;
                self.write(&ServerMsg::Bye {
                    reason: "server stopped".into(),
                })?;
                self.broker.stop();
                Ok(true)
            }
            ClientMsg::Leave => {
                self.broker.leave(&self.owner.user_id, &self.client_id)?;
                self.write(&ServerMsg::Bye {
                    reason: "left room".into(),
                })?;
                Ok(true)
            }
            message => self.handle_client(message).map(|()| false),
        }
    }

    fn close_runtime(&mut self, user: &str) -> io::Result<()> {
        if let Some(runtime) = self.runtimes.remove(user) {
            runtime.close()?;
        }
        Ok(())
    }

    fn remove_departed_runtimes(&mut self) -> io::Result<()> {
        let users: BTreeSet<_> = self.runtimes.keys().chain(&self.lists).cloned().collect();
        for user in users {
            if self.broker.registry().person(&user)?.is_some() {
                continue;
            }
            self.close_runtime(&user)?;
            self.lists.remove(&user);
            self.watches.retain(|(owner, _), _| owner != &user);
            self.write(&ServerMsg::Terminals {
                user,
                terminals: Vec::new(),
            })?;
        }
        Ok(())
    }

    fn handle_client(&mut self, message: ClientMsg) -> io::Result<()> {
        match message {
            ClientMsg::Watch {
                user,
                pane,
                cols,
                rows,
                viewer,
            } => self.watch(&user, &pane, PaneSize { cols, rows }, viewer),
            ClientMsg::Unwatch { user, pane } => {
                self.watches.remove(&(user.clone(), pane.clone()));
                if let Some(runtime) = self.runtimes.get_mut(&user) {
                    runtime.send(&ClientMsg::Unwatch { user, pane })?;
                }
                Ok(())
            }
            ClientMsg::Resync { user, pane } => {
                if let Some(runtime) = self.runtimes.get_mut(&user) {
                    runtime.send(&ClientMsg::Resync { user, pane })?;
                }
                Ok(())
            }
            ClientMsg::Terminals { user } => self.list(&user),
            ClientMsg::MouseInto { user, pane, mouse } => self.mouse_into(&user, &pane, mouse),
            ClientMsg::TypeInto { user, pane, bytes } => self.type_into(&user, &pane, bytes),
            ClientMsg::SetAllGrants { can_type } => self.set_all_grants(can_type),
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
                Err(io::Error::other(
                    "own terminal actions go to the local runtime, not the room",
                ))
            }
            _ => Err(io::Error::other("unsupported client message")),
        }
    }

    fn runtime(&mut self, user: &str) -> io::Result<&mut RuntimeConnection> {
        if !self.runtimes.contains_key(user) {
            let person = self.person(user)?;
            let mut runtime = RuntimeConnection::connect(self.broker, &person, &self.event_sender)?;
            for ((target, pane), (size, viewer)) in &self.watches {
                if target == user {
                    runtime.send(&ClientMsg::Watch {
                        user: user.into(),
                        pane: pane.clone(),
                        cols: size.cols,
                        rows: size.rows,
                        viewer: *viewer,
                    })?;
                }
            }
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

    fn watch(&mut self, user: &str, pane: &str, size: PaneSize, viewer: bool) -> io::Result<()> {
        if size.cols == 0 || size.rows == 0 {
            return Err(io::Error::other("terminal size must be positive"));
        }
        let runtime = self.runtime(user)?;
        runtime.location(pane)?;
        let frame = runtime.frames.get(pane).cloned();
        runtime.send(&ClientMsg::Watch {
            user: user.into(),
            pane: pane.into(),
            cols: size.cols,
            rows: size.rows,
            viewer,
        })?;
        self.watches
            .insert((user.to_owned(), pane.to_owned()), (size, viewer));
        // The cached screen is not numbered. The runtime answers the Watch
        // with a numbered screen, and the viewer counts from that one.
        if let Some(frame) = frame {
            self.write(&ServerMsg::Cells {
                user: user.into(),
                pane: pane.into(),
                frame,
                seq: 0,
            })?;
        }
        Ok(())
    }

    fn list(&mut self, user: &str) -> io::Result<()> {
        self.person(user)?;
        self.lists.insert(user.to_owned());
        let terminals = match self.runtime(user) {
            Ok(runtime) => runtime.terminals.clone(),
            Err(_) => return Ok(()),
        };
        self.write(&ServerMsg::Terminals {
            user: user.into(),
            terminals,
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
        let _ = &person;
        let mut stream = self.broker.runtimes().open(user)?;
        codec::encode(&mut stream, &ClientMsg::QueryTargets { user: user.into() })?;
        let ServerMsg::RuntimeReady { .. } = codec::decode(&mut stream)? else {
            return Err(io::Error::other("runtime did not report readiness"));
        };
        let message: ServerMsg = codec::decode(&mut stream)?;
        let _ = stream.shutdown(std::net::Shutdown::Both);
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
            }
            ServerMsg::Cells {
                pane, frame, seq, ..
            } => {
                runtime.frames.insert(pane.clone(), frame.clone());
                if self.watches.contains_key(&(user.into(), pane.clone())) {
                    self.write(&ServerMsg::Cells {
                        user: user.into(),
                        pane,
                        frame,
                        seq,
                    })?;
                }
            }
            ServerMsg::CellsDiff {
                user: _,
                pane,
                seq,
                diff,
            } => {
                let applied = runtime
                    .frames
                    .get(&pane)
                    .map(|held| seer_core::frame_diff::apply(held, &diff));
                match applied {
                    Some(Ok(frame)) => {
                        runtime.frames.insert(pane.clone(), frame);
                    }
                    _ => {
                        runtime.frames.remove(&pane);
                    }
                }
                if self.watches.contains_key(&(user.into(), pane.clone())) {
                    self.write(&ServerMsg::CellsDiff {
                        user: user.into(),
                        pane,
                        seq,
                        diff,
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
                seer_core::debug_log!(
                    "client={} listed={} {}",
                    self.client_id,
                    self.lists.contains(user),
                    seer_core::debug_log::server_summary(&ServerMsg::Terminals {
                        user: user.into(),
                        terminals: terminals.clone(),
                    })
                );
                if self.lists.contains(user) {
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
