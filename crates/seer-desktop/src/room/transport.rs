use std::io;
use std::net::Shutdown;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use futures::{
    SinkExt,
    channel::mpsc::{Sender as EventSender, channel},
};
use seer::ServerEntry;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_net::{Socket, Stream};

use crate::link::{Event, Messages};

pub(crate) enum RoomEvent {
    Connected(Sender<ClientMsg>),
    VersionMismatch(seer::VersionRefusal),
    Message(ServerMsg),
    Lost,
    Stopped,
    Removed,
}

enum Disconnect {
    Lost,
    Stopped,
    Removed,
}

pub(super) fn start(server: ServerEntry, stopped: Arc<AtomicBool>) -> Messages {
    let (mut sender, messages) = channel(64);
    thread::spawn(move || {
        let mut wait = Duration::from_secs(1);
        while !stopped.load(Ordering::Acquire) && !sender.is_closed() {
            let connection = seer::connect_room(&server);
            if let Err(error) = &connection
                && let Some(refusal) = error
                    .get_ref()
                    .and_then(|error| error.downcast_ref::<seer::VersionRefusal>())
            {
                let _ = futures::executor::block_on(
                    sender.send(Ok(Event::Room(RoomEvent::VersionMismatch(refusal.clone())))),
                );
            }
            if let Ok(socket) = connection {
                let event = match read(socket, &mut sender) {
                    Ok(Disconnect::Stopped) => Some(RoomEvent::Stopped),
                    Ok(Disconnect::Removed) => Some(RoomEvent::Removed),
                    _ => None,
                };
                if let Some(event) = event {
                    let _ = futures::executor::block_on(sender.send(Ok(Event::Room(event))));
                    break;
                }
                wait = Duration::from_secs(1);
            }
            if futures::executor::block_on(sender.send(Ok(Event::Room(RoomEvent::Lost)))).is_err() {
                break;
            }
            thread::sleep(wait);
            wait = (wait * 2).min(Duration::from_secs(30));
        }
    });
    Box::pin(messages)
}

fn read(mut socket: Socket, sender: &mut EventSender<io::Result<Event>>) -> io::Result<Disconnect> {
    let (requests, outgoing) = mpsc::channel();
    let mut writer = socket.clone();
    thread::spawn(move || {
        while let Ok(message) = outgoing.recv() {
            if codec::encode(&mut writer, &message).is_err() {
                break;
            }
        }
        let _ = writer.shutdown(Shutdown::Both);
    });
    futures::executor::block_on(sender.send(Ok(Event::Room(RoomEvent::Connected(requests)))))
        .map_err(|_| io::Error::other("window closed"))?;
    loop {
        let message: ServerMsg = codec::decode(&mut socket)?;
        if let ServerMsg::Bye { reason } = message {
            return Ok(match reason.as_str() {
                "server stopped" => Disconnect::Stopped,
                "removed by host" => Disconnect::Removed,
                _ => Disconnect::Lost,
            });
        }
        futures::executor::block_on(sender.send(Ok(Event::Room(RoomEvent::Message(message)))))
            .map_err(|_| io::Error::other("window closed"))?;
    }
}
