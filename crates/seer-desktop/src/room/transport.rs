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
    Message(ServerMsg),
    Lost,
}

pub(super) fn start(server: ServerEntry, stopped: Arc<AtomicBool>) -> Messages {
    let (mut sender, messages) = channel(64);
    thread::spawn(move || {
        let mut wait = Duration::from_secs(1);
        while !stopped.load(Ordering::Acquire) && !sender.is_closed() {
            if let Ok(socket) = seer::connect_room(&server) {
                let _ = read(socket, &mut sender);
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

fn read(mut socket: Socket, sender: &mut EventSender<io::Result<Event>>) -> io::Result<()> {
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
        if matches!(message, ServerMsg::Bye { .. }) {
            return Ok(());
        }
        futures::executor::block_on(sender.send(Ok(Event::Room(RoomEvent::Message(message)))))
            .map_err(|_| io::Error::other("window closed"))?;
    }
}
