use std::io;

use seer_core::proto::Person;

use crate::commands;
use crate::store::{ServerEntry, ServerStore};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RoomState {
    Attached,
    Detached,
    Unreachable,
}

impl RoomState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Attached => "attached",
            Self::Detached => "detached",
            Self::Unreachable => "unreachable",
        }
    }
}

#[derive(Clone)]
pub struct RoomSummary {
    pub server: ServerEntry,
    pub state: RoomState,
    pub people: Vec<Person>,
    pub host: bool,
}

impl RoomSummary {
    pub fn room_name(&self) -> &str {
        &self.server.alias
    }

    pub fn address(&self) -> &str {
        &self.server.endpoint
    }
}

pub fn saved_rooms() -> io::Result<Vec<RoomSummary>> {
    let mut servers = ServerStore::load()?.servers;
    servers.sort_by_key(|server| !server.current);
    Ok(servers.into_iter().map(inspect_room).collect())
}

pub fn inspect_room(server: ServerEntry) -> RoomSummary {
    let people = commands::people(&server);
    let state = match &people {
        Ok(people) => {
            if people
                .iter()
                .any(|person| person.user_id == server.user_id && person.attached_clients > 1)
            {
                RoomState::Attached
            } else {
                RoomState::Detached
            }
        }
        Err(_) => RoomState::Unreachable,
    };
    let host = match &people {
        Ok(people) => people
            .iter()
            .any(|person| person.user_id == server.user_id && person.host),
        Err(_) => configured_room(&server),
    };
    let people = people.unwrap_or_default();
    RoomSummary {
        server,
        state,
        people,
        host,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RoomAction {
    Start,
    Stop,
    Leave,
    Restore,
    StartNew,
}

impl RoomAction {
    pub fn run(self, server: Option<&ServerEntry>) -> io::Result<()> {
        match (self, server) {
            (Self::Start, _) => start_room(),
            (Self::Stop, Some(server)) => {
                commands::stop_selected_room(server).map_err(command_error)
            }
            (Self::Leave, Some(server)) => commands::leave_room(server).map_err(command_error),
            (Self::Restore | Self::StartNew, Some(server)) => {
                start_saved(server, self == Self::Restore)
            }
            (_, None) => Err(io::Error::other("No room is selected.")),
        }
    }
}

fn command_error(error: commands::CommandError) -> io::Error {
    io::Error::other(error.message)
}

#[cfg(target_os = "linux")]
fn configured_room(server: &ServerEntry) -> bool {
    crate::start::configured_room(&server.endpoint, &server.name)
}

#[cfg(target_os = "macos")]
fn configured_room(_: &ServerEntry) -> bool {
    false
}

#[cfg(target_os = "linux")]
fn start_saved(server: &ServerEntry, restore: bool) -> io::Result<()> {
    if !configured_room(server) {
        return Err(io::Error::other("This room is hosted on another computer."));
    }
    crate::start::run_linux(restore)
}

pub fn stopped_host(server: &ServerEntry) -> Option<RoomSummary> {
    if !configured_room(server) {
        return None;
    }
    let room = inspect_room(server.clone());
    (room.host && room.state == RoomState::Unreachable).then_some(room)
}

#[cfg(target_os = "macos")]
fn start_saved(_: &ServerEntry, _: bool) -> io::Result<()> {
    Err(io::Error::other("The room server runs on Linux only."))
}

#[cfg(target_os = "linux")]
fn start_room() -> io::Result<()> {
    crate::start::run_window()
}

#[cfg(target_os = "macos")]
fn start_room() -> io::Result<()> {
    Err(io::Error::other("The room server runs on Linux only."))
}
