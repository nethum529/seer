use seer_core::proto::{ClientMsg, codec};
use seer_net::{Socket, Stream};
use std::io;

// ADR 0009: own terminal work goes over the private local socket to this
// computer's runtime, room work goes over the broker. The room route is
// optional and local work must keep going without it.
pub(crate) struct Routes {
    local: Socket,
    room: Option<Socket>,
    own_user: String,
    room_lost: bool,
}

impl Routes {
    pub(crate) fn new(local: Socket, room: Option<Socket>, own_user: String) -> Self {
        Self {
            local,
            room,
            own_user,
            room_lost: false,
        }
    }

    pub(crate) fn take_room_loss(&mut self) -> bool {
        std::mem::take(&mut self.room_lost)
    }

    pub(crate) fn restore_room(&mut self, room: Socket) {
        self.room = Some(room);
    }

    pub(crate) fn drop_room(&mut self) {
        if self.room.take().is_some() {
            self.room_lost = true;
        }
    }

    pub(crate) fn send(&mut self, message: &ClientMsg) -> io::Result<()> {
        seer_core::debug_log!(
            "send route={} {}",
            self.route(message),
            seer_core::debug_log::client_summary(message)
        );
        if matches!(message, ClientMsg::Detach) {
            self.send_room(message)?;
            return codec::encode(&mut self.local, message);
        }
        if self.is_local(message) {
            return codec::encode(&mut self.local, message);
        }
        self.send_room(message)
    }

    #[cfg(debug_assertions)]
    fn route(&self, message: &ClientMsg) -> &'static str {
        if self.is_local(message) {
            "local"
        } else if self.room.is_some() {
            "room"
        } else {
            "dropped-room-offline"
        }
    }

    fn is_local(&self, message: &ClientMsg) -> bool {
        match message {
            ClientMsg::TerminalInput { .. }
            | ClientMsg::CreateTab { .. }
            | ClientMsg::SplitPane { .. }
            | ClientMsg::ClosePane { .. }
            | ClientMsg::FocusPane { .. }
            | ClientMsg::Resize { .. }
            | ClientMsg::TerminalCapabilities { .. } => true,
            ClientMsg::Watch { user, .. }
            | ClientMsg::Unwatch { user, .. }
            | ClientMsg::Terminals { user } => user == &self.own_user,
            _ => false,
        }
    }

    // A message for a room that is gone is dropped, never queued. Nothing is
    // replayed after a reconnect.
    fn send_room(&mut self, message: &ClientMsg) -> io::Result<()> {
        let Some(room) = self.room.as_mut() else {
            return Ok(());
        };
        if codec::encode(room, message).is_err() {
            let _ = room.shutdown(std::net::Shutdown::Both);
            self.room = None;
            self.room_lost = true;
        }
        Ok(())
    }
}
