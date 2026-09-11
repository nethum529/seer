use super::{
    CommandError, NETWORK_TIMEOUT, authenticate, receive_reply_before, selected_server, send,
    unexpected_reply,
};
use crate::store::ServerStore;
use seer_core::proto::{ClientMsg, ServerMsg};

pub(crate) fn leave() -> Result<(), CommandError> {
    let server = selected_server()?;
    let (mut stream, _) = authenticate(&server)?;
    send(&mut stream, &ClientMsg::Leave)?;
    let deadline = std::time::Instant::now() + NETWORK_TIMEOUT;
    loop {
        match receive_reply_before(&mut stream, deadline)? {
            ServerMsg::Bye { .. } => break,
            ServerMsg::People { .. } => continue,
            ServerMsg::Refused { reason } => return Err(CommandError::usage(reason)),
            _ => return Err(unexpected_reply()),
        }
    }
    let mut store = ServerStore::load().map_err(CommandError::system)?;
    store
        .servers
        .retain(|entry| entry.user_id != server.user_id);
    store.save().map_err(CommandError::system)?;
    println!("Left {}. The room stays open for the others.", server.alias);
    crate::local::stop(&server.user_id).map_err(CommandError::system)?;
    Ok(())
}
