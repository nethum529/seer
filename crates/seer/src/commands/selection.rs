use std::io::{self, BufRead, IsTerminal, Write};

use seer_core::proto::{ClientInfo, ClientMsg};

use crate::store::ServerStore;

use super::{
    CommandError, ServerEntry, authenticate, finish_session, people_reply, print_close_names,
    receive_reply, send,
};

pub(super) fn select_client(clients: &[ClientInfo]) -> Result<ClientInfo, CommandError> {
    match clients {
        [] => Err(CommandError::usage("no attached client")),
        [client] => Ok(client.clone()),
        _ => {
            let stdin = io::stdin();
            let mut input = stdin.lock();
            let stdout = io::stdout();
            let mut output = stdout.lock();
            pick_client(clients, &mut input, &mut output)
        }
    }
}

fn pick_client(
    clients: &[ClientInfo],
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<ClientInfo, CommandError> {
    writeln!(output, "Select a client:").map_err(CommandError::system)?;
    for (index, client) in clients.iter().enumerate() {
        writeln!(
            output,
            "  {}. {} (connected {} seconds)",
            index + 1,
            client.client_id,
            client.connected_secs
        )
        .map_err(CommandError::system)?;
    }
    write!(output, "Client: ").map_err(CommandError::system)?;
    output.flush().map_err(CommandError::system)?;
    let mut selection = String::new();
    if input
        .read_line(&mut selection)
        .map_err(CommandError::system)?
        == 0
    {
        return Err(CommandError::usage("no client selected"));
    }
    let index = selection
        .trim()
        .parse::<usize>()
        .map_err(|_| CommandError::usage("no client selected"))?;
    index
        .checked_sub(1)
        .and_then(|index| clients.get(index))
        .cloned()
        .ok_or_else(|| CommandError::usage("no client selected"))
}

pub(crate) fn selected_server() -> Result<ServerEntry, CommandError> {
    let store = ServerStore::load().map_err(CommandError::system)?;
    if store.servers.is_empty() {
        return Err(CommandError::usage("run seer join first"));
    }
    if store.servers.len() == 1 {
        return Ok(store.servers[0].clone());
    }
    if let Some(server) = store.servers.iter().find(|server| server.current) {
        return Ok(server.clone());
    }
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    pick_server(&store.servers, &mut input, &mut output)
}

fn pick_server(
    servers: &[ServerEntry],
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<ServerEntry, CommandError> {
    writeln!(output, "Select a server:").map_err(CommandError::system)?;
    for (index, server) in servers.iter().enumerate() {
        writeln!(
            output,
            "  {}. {} ({})",
            index + 1,
            server.alias,
            server.name
        )
        .map_err(CommandError::system)?;
    }
    write!(output, "Server: ").map_err(CommandError::system)?;
    output.flush().map_err(CommandError::system)?;
    let mut selection = String::new();
    if input
        .read_line(&mut selection)
        .map_err(CommandError::system)?
        == 0
    {
        return Err(CommandError::usage("no server selected"));
    }
    let index = selection
        .trim()
        .parse::<usize>()
        .map_err(|_| CommandError::usage("no server selected"))?;
    index
        .checked_sub(1)
        .and_then(|index| servers.get(index))
        .cloned()
        .ok_or_else(|| CommandError::usage("no server selected"))
}

pub(crate) fn peek(target: &str) -> Result<(), CommandError> {
    let server = selected_server()?;
    let (mut stream, _) = authenticate(&server)?;
    send(&mut stream, &ClientMsg::ListPeople)?;
    let people = people_reply(receive_reply(&mut stream)?)?;
    let Some(person) = people.iter().find(|person| person.name == target) else {
        print_close_names(target, &people);
        return Err(CommandError::usage(format!("no person named {target}")));
    };
    drop(stream);
    let name = person.name.clone();
    let (stream, _) = authenticate(&server)?;
    finish_session(
        io::stdout().is_terminal(),
        Some(stream),
        Some(&name),
        &server,
    )
}

pub(crate) fn attach_bare() -> Result<(), CommandError> {
    if ServerStore::load()
        .map_err(CommandError::system)?
        .servers
        .is_empty()
    {
        crate::start::restore_owner().map_err(CommandError::system)?;
    }
    super::attach()
}
