use iroh::endpoint::{Connection, RecvStream, SendStream};
use std::io;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinSet;

const OPEN_TIMEOUT: Duration = Duration::from_secs(10);

type OpenReply = Sender<io::Result<UnixStream>>;

/// One iroh connection that carries many streams.
///
/// iroh hands a stream to the far side only after the opener writes, so the
/// side that calls `open` must send the first message on that stream.
pub struct Session {
    opens: UnboundedSender<OpenReply>,
    accepted: Mutex<Receiver<UnixStream>>,
    control: Option<UnixStream>,
}

impl Session {
    pub fn open(&self) -> io::Result<UnixStream> {
        let (reply, replies) = mpsc::channel();
        self.opens
            .send(reply)
            .map_err(|_| io::Error::other("session is closed"))?;
        replies
            .recv_timeout(OPEN_TIMEOUT)
            .map_err(|_| io::Error::other("session did not open a stream"))?
    }

    pub fn accept(&self) -> io::Result<UnixStream> {
        let accepted = self
            .accepted
            .lock()
            .map_err(|_| io::Error::other("session accept lock is poisoned"))?;
        accepted
            .recv()
            .map_err(|_| io::Error::other("session is closed"))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(control) = self.control.take() {
            let _ = control.shutdown(std::net::Shutdown::Both);
        }
    }
}

pub(crate) struct SessionParts {
    pub(crate) opens: UnboundedReceiver<OpenReply>,
    pub(crate) accepted: Sender<UnixStream>,
}

pub(crate) fn session_pair(control: Option<UnixStream>) -> (Session, SessionParts) {
    let (opens_tx, opens_rx) = unbounded_channel();
    let (accepted_tx, accepted_rx) = mpsc::channel();
    (
        Session {
            opens: opens_tx,
            accepted: Mutex::new(accepted_rx),
            control,
        },
        SessionParts {
            opens: opens_rx,
            accepted: accepted_tx,
        },
    )
}

pub(crate) async fn serve_connection(connection: Connection, parts: SessionParts) {
    let SessionParts {
        mut opens,
        accepted,
    } = parts;
    let mut bridges = JoinSet::new();
    let mut serving_opens = true;
    loop {
        tokio::select! {
            request = opens.recv(), if serving_opens => {
                let Some(reply) = request else {
                    serving_opens = false;
                    continue;
                };
                open_stream(&connection, &reply, &mut bridges).await;
            }
            streams = connection.accept_bi() => {
                let Ok((send, recv)) = streams else { break };
                accept_stream(send, recv, &accepted, &mut bridges);
            }
            Some(_) = bridges.join_next(), if !bridges.is_empty() => {}
        }
    }
    bridges.shutdown().await;
}

async fn open_stream(connection: &Connection, reply: &OpenReply, bridges: &mut JoinSet<()>) {
    let streams = match connection.open_bi().await {
        Ok(streams) => streams,
        Err(_) => {
            let _ = reply.send(Err(io::Error::other("could not open a stream")));
            return;
        }
    };
    let (caller, bridge) = match crate::stream_pair() {
        Ok(pair) => pair,
        Err(error) => {
            let _ = reply.send(Err(error));
            return;
        }
    };
    if reply.send(Ok(caller)).is_ok() {
        bridges.spawn(run_bridge(streams.0, streams.1, bridge));
    }
}

fn accept_stream(
    send: SendStream,
    recv: RecvStream,
    accepted: &Sender<UnixStream>,
    bridges: &mut JoinSet<()>,
) {
    let Ok((caller, bridge)) = crate::stream_pair() else {
        return;
    };
    if accepted.send(caller).is_ok() {
        bridges.spawn(run_bridge(send, recv, bridge));
    }
}

async fn run_bridge(send: SendStream, recv: RecvStream, unix: tokio::net::UnixStream) {
    let _ = crate::bridge(send, recv, unix).await;
}

pub(crate) async fn shutdown_on_control(mut control: tokio::net::UnixStream) {
    let mut byte = [0_u8; 1];
    loop {
        match tokio::io::AsyncReadExt::read(&mut control, &mut byte).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}
