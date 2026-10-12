use std::io::{self, IsTerminal, Read};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientInfo, ClientMsg, Person, PersonState, ServerMsg, codec};
use seer_net::{Socket, Stream};

use crate::capsule;
use crate::capsule::Endpoint;
use crate::store::{ServerEntry, ServerStore};
use crate::tui;

pub(crate) use exit::exit;
pub(crate) use handshake::{Welcome, authenticate, hello};
pub(crate) use selection::{attach_bare, peek, selected_server};
mod exit;
mod handshake;
pub(crate) mod invitation;
pub(crate) mod join;
pub(crate) use join::run as join;
mod lifecycle;
pub(crate) use lifecycle::{leave, leave_room, perms, stop, stop_selected_room};
mod ps;
pub(crate) use ps::{PsAction, ps};
mod restart;
pub(crate) use restart::restart;
mod selection;
use selection::select_client;
const NETWORK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct CommandError {
    pub(crate) code: u8,
    pub(crate) message: String,
}

impl CommandError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: message.into(),
        }
    }

    fn system(error: impl std::fmt::Display) -> Self {
        Self {
            code: 2,
            message: format!("error: {error}"),
        }
    }
}

pub(crate) fn detach() -> Result<(), CommandError> {
    let server = selected_server()?;
    let (mut stream, _) = authenticate(&server)?;
    send(
        &mut stream,
        &ClientMsg::DetachClient {
            client_id: String::new(),
        },
    )?;
    let clients = receive_clients(&mut stream)?;
    let client = select_client(&clients)?;
    send(
        &mut stream,
        &ClientMsg::DetachClient {
            client_id: client.client_id,
        },
    )?;
    receive_clients(&mut stream)?;
    print_detached(&server.alias);
    Ok(())
}

pub(crate) fn attach() -> Result<(), CommandError> {
    let server = selected_server()?;
    println!("Attached to {} as {}.", server.alias, server.name);
    // Issue 338: the local terminals must open without waiting for the room.
    // A broker that accepts the connection and never answers would otherwise
    // hold up the window. The window reaches the room in the background.
    finish_session(io::stdout().is_terminal(), None, None, &server)
}

pub(crate) fn invite(hours: Option<&str>) -> Result<(), CommandError> {
    let hours = hours
        .map(|value| {
            value
                .parse::<u32>()
                .ok()
                .filter(|hours| (1..=168).contains(hours))
                .ok_or_else(|| CommandError::usage("hours must be from 1 to 168"))
        })
        .transpose()?;
    let server = selected_server()?;
    let invitation = invitation::create(&server, hours)?;
    let hours = invitation.expires_in_secs / 3_600;
    let unit = if hours == 1 { "hour" } else { "hours" };
    println!("Seat ready. It works once and expires in {hours} {unit}.");
    println!("Send this to a friend:");
    println!();
    println!("Has Seer? Paste this in Terminal:");
    println!("{}", invitation.join_line());
    println!();
    println!("No Seer yet? Paste this instead:");
    println!("{}", invitation.install_line());
    Ok(())
}

#[cfg(target_os = "linux")]
pub(crate) fn first_invite(first_start: bool) -> Result<(), CommandError> {
    if first_start { invite(None) } else { Ok(()) }
}

pub(crate) fn list() -> Result<(), CommandError> {
    let store = ServerStore::load().map_err(CommandError::system)?;
    let mut servers: Vec<&ServerEntry> = store.servers.iter().collect();
    servers.sort_by_key(|server| !server.current);
    let rows: Vec<Vec<String>> = servers.into_iter().map(list_server).collect();
    print_columns(&["SERVER", "YOU", "STATE", "PEOPLE"], &rows);
    Ok(())
}

fn list_server(server: &ServerEntry) -> Vec<String> {
    let room = crate::rooms::inspect_room(server.clone());
    let mut people: Vec<_> = room.people.iter().collect();
    people.sort_unstable_by_key(|person| person.name.to_ascii_lowercase());
    vec![
        server.alias.clone(),
        server.name.clone(),
        room.state.label().to_owned(),
        people
            .into_iter()
            .map(describe_person)
            .collect::<Vec<_>>()
            .join(", "),
    ]
}

fn describe_person(person: &Person) -> String {
    let state = match person.state {
        PersonState::Active => "active",
        PersonState::Idle => "idle",
        PersonState::Away => "away",
    };
    let foreground = if person.foreground.is_empty() {
        "-"
    } else {
        person.foreground.as_str()
    };
    let (name, tabs, idle) = (&person.name, person.tabs, person.idle_secs);
    format!("{name} {state} {tabs} {foreground} {idle}s")
}

fn receive_clients(stream: &mut impl Stream) -> Result<Vec<ClientInfo>, CommandError> {
    match receive_reply(stream)? {
        ServerMsg::Clients { clients } => Ok(clients),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}

fn print_detached(alias: &str) {
    println!("Detached from {alias}. Your panes are still running.");
}
fn print_server_stopped() {
    println!("Server stopped.");
}

fn print_close_names(target: &str, people: &[Person]) {
    let mut names: Vec<&str> = people
        .iter()
        .filter(|person| is_close(target, &person.name))
        .map(|person| person.name.as_str())
        .collect();
    names.sort_unstable_by_key(|name| name.to_ascii_lowercase());
    if !names.is_empty() {
        eprintln!("Close names: {}", names.join(", "));
    }
}

fn is_close(target: &str, candidate: &str) -> bool {
    let target = target.to_ascii_lowercase();
    let candidate = candidate.to_ascii_lowercase();
    candidate.starts_with(&target)
        || target.starts_with(&candidate)
        || edit_distance_at_most_one(target.as_bytes(), candidate.as_bytes())
}

fn edit_distance_at_most_one(left: &[u8], right: &[u8]) -> bool {
    if left.len().abs_diff(right.len()) > 1 {
        return false;
    }
    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    let mut differences = 0;
    let mut short_index = 0;
    let mut long_index = 0;
    while short_index < shorter.len() {
        if shorter[short_index] == longer[long_index] {
            short_index += 1;
        } else {
            differences += 1;
            if differences == 2 {
                return false;
            }
            if shorter.len() == longer.len() {
                short_index += 1;
            }
        }
        long_index += 1;
    }
    differences == 0 || long_index == longer.len()
}

pub(crate) fn people(server: &ServerEntry) -> Result<Vec<Person>, CommandError> {
    let (mut stream, _) = authenticate(server)?;
    send(&mut stream, &ClientMsg::ListPeople)?;
    people_reply(receive_reply(&mut stream)?)
}

fn people_reply(reply: ServerMsg) -> Result<Vec<Person>, CommandError> {
    match reply {
        ServerMsg::People { people } => Ok(people),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}

fn connect(endpoint: &str) -> Result<Socket, CommandError> {
    let parsed = capsule::parse_endpoint(endpoint).ok_or_else(tcp_failure)?;
    let stream = match parsed {
        Endpoint::Tcp(address) => {
            Socket::from(TcpStream::connect(&address).map_err(|_| tcp_failure())?)
        }
        Endpoint::Iroh(id) => connect_iroh(&id)?,
    };
    stream
        .set_read_timeout(Some(NETWORK_TIMEOUT))
        .map_err(CommandError::system)?;
    stream
        .set_write_timeout(Some(NETWORK_TIMEOUT))
        .map_err(CommandError::system)?;
    Ok(stream)
}

fn connect_iroh(id: &str) -> Result<Socket, CommandError> {
    let directory = crate::store::config_dir().map_err(CommandError::system)?;
    let secret_key =
        crate::store::ServerStore::load_or_create_device_key(&directory.join("device.key"))
            .map_err(CommandError::system)?;
    let endpoint_id = seer_net::decode_endpoint_id(id).map_err(CommandError::system)?;
    seer_net::dial(secret_key, endpoint_id)
        .map(Socket::from)
        .map_err(|_| {
            CommandError::usage(
                "Cannot reach the server. Check that the owner ran seer start and that you both have internet.",
            )
        })
}

fn tcp_failure() -> CommandError {
    CommandError::usage(
        "Cannot reach the server. Check that the server is running and that you are on the same network.",
    )
}

fn send(stream: &mut impl Stream, message: &ClientMsg) -> Result<(), CommandError> {
    codec::encode(stream, message).map_err(CommandError::system)
}

fn receive(stream: &mut impl Stream) -> Result<ServerMsg, CommandError> {
    codec::decode(stream).map_err(CommandError::system)
}

fn receive_reply(stream: &mut impl Stream) -> Result<ServerMsg, CommandError> {
    receive_reply_before(stream, Instant::now() + NETWORK_TIMEOUT).map_err(CommandError::system)
}

fn receive_reply_before<S: Stream>(stream: &mut S, deadline: Instant) -> io::Result<ServerMsg> {
    loop {
        let reply = codec::decode(&mut DeadlineReader { stream, deadline })?;
        if !matches!(
            reply,
            ServerMsg::Tree { .. }
                | ServerMsg::Frame { .. }
                | ServerMsg::Cells { .. }
                | ServerMsg::Grants { .. }
                | ServerMsg::Presence { .. }
                | ServerMsg::Terminals { .. }
        ) {
            return Ok(reply);
        }
    }
}

struct DeadlineReader<'a, S> {
    stream: &'a mut S,
    deadline: Instant,
}

impl<S: Stream> Read for DeadlineReader<'_, S> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "command reply timed out"))?;
        self.stream.set_read_timeout(Some(remaining))?;
        self.stream.read(buffer)
    }
}

/// Runs the window on both routes.
///
/// The local runtime is reached first and the room is optional: the window
/// must render and type in its own terminals whether or not the room answers.
fn finish_session(
    terminal: bool,
    room: Option<Socket>,
    peek_person: Option<&str>,
    server: &ServerEntry,
) -> Result<(), CommandError> {
    if !terminal {
        return Ok(());
    }
    #[cfg(debug_assertions)]
    if let Ok(directory) = crate::local::runtime_directory(&server.user_id) {
        seer_core::debug_log::open(&directory, "client", &server.user_id);
    }
    let (local, tree, standing_notice) =
        crate::local::attach(server).map_err(CommandError::system)?;
    if let Some(room) = &room {
        crate::tui_link::prepare_room(room).map_err(CommandError::system)?;
    }
    tui::set_peek_person(peek_person);
    let exit = tui::run(local, room, tree, server.clone(), standing_notice)
        .map_err(CommandError::system)?;
    match exit {
        tui::SessionExit::Detached => print_detached(&server.alias),
        tui::SessionExit::ServerStopped => print_server_stopped(),
        tui::SessionExit::Restarted => restart::print_restarted(),
        tui::SessionExit::LocalLinkLost if crate::local::runtime_answers(&server.user_id) => {
            println!(
                "Your terminals on this computer still run. Run: seer restart. The restart ends the running shells."
            );
        }
        tui::SessionExit::LocalLinkLost => print_server_stopped(),
        tui::SessionExit::Client | tui::SessionExit::TerminalLost => {}
    }
    Ok(())
}

pub(crate) fn join_line(capsule: &str) -> String {
    format!("seer join {capsule}")
}

fn unexpected_reply() -> CommandError {
    CommandError::system("unexpected server reply")
}

// The last column is not padded, so no line ends with spaces.
fn print_columns(header: &[&str], rows: &[Vec<String>]) {
    let widths: Vec<usize> = header
        .iter()
        .enumerate()
        .map(|(column, title)| {
            rows.iter()
                .map(|row| row.get(column).map_or(0, |text| text.chars().count()))
                .max()
                .unwrap_or(0)
                .max(title.chars().count())
        })
        .collect();
    let line = |cells: &[&str]| {
        widths
            .iter()
            .enumerate()
            .map(|(column, width)| {
                let text = cells.get(column).copied().unwrap_or("");
                if column + 1 == widths.len() {
                    text.to_owned()
                } else {
                    format!("{text:<width$}")
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    };
    println!("{}", line(header));
    for row in rows {
        let cells: Vec<&str> = row.iter().map(String::as_str).collect();
        println!("{}", line(&cells));
    }
}
