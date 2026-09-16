use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_net::Socket;

use super::{CommandError, connect, receive_reply, send, unexpected_reply};
use crate::store::ServerEntry;
use crate::version_skew::{refusal_message, version_refusal};

pub(crate) enum Welcome {
    Accepted(Socket, Tree),
    Refused(String),
}

pub(crate) fn hello(server: &ServerEntry) -> Result<Welcome, CommandError> {
    let mut stream = connect(&server.endpoint)?;
    let hello = ClientMsg::Hello {
        user_id: server.user_id.clone(),
        credential: server.credential.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    send(&mut stream, &hello)?;
    match receive_reply(&mut stream)? {
        ServerMsg::Welcome { tree, .. } => Ok(Welcome::Accepted(stream, tree)),
        ServerMsg::Refused { reason } => Ok(Welcome::Refused(reason)),
        _ => Err(unexpected_reply()),
    }
}

pub(crate) fn authenticate(server: &ServerEntry) -> Result<(Socket, Tree), CommandError> {
    match hello(server)? {
        Welcome::Accepted(stream, tree) => Ok((stream, tree)),
        Welcome::Refused(reason) => Err(CommandError::usage(refusal_message(
            &reason,
            crate::start::hosts_room(&server.endpoint),
        ))),
    }
}

// A Hello with no credentials gets the version verdict from every room
// server, also one from before the Join version check, and uses no seat.
pub(crate) fn check_room_version(endpoint: &str) -> Result<(), CommandError> {
    let probe = ServerEntry {
        endpoint: endpoint.to_owned(),
        alias: String::new(),
        user_id: String::new(),
        name: String::new(),
        credential: String::new(),
        current: false,
    };
    match hello(&probe)? {
        Welcome::Refused(reason) if version_refusal(&reason).is_some() => {
            Err(CommandError::usage(refusal_message(&reason, false)))
        }
        _ => Ok(()),
    }
}
