use std::io;

use seer_core::proto::ServerMsg;

use super::{SharedSession, lock, writer};

pub(super) fn ready(generation: &str, room_refused: Option<String>) -> ServerMsg {
    ServerMsg::RuntimeReady {
        generation: generation.to_owned(),
        version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        room_refused,
    }
}

// Doc 24: a window from before 0.6.0 drops its link on a message kind it
// cannot decode, so the refusal goes to the windows in RuntimeReady again.
impl SharedSession {
    pub(super) fn room_refusal(&self) -> io::Result<Option<String>> {
        Ok(lock(&self.room_refused)?.clone())
    }

    pub(super) fn set_room_refusal(
        &self,
        generation: &str,
        reason: Option<String>,
    ) -> io::Result<()> {
        let connections = lock(&self.connections)?;
        let mut current = lock(&self.room_refused)?;
        if *current == reason {
            return Ok(());
        }
        current.clone_from(&reason);
        let message = writer::encode(&ready(generation, reason))?;
        for window in connections
            .iter()
            .filter(|connection| !connection.read_only)
        {
            window.send(message.clone());
        }
        Ok(())
    }

    // The refusal can change after the greeting and before the window joins
    // the connections.
    pub(super) fn resend_room_refusal(
        &self,
        id: u64,
        generation: &str,
        greeted: &Option<String>,
    ) -> io::Result<()> {
        let connections = lock(&self.connections)?;
        let current = lock(&self.room_refused)?;
        if *current == *greeted {
            return Ok(());
        }
        let message = writer::encode(&ready(generation, current.clone()))?;
        for window in connections.iter().filter(|connection| connection.id == id) {
            window.send(message.clone());
        }
        Ok(())
    }
}
