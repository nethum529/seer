use std::env;

use seer_core::proto::{ClientMsg, ServerMsg};
use seer_net::{Socket, Stream};

use super::{CommandError, NETWORK_TIMEOUT, receive, send, unexpected_reply};
use crate::local;

pub(crate) fn exit() -> Result<(), CommandError> {
    let (Some(user_id), Some(pane)) = (marker("SEER_USER_ID"), marker("SEER_PANE")) else {
        return Err(CommandError::usage(
            "seer exit works only inside a Seer terminal.",
        ));
    };
    let socket = local::runtime_directory(&user_id)
        .map_err(CommandError::system)?
        .join("socket");
    let stream = local::connect(&socket, None)
        .map_err(|_| CommandError::usage("The Seer runtime for this terminal is not running."))?;
    let mut stream = Socket::from(stream);
    stream
        .set_read_timeout(Some(NETWORK_TIMEOUT))
        .map_err(CommandError::system)?;
    send(&mut stream, &ClientMsg::ExitClient { pane })?;
    match receive(&mut stream)? {
        ServerMsg::Bye { .. } => Ok(()),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    }
}

fn marker(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}
