use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use seer::ServerEntry;
use seer_core::proto::{AgentState, ClientMsg, Person, PersonState, ServerMsg, TerminalInfo};

use crate::link::frames;
use crate::link::{Messages, Terminal, lock};
use transport::RoomEvent;

pub(crate) mod transport;

pub(crate) struct Room {
    own: String,
    can_type_here: BTreeSet<String>,
    you_may_type_into: BTreeSet<String>,
    confirmed: bool,
    pending: Option<(String, bool)>,
    pub(crate) control_error: Option<&'static str>,
    pub(crate) people: Vec<Person>,
    pub(crate) selected: String,
    pub(crate) pane: Option<String>,
    pub(crate) terminal: Terminal,
    lists: HashMap<String, Vec<TerminalInfo>>,
    writer: Option<Sender<ClientMsg>>,
    stopped: Arc<AtomicBool>,
    size: Option<(u16, u16)>,
    resyncing: bool,
    start_person: Option<String>,
}

impl Room {
    pub(crate) fn new(own: String) -> Self {
        let person = Person {
            user_id: own.clone(),
            name: own.clone(),
            online: true,
            idle_secs: 0,
            attached_clients: 1,
            peekable: true,
            host: false,
            state: PersonState::Active,
            tabs: 0,
            foreground: String::new(),
        };
        Self {
            selected: own.clone(),
            own,
            can_type_here: BTreeSet::new(),
            you_may_type_into: BTreeSet::new(),
            confirmed: false,
            pending: None,
            control_error: None,
            people: vec![person],
            pane: None,
            terminal: Terminal::default(),
            lists: HashMap::new(),
            writer: None,
            stopped: Arc::default(),
            size: None,
            resyncing: false,
            start_person: None,
        }
    }

    pub(crate) fn join(&mut self, server: ServerEntry, person: Option<String>) -> Messages {
        if let Some(own) = self.people.first_mut() {
            own.name.clone_from(&server.name);
        }
        self.start_person = person;
        transport::start(server, Arc::clone(&self.stopped))
    }

    pub(crate) fn remote(&self) -> bool {
        self.selected != self.own
    }

    pub(crate) fn can_input(&self) -> bool {
        self.writer.is_some()
            && self.pane.is_some()
            && self.you_may_type_into.contains(&self.selected)
    }

    pub(crate) fn shares_control(&self, user: &str) -> bool {
        self.can_type_here.contains(user)
    }

    pub(crate) fn can_share(&self) -> bool {
        self.writer.is_some() && self.confirmed && self.pending.is_none()
    }

    pub(crate) fn share_control(&mut self, user: &str) {
        if user == self.own || !self.can_share() {
            return;
        }
        let can_type = !self.shares_control(user);
        self.control_error = None;
        if self.writer.as_ref().is_some_and(|writer| {
            writer
                .send(ClientMsg::SetGrant {
                    user: user.into(),
                    can_type,
                })
                .is_ok()
        }) {
            self.pending = Some((user.into(), can_type));
        } else {
            self.control_error = Some("Room is offline.");
        }
    }

    pub(crate) fn input(&self, event: seer_core::InputEvent) {
        if !self.can_input() || matches!(event, seer_core::InputEvent::Scrollback { .. }) {
            return;
        }
        if let Some(pane) = &self.pane {
            self.send(ClientMsg::InputInto {
                user: self.selected.clone(),
                pane: pane.clone(),
                input: seer_core::TerminalInput::new(event),
            });
        }
    }

    pub(crate) fn person(&self) -> Option<&Person> {
        self.people
            .iter()
            .find(|person| person.user_id == self.selected)
    }

    pub(crate) fn terminals(&self) -> &[TerminalInfo] {
        self.lists.get(&self.selected).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn activity(&self, user: &str) -> (bool, usize) {
        let terminals = self.lists.get(user).map_or(&[][..], Vec::as_slice);
        (
            terminals
                .iter()
                .any(|terminal| terminal.agent_state == Some(AgentState::Waiting)),
            terminals
                .iter()
                .filter(|terminal| terminal.agent_state == Some(AgentState::Working))
                .count(),
        )
    }

    pub(crate) fn select_person(&mut self, user: &str) {
        if self.selected == user || !self.people.iter().any(|person| person.user_id == user) {
            return;
        }
        self.unwatch();
        self.selected = user.into();
        self.pane = None;
        self.clear();
        self.retarget();
    }

    pub(crate) fn select_pane(&mut self, pane: &str) {
        if self.pane.as_deref() == Some(pane)
            || !self
                .terminals()
                .iter()
                .any(|terminal| terminal.pane == pane)
        {
            return;
        }
        self.unwatch();
        self.pane = Some(pane.into());
        self.clear();
        self.watch();
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 || self.size == Some((cols, rows)) {
            return;
        }
        self.size = Some((cols, rows));
        self.watch();
    }

    pub(crate) fn receive(&mut self, event: RoomEvent) {
        match event {
            RoomEvent::Connected(writer) => {
                self.writer = Some(writer);
                self.control_error = None;
                self.send(ClientMsg::ListPeople);
            }
            RoomEvent::Message(message) => self.message(message),
            RoomEvent::Lost | RoomEvent::Stopped => {
                self.writer = None;
                self.can_type_here.clear();
                self.you_may_type_into.clear();
                self.confirmed = false;
                self.pending = None;
                self.control_error = Some("Room is offline.");
                for person in &mut self.people {
                    if person.user_id != self.own {
                        person.online = false;
                        person.peekable = false;
                    }
                }
                self.lists.clear();
                self.pane = None;
                self.clear();
            }
        }
    }

    fn message(&mut self, message: ServerMsg) {
        match message {
            ServerMsg::People { people } => self.note_people(people),
            ServerMsg::Grants {
                can_type_here,
                you_may_type_into,
            } => {
                self.can_type_here = can_type_here.into_iter().collect();
                self.you_may_type_into = you_may_type_into.into_iter().collect();
                self.confirmed = true;
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|(user, can_type)| self.shares_control(user) == *can_type)
                {
                    self.pending = None;
                }
            }
            ServerMsg::Refused { .. } if self.pending.take().is_some() => {
                self.control_error = Some("Share control was refused.");
            }
            ServerMsg::Presence {
                user,
                online,
                idle_secs,
            } => {
                if let Some(person) = self.people.iter_mut().find(|person| person.user_id == user) {
                    person.online = online;
                    person.idle_secs = idle_secs;
                }
            }
            ServerMsg::Terminals { user, terminals } => {
                self.lists.insert(user.clone(), terminals);
                if user == self.selected {
                    self.retarget();
                }
            }
            ServerMsg::Cells {
                user,
                pane,
                frame,
                seq,
            } if self.shows(&user, &pane) => {
                frames::replace(&self.terminal, frame, seq);
                self.resyncing = false;
            }
            ServerMsg::CellsDiff {
                user,
                pane,
                seq,
                diff,
            } if self.shows(&user, &pane)
                && !self.resyncing
                && !frames::apply(&self.terminal, seq, &diff) =>
            {
                self.resyncing = true;
                self.send(ClientMsg::Resync { user, pane });
            }
            _ => {}
        }
    }

    fn note_people(&mut self, mut people: Vec<Person>) {
        if !people.iter().any(|person| person.user_id == self.own)
            && let Some(own) = self
                .people
                .iter()
                .find(|person| person.user_id == self.own)
                .cloned()
        {
            people.insert(0, own);
        }
        people.sort_by_key(|person| person.user_id != self.own);
        for person in &people {
            if person.user_id == self.own {
                continue;
            }
            if person.peekable && !self.lists.contains_key(&person.user_id) {
                self.send(ClientMsg::Terminals {
                    user: person.user_id.clone(),
                });
            } else if !person.peekable {
                self.lists.remove(&person.user_id);
            }
        }
        self.lists
            .retain(|user, _| people.iter().any(|person| &person.user_id == user));
        self.people = people;
        if let Some(name) = self.start_person.as_ref()
            && let Some(person) = self.people.iter().find(|person| &person.name == name)
        {
            let user = person.user_id.clone();
            self.start_person = None;
            self.select_person(&user);
        }
        if !self
            .people
            .iter()
            .any(|person| person.user_id == self.selected)
        {
            let own = self.own.clone();
            self.select_person(&own);
        }
        self.retarget();
    }

    fn retarget(&mut self) {
        if !self.remote() {
            return;
        }
        if self.pane.as_ref().is_some_and(|pane| {
            self.terminals()
                .iter()
                .any(|terminal| &terminal.pane == pane)
        }) {
            return;
        }
        self.unwatch();
        self.pane = self
            .terminals()
            .first()
            .map(|terminal| terminal.pane.clone());
        self.clear();
        self.watch();
    }

    fn shows(&self, user: &str, pane: &str) -> bool {
        self.remote() && user == self.selected && self.pane.as_deref() == Some(pane)
    }

    fn clear(&mut self) {
        *lock(&self.terminal) = None;
        self.resyncing = false;
    }

    fn unwatch(&self) {
        if let Some(pane) = &self.pane {
            self.send(ClientMsg::Unwatch {
                user: self.selected.clone(),
                pane: pane.clone(),
            });
        }
    }

    fn watch(&self) {
        if self.remote()
            && let (Some(pane), Some((cols, rows))) = (&self.pane, self.size)
        {
            self.send(ClientMsg::Watch {
                user: self.selected.clone(),
                pane: pane.clone(),
                cols,
                rows,
                viewer: true,
            });
        }
    }

    fn send(&self, message: ClientMsg) {
        if let Some(writer) = &self.writer {
            let _ = writer.send(message);
        }
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}
