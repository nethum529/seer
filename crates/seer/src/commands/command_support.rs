use std::io::{self, Read};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientInfo, ClientMsg, Person, ServerMsg, codec};
use seer_net::{Socket, Stream};

use super::{CommandError, unexpected_reply};
use crate::capsule;
use crate::capsule::Endpoint;
use crate::store::{ServerEntry, ServerStore};
use crate::tui;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn receive_clients(stream: &mut impl Stream) -> Result<Vec<ClientInfo>, CommandError> {
    match receive_reply(stream)? {
        ServerMsg::Clients { clients } => Ok(clients),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}


pub(super) fn connect(endpoint: &str) -> Result<Socket, CommandError> {
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

pub(super) fn authenticate(server: &ServerEntry) -> Result<(Socket, Tree), CommandError> {
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

pub(super) fn people(server: &ServerEntry) -> Result<Vec<Person>, CommandError> {
    let (mut stream, _) = authenticate(server)?;
    send(&mut stream, &ClientMsg::ListPeople)?;
    people_reply(receive_reply(&mut stream)?)
}

pub(super) fn people_reply(reply: ServerMsg) -> Result<Vec<Person>, CommandError> {
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

pub(super) fn send(stream: &mut impl Stream, message: &ClientMsg) -> Result<(), CommandError> {
    codec::encode(stream, message).map_err(CommandError::system)
}

pub(super) fn receive(stream: &mut impl Stream) -> Result<ServerMsg, CommandError> {
    codec::decode(stream).map_err(CommandError::system)
}

pub(super) fn receive_reply(stream: &mut impl Stream) -> Result<ServerMsg, CommandError> {
    receive_reply_before(stream, Instant::now() + NETWORK_TIMEOUT)
}

pub(super) fn receive_reply_before<S: Stream>(
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
        super::print_detached(alias);
    }
    Ok(())
}
