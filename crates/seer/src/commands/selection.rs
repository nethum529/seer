use std::io::{self, BufRead, IsTerminal, Write};

use seer_core::Tree;
use seer_core::proto::{ClientInfo, ClientMsg, PeekTarget, ServerMsg};

use crate::peek_mode::unique_target;
use crate::store::ServerStore;

use super::{
    CommandError, ServerEntry, authenticate, finish_session, people_reply, print_close_names,
    receive, receive_reply, send, unexpected_reply,
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

pub(super) fn selected_server() -> Result<ServerEntry, CommandError> {
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
    let tree = peek_reply(receive(&mut stream)?)?;
    println!("PEEK: {} - READ ONLY", person.name);
    println!("Workspace: {}/{}", person.name, selected.workspace);
    finish_session(
        io::stdout().is_terminal(),
        stream,
        tree,
        Some(&person.name),
        &server.alias,
        crate::tui::run,
    )
}

fn targets_reply(reply: ServerMsg) -> Result<Vec<PeekTarget>, CommandError> {
    match reply {
        ServerMsg::Targets { targets } => Ok(targets),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}

fn peek_reply(reply: ServerMsg) -> Result<Tree, CommandError> {
    match reply {
        ServerMsg::Tree { tree } => Ok(tree),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}

fn select_target(targets: &[PeekTarget]) -> Result<PeekTarget, CommandError> {
    if let Some(target) = unique_target(targets) {
        return Ok(target.clone());
    }
    if targets.is_empty() {
        return Err(CommandError::usage("no active target"));
    }
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    pick_target(targets, &mut input, &mut output)
}

fn pick_target(
    targets: &[PeekTarget],
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<PeekTarget, CommandError> {
    writeln!(output, "Select a target:").map_err(CommandError::system)?;
    for (index, target) in targets.iter().enumerate() {
        writeln!(
            output,
            "  {}. {}/{}",
            index + 1,
            target.workspace_name,
            target.tab_title
        )
        .map_err(CommandError::system)?;
    }
    write!(output, "Target: ").map_err(CommandError::system)?;
    output.flush().map_err(CommandError::system)?;
    let mut selection = String::new();
    if input
        .read_line(&mut selection)
        .map_err(CommandError::system)?
        == 0
    {
        return Err(CommandError::usage("no target selected"));
    }
    let index = selection
        .trim()
        .parse::<usize>()
        .map_err(|_| CommandError::usage("no target selected"))?;
    targets
        .get(
            index
                .checked_sub(1)
                .ok_or_else(|| CommandError::usage("no target selected"))?,
        )
        .cloned()
        .ok_or_else(|| CommandError::usage("no target selected"))
}
