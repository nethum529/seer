use std::io::{self, Read};
use std::net::Shutdown;
use std::time::{Duration, Instant};

use seer_core::Cell;
use seer_core::proto::{ClientMsg, PeekTarget, Person, ServerMsg, codec};
use seer_net::{Socket, Stream};

use crate::commands::{authenticate, selected_server};
use crate::state::ClientState;

const POLL_TIMEOUT: Duration = Duration::from_millis(2);
const DRAW_INTERVAL: Duration = Duration::from_secs(1);
const READS_PER_POLL: usize = 32;

pub(crate) struct Preview {
    stream: Socket,
    state: ClientState,
    buffer: Vec<u8>,
    pending: bool,
    last_draw: Instant,
}

impl Preview {
    pub(crate) fn open(person: &Person) -> io::Result<Self> {
        let server = selected_server().map_err(|error| io::Error::other(error.message))?;
        let (mut stream, _) =
            authenticate(&server).map_err(|error| io::Error::other(error.message))?;
        codec::encode(
            &mut stream,
            &ClientMsg::QueryTargets {
                user: person.user_id.clone(),
            },
        )?;
        let targets = read_until(&mut stream, |message| match message {
            ServerMsg::Targets { targets } => Some(targets),
            _ => None,
        })?;
        let target = first_target(targets)?;
        codec::encode(
            &mut stream,
            &ClientMsg::Peek {
                user: person.user_id.clone(),
                workspace: target.workspace,
                tab: target.tab,
            },
        )?;
        let tree = read_until(&mut stream, |message| match message {
            ServerMsg::Tree { tree } => Some(tree),
            _ => None,
        })?;
        stream.set_read_timeout(Some(POLL_TIMEOUT))?;
        Ok(Self {
            stream,
            state: ClientState::new(tree, String::new()),
            buffer: Vec::new(),
            pending: true,
            last_draw: Instant::now() - DRAW_INTERVAL,
        })
    }

    pub(crate) fn poll(&mut self) -> io::Result<()> {
        self.fill()?;
        self.drain();
        Ok(())
    }

    pub(crate) fn tick(&mut self) -> io::Result<bool> {
        self.poll()?;
        if !self.pending || self.last_draw.elapsed() < DRAW_INTERVAL {
            return Ok(false);
        }
        self.pending = false;
        self.last_draw = Instant::now();
        Ok(true)
    }

    pub(crate) fn rows(&self) -> &[Vec<Cell>] {
        match self.state.focused() {
            Some(pane) => self.state.pane_rows(pane),
            None => &[],
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        let mut chunk = [0; 8192];
        for _ in 0..READS_PER_POLL {
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "preview connection closed",
                    ));
                }
                Ok(count) => self.buffer.extend_from_slice(&chunk[..count]),
                Err(error) if idle(&error) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn drain(&mut self) {
        let mut messages = Vec::new();
        let mut consumed = 0;
        let mut rest: &[u8] = &self.buffer;
        while let Ok(message) = codec::decode::<_, ServerMsg>(&mut rest) {
            consumed = self.buffer.len() - rest.len();
            messages.push(message);
        }
        self.buffer.drain(..consumed);
        for message in messages {
            self.apply(message);
        }
    }

    fn apply(&mut self, message: ServerMsg) {
        match message {
            ServerMsg::Tree { tree } => {
                self.state.replace_tree(tree);
                self.pending = true;
            }
            ServerMsg::Cells { pane, frame } => {
                self.state.apply_frame(pane, frame);
                self.pending = true;
            }
            _ => {}
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        let _ = codec::encode(&mut self.stream, &ClientMsg::StopPeek);
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

fn first_target(targets: Vec<PeekTarget>) -> io::Result<PeekTarget> {
    targets
        .iter()
        .find(|target| target.active)
        .or_else(|| targets.first())
        .cloned()
        .ok_or_else(|| io::Error::other("no peek target"))
}

fn read_until<T>(stream: &mut Socket, take: fn(ServerMsg) -> Option<T>) -> io::Result<T> {
    loop {
        match codec::decode(stream)? {
            ServerMsg::Refused { reason } => return Err(io::Error::other(reason)),
            message => {
                if let Some(value) = take(message) {
                    return Ok(value);
                }
            }
        }
    }
}

fn idle(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

#[cfg(test)]
impl Preview {
    pub(super) fn with_stream(stream: Socket) -> Self {
        Self {
            stream,
            state: ClientState::new(seer_core::Tree::new(), String::new()),
            buffer: Vec::new(),
            pending: false,
            last_draw: Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests;
