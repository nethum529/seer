use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientInfo, ClientMsg, Person, ServerMsg, codec};
use seer_net::{Socket, Stream};

use crate::capsule;
use crate::capsule::Endpoint;
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
    let peek = ClientMsg::Peek {
        user: person.user_id.clone(),
        workspace: "w1".into(),
        tab: "w1:t1".into(),
    };
    send(&mut stream, &peek)?;
    println!("PEEK: {} - READ ONLY", person.name);
    println!("Workspace: {}/w1", person.name);
    finish_session(
        io::stdout().is_terminal(),
        stream,
        tree,
        Some(&person.name),
        &server.alias,
        tui::run,
    )
}

fn receive_clients(stream: &mut impl Stream) -> Result<Vec<ClientInfo>, CommandError> {
    match receive_reply(stream)? {
        ServerMsg::Clients { clients } => Ok(clients),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}

fn select_client(clients: &[ClientInfo]) -> Result<ClientInfo, CommandError> {
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

fn print_detached(alias: &str) {
    println!("Detached from {alias}. Your panes are still running.");
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

fn selected_server() -> Result<ServerEntry, CommandError> {
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

fn authenticate(server: &ServerEntry) -> Result<(Socket, Tree), CommandError> {
    let mut stream = connect(&server.endpoint)?;
    let hello = ClientMsg::Hello {
        user_id: server.user_id.clone(),
        credential: server.credential.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    send(&mut stream, &hello)?;
    let tree = welcome_tree(receive_reply(&mut stream)?)?;
    Ok((stream, tree))
}

fn people(server: &ServerEntry) -> Result<Vec<Person>, CommandError> {
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

fn welcome_tree(reply: ServerMsg) -> Result<Tree, CommandError> {
    match reply {
        ServerMsg::Welcome { tree, .. } => Ok(tree),
        ServerMsg::Refused { reason } => Err(CommandError::usage(format!("refused: {reason}"))),
        _ => Err(unexpected_reply()),
    }
}

fn connect(endpoint: &str) -> Result<Socket, CommandError> {
    let parsed = capsule::parse_endpoint(endpoint).ok_or_else(|| tcp_failure(endpoint))?;
    let stream = match parsed {
        Endpoint::Tcp(address) => {
            Socket::from(TcpStream::connect(&address).map_err(|_| tcp_failure(endpoint))?)
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
    std::fs::create_dir_all(&directory).map_err(CommandError::system)?;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
        .map_err(CommandError::system)?;
    let secret_key = seer_net::load_or_create_secret_key(&directory.join("device.key"))
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

fn tcp_failure(endpoint: &str) -> CommandError {
    CommandError::usage(format!(
        "Cannot reach {endpoint}. Check that the server is running and that you are on the same network."
    ))
}

fn send(stream: &mut impl Stream, message: &ClientMsg) -> Result<(), CommandError> {
    codec::encode(stream, message).map_err(CommandError::system)
}

fn receive(stream: &mut impl Stream) -> Result<ServerMsg, CommandError> {
    codec::decode(stream).map_err(CommandError::system)
}

fn receive_reply(stream: &mut impl Stream) -> Result<ServerMsg, CommandError> {
    receive_reply_before(stream, Instant::now() + NETWORK_TIMEOUT)
}

fn receive_reply_before<S: Stream>(
    stream: &mut S,
    deadline: Instant,
) -> Result<ServerMsg, CommandError> {
    loop {
        let reply = codec::decode(&mut DeadlineReader { stream, deadline })
            .map_err(CommandError::system)?;
        if !matches!(
            reply,
            ServerMsg::Tree { .. } | ServerMsg::Frame { .. } | ServerMsg::Cells { .. }
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

fn finish_session(
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
