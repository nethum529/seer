use mux_core::proto::{ClientMsg, ServerMsg, codec};
use std::fs;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use crate::UserSession;

const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub fn bind(path: &Path) -> io::Result<UnixListener> {
    remove_stale_socket(path)?;
    UnixListener::bind(path)
}

pub fn serve(listener: UnixListener, mut session: UserSession) -> io::Result<()> {
    loop {
        let (stream, _) = listener.accept()?;
        if let Err(error) = handle_connection(stream, &mut session) {
            eprintln!("runtime connection error: {error}");
        }
    }
}

fn remove_stale_socket(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    match UnixStream::connect(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "runtime socket is already in use",
        )),
        Err(_) => fs::remove_file(path),
    }
}

fn handle_connection(mut stream: UnixStream, session: &mut UserSession) -> io::Result<()> {
    codec::encode(
        &mut stream,
        &ServerMsg::Tree {
            tree: session.tree.clone(),
        },
    )?;

    let receiver = start_reader(stream.try_clone()?);
    let result = connection_loop(&mut stream, session, &receiver);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    result
}

fn start_reader(mut stream: UnixStream) -> Receiver<io::Result<ClientMsg>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        loop {
            let message = codec::decode(&mut stream);
            let finished = message.is_err();
            if sender.send(message).is_err() {
                return;
            }
            if finished {
                return;
            }
        }
    });
    receiver
}

fn connection_loop(
    stream: &mut UnixStream,
    session: &mut UserSession,
    receiver: &Receiver<io::Result<ClientMsg>>,
) -> io::Result<()> {
    loop {
        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(Ok(message)) => {
                let detach = message == ClientMsg::Detach;
                write_messages(stream, &session.apply(message)?)?;
                if detach {
                    return Ok(());
                }
            }
            Ok(Err(_)) => return Ok(()),
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => {}
        }

        write_messages(stream, &session.poll())?;
    }
}

fn write_messages(stream: &mut UnixStream, messages: &[ServerMsg]) -> io::Result<()> {
    for message in messages {
        codec::encode(stream, message)?;
    }
    Ok(())
}
