use seer_core::proto::{ClientMsg, ServerMsg};

use super::{CommandError, authenticate, finish_request, receive_reply, send, unexpected_reply};
use crate::{IssuedInvite, ServerEntry};

pub(crate) fn create(
    server: &ServerEntry,
    hours: Option<u32>,
) -> Result<IssuedInvite, CommandError> {
    if hours.is_some_and(|hours| !(1..=168).contains(&hours)) {
        return Err(CommandError::usage("hours must be from 1 to 168"));
    }
    let (mut stream, _) = authenticate(server)?;
    send(&mut stream, &ClientMsg::Invite { hours })?;
    let reply = receive_reply(&mut stream).and_then(|reply| match reply {
        ServerMsg::Seat {
            capsule,
            expires_in_secs,
        } => Ok(IssuedInvite {
            capsule,
            expires_in_secs,
        }),
        ServerMsg::Refused { reason } => Err(CommandError::usage(reason)),
        _ => Err(unexpected_reply()),
    });
    finish_request(stream, reply)
}
