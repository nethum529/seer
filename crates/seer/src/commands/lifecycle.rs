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
    wait_for_completion(&mut stream, |reply| matches!(reply, ServerMsg::Bye { .. }))?;
    let mut store = ServerStore::load().map_err(CommandError::system)?;
    store
        .servers
        .retain(|entry| entry.user_id != server.user_id);
    store.save().map_err(CommandError::system)?;
    println!("Left {}. The room stays open for the others.", server.alias);
    crate::local::stop(&server.user_id).map_err(CommandError::system)?;
    Ok(())
}

pub(crate) fn stop() -> Result<(), CommandError> {
    #[cfg(target_os = "linux")]
    if ServerStore::load()
        .map_err(CommandError::system)?
        .servers
        .is_empty()
        && crate::start::stop_hosted_broker().map_err(CommandError::system)?
    {
        println!("Room server stopped.");
        return Ok(());
    }
    let server = selected_server()?;
    let (mut stream, _) = authenticate(&server)?;
    send(&mut stream, &ClientMsg::Stop)?;
    wait_for_completion(&mut stream, |reply| matches!(reply, ServerMsg::Bye { .. }))?;
    let deadline = std::time::Instant::now() + NETWORK_TIMEOUT;
    while super::connect(&server.endpoint).is_ok() {
        if std::time::Instant::now() >= deadline {
            return Err(CommandError::usage("The server has not stopped yet."));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    if crate::local::stop(&server.user_id).map_err(CommandError::system)? {
        println!("Your terminals on this computer stopped.");
    }
    println!("Room server stopped.");
    Ok(())
}

fn wait_for_completion(
    stream: &mut seer_net::Socket,
    done: impl Fn(&ServerMsg) -> bool,
) -> Result<(), CommandError> {
    let deadline = std::time::Instant::now() + NETWORK_TIMEOUT;
    loop {
        match receive_reply_before(stream, deadline)? {
            reply if done(&reply) => break,
            ServerMsg::People { .. } => continue,
            ServerMsg::Refused { reason } => return Err(CommandError::usage(reason)),
            _ => return Err(unexpected_reply()),
        }
    }
    Ok(())
}

pub(crate) fn perms(can_type: bool) -> Result<(), CommandError> {
    let server = selected_server()?;
    let (mut stream, _) = authenticate(&server)?;
    send(&mut stream, &ClientMsg::SetAllGrants { can_type })?;
    wait_for_completion(&mut stream, |reply| *reply == ServerMsg::GrantsUpdated)?;
    if can_type {
        println!("All current participants can type into your terminals.");
    } else {
        println!("All permissions to type into your terminals are revoked.");
    }
    Ok(())
}
