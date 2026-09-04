use std::io::{self, BufRead, IsTerminal, Write};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, PeekTarget, Person, ServerMsg};
use seer_net::{Socket, Stream};

use super::{
    CommandError, authenticate, print_detached, receive, receive_reply, selected_server, send,
    unexpected_reply,
};
use crate::tui;

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

pub(super) fn people_reply(reply: ServerMsg) -> Result<Vec<Person>, CommandError> {
    match reply {
        ServerMsg::People { people } => Ok(people),
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

pub(super) fn finish_session(
    terminal: bool,
    stream: Socket,
    tree: Tree,
    peek_person: Option<&str>,
    alias: &str,
    run: impl FnOnce(Socket, Tree) -> io::Result<tui::SessionExit>,
) -> Result<(), CommandError> {
    if !terminal {
        return Ok(());
    }
    stream
        .set_read_timeout(None)
        .map_err(CommandError::system)?;
    tui::set_peek_person(peek_person);
    let exit = run(stream, tree).map_err(CommandError::system)?;
    if exit == tui::SessionExit::Detached {
        print_detached(alias);
    }
    Ok(())
}

fn select_target(targets: &[PeekTarget]) -> Result<PeekTarget, CommandError> {
    let active: Vec<_> = targets.iter().filter(|target| target.active).collect();
    match active.as_slice() {
        [target] => return Ok((*target).clone()),
        [] if targets.len() == 1 => return Ok(targets[0].clone()),
        _ => {}
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

pub(super) fn is_close(target: &str, candidate: &str) -> bool {
    let target = target.to_ascii_lowercase();
    let candidate = candidate.to_ascii_lowercase();
    candidate.starts_with(&target)
        || target.starts_with(&candidate)
        || edit_distance_at_most_one(target.as_bytes(), candidate.as_bytes())
}

pub(super) fn edit_distance_at_most_one(left: &[u8], right: &[u8]) -> bool {
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
