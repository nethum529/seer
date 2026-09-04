use std::io::{self, BufRead, Write};

use seer_core::proto::{ClientInfo, PeekTarget};

use super::CommandError;
use crate::state;
use crate::store::{ServerEntry, ServerStore};

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

pub(super) fn select_target(targets: &[PeekTarget]) -> Result<PeekTarget, CommandError> {
    if let Some(target) = state::default_peek_target(targets) {
        return Ok(target);
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
    state::peek_target_at(targets, index)
        .ok_or_else(|| CommandError::usage("no target selected"))
}
