#[path = "commands/command_support.rs"]
mod command_support;
#[path = "commands/selection.rs"]
mod selection;

use command_support::{
    authenticate, connect, finish_session, people, people_reply, receive, receive_clients,
    receive_reply, send,
};
pub(super) use command_support::receive_reply_before;
use selection::{select_client, select_target, selected_server};
use std::io::{self, IsTerminal};
use std::time::Duration;

use seer_core::proto::{ClientMsg, PeekTarget, Person, ServerMsg};

use crate::capsule;
use crate::prompt;
use crate::store::{ServerEntry, ServerStore};
use crate::tui;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(5);
const INSTALL_URL: &str =
    "https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh";

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
    print_detached(&server.alias);
    Ok(())
}

pub(crate) fn join(invitation: Option<&str>) -> Result<(), CommandError> {
    let invitation = match invitation {
        Some(invitation) => invitation.to_owned(),
        None => prompt::hidden("Invitation: ").map_err(CommandError::system)?,
    };
    let capsule = capsule::parse(&invitation).map_err(CommandError::system)?;
    println!("Server: {}", capsule.endpoint);
    let endpoint = capsule.endpoint.to_string();

    let default_name = std::env::var("USER")
        .ok()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            std::env::var("LOGNAME")
                .ok()
                .filter(|name| !name.is_empty())
        });

    loop {
        let name = prompt::visible_with_default("Name", default_name.as_deref())
            .map_err(CommandError::system)?;
        let mut stream = connect(&endpoint)?;
        let join = ClientMsg::Join {
            seat_token: capsule.token.clone(),
            name,
        };
        send(&mut stream, &join)?;
        match receive(&mut stream)? {
            ServerMsg::Refused { reason } if reason == "name is in use" => {
                println!("That name is in use.");
            }
            ServerMsg::Joined {
                user_id,
                credential,
                name,
            } => return complete_join(&capsule, user_id, credential, name),
            ServerMsg::Refused { reason } => return Err(CommandError::usage(reason)),
            _ => return Err(unexpected_reply()),
        }
    }
}

fn complete_join(
    capsule: &capsule::Capsule,
    user_id: String,
    credential: String,
    name: String,
) -> Result<(), CommandError> {
    let mut store = ServerStore::load().map_err(CommandError::system)?;
    let server = ServerEntry {
        endpoint: capsule.endpoint.to_string(),
        alias: capsule.alias.clone(),
        user_id,
        name: name.clone(),
        credential,
        current: true,
    };
    store.make_current(server.clone());
    store.save().map_err(CommandError::system)?;
    println!("Joined as {name}. Attaching...");
    let (stream, tree) = authenticate(&server)?;
    finish_session(
        io::stdout().is_terminal(),
        stream,
        tree,
        None,
        &capsule.alias,
        tui::run,
    )
}

pub(crate) fn attach() -> Result<(), CommandError> {
    let server = selected_server()?;
    let (stream, tree) = authenticate(&server)?;
    println!("Attached to {} as {}.", server.alias, server.name);
    finish_session(
        io::stdout().is_terminal(),
        stream,
        tree,
        None,
        &server.alias,
        tui::run,
    )
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
    let (mut stream, _) = authenticate(&server)?;
    send(&mut stream, &ClientMsg::Invite { hours })?;
    match receive_reply(&mut stream)? {
        ServerMsg::Seat {
            capsule,
            expires_in_secs,
        } => {
            let hours = expires_in_secs / 3_600;
            let unit = if hours == 1 { "hour" } else { "hours" };
            println!("Seat ready. It works once and expires in {hours} {unit}.");
            println!("Send this to a friend:");
            println!();
            println!("Paste this in Terminal:");
            println!("curl -fsSL {INSTALL_URL} | sh -s -- {capsule}");
            Ok(())
        }
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn first_invite(first_start: bool) -> Result<(), CommandError> {
    if first_start { invite(None) } else { Ok(()) }
}

pub(crate) fn list() -> Result<(), CommandError> {
    let store = ServerStore::load().map_err(CommandError::system)?;
    let mut servers: Vec<&ServerEntry> = store.servers.iter().collect();
    servers.sort_by_key(|server| !server.current);
    let rows: Vec<ListRow> = servers.into_iter().map(list_server).collect();
    print_table(&rows);
    Ok(())
}

fn list_server(server: &ServerEntry) -> ListRow {
    match people(server) {
        Ok(people) => {
            let attached = people
                .iter()
                .find(|person| person.user_id == server.user_id)
                .is_some_and(|person| person.attached_clients > 1);
            let mut names: Vec<&str> = people.iter().map(|person| person.name.as_str()).collect();
            names.sort_unstable_by_key(|name| name.to_ascii_lowercase());
            ListRow {
                server: server.alias.clone(),
                you: server.name.clone(),
                state: if attached { "attached" } else { "detached" },
                people: names.join(", "),
            }
        }
        Err(_) => ListRow {
            server: server.alias.clone(),
            you: server.name.clone(),
            state: "unreachable",
            people: String::new(),
        },
    }
}

pub(crate) fn peek(target: &str) -> Result<(), CommandError> {
    let server = selected_server()?;
    let (mut stream, tree) = authenticate(&server)?;
    send(&mut stream, &ClientMsg::ListPeople)?;
    let people = people_reply(receive_reply(&mut stream)?)?;
    let Some(person) = people.iter().find(|person| person.name == target) else {
        print_close_names(target, &people);
        return Err(CommandError::usage(format!("no person named {target}")));
    };
    send(
        &mut stream,
        &ClientMsg::QueryTargets {
            user: person.user_id.clone(),
        },
    )?;
    let targets = targets_reply(receive_reply(&mut stream)?)?;
    let selected = select_target(&targets)?;
    send(
        &mut stream,
        &ClientMsg::Peek {
            user: person.user_id.clone(),
            workspace: selected.workspace.clone(),
            tab: selected.tab.clone(),
        },
    )?;
    println!("PEEK: {} - READ ONLY", person.name);
    println!("Workspace: {}/{}", person.name, selected.workspace);
    finish_session(
        io::stdout().is_terminal(),
        stream,
        tree,
        Some(&person.name),
        &server.alias,
        tui::run,
    )
}

fn targets_reply(reply: ServerMsg) -> Result<Vec<PeekTarget>, CommandError> {
    match reply {
        ServerMsg::Targets { targets } => Ok(targets),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}



fn unexpected_reply() -> CommandError {
    CommandError::system("unexpected server reply")
}

struct ListRow {
    server: String,
    you: String,
    state: &'static str,
    people: String,
}

fn print_table(rows: &[ListRow]) {
    let server_width = rows
        .iter()
        .map(|row| row.server.len())
        .max()
        .unwrap_or(0)
        .max("SERVER".len());
    let you_width = rows
        .iter()
        .map(|row| row.you.len())
        .max()
        .unwrap_or(0)
        .max("YOU".len());
    let state_width = rows
        .iter()
        .map(|row| row.state.len())
        .max()
        .unwrap_or(0)
        .max("STATE".len());
    println!(
        "{:<server_width$}  {:<you_width$}  {:<state_width$}  PEOPLE",
        "SERVER", "YOU", "STATE"
    );
    for row in rows {
        println!(
            "{:<server_width$}  {:<you_width$}  {:<state_width$}  {}",
            row.server, row.you, row.state, row.people
        );
    }
}

#[cfg(test)]
mod tests;
