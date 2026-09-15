use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TerminalInput};
use std::io::{self, Read};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(20);
const RETRY: Duration = Duration::from_millis(50);

pub(crate) struct HostWindow {
    pub(crate) stream: UnixStream,
    pub(crate) workspace: String,
    pub(crate) tab: String,
    pub(crate) pane: String,
}

impl HostWindow {
    pub(crate) fn attach(socket: &Path) -> io::Result<Self> {
        let deadline = Instant::now() + WAIT;
        let mut stream = loop {
            if let Ok(mut stream) = UnixStream::connect(socket) {
                stream.set_read_timeout(Some(WAIT))?;
                if let Ok(ServerMsg::RuntimeReady { .. }) =
                    codec::decode::<_, ServerMsg>(&mut stream)
                {
                    codec::encode(&mut stream, &ClientMsg::AttachRuntime)?;
                    break stream;
                }
            }
            if Instant::now() > deadline {
                return Err(timeout("the local runtime did not answer"));
            }
            thread::sleep(RETRY);
        };
        loop {
            if let ServerMsg::Tree { tree } = codec::decode(&mut stream)?
                && let Some(workspace) = tree.workspaces.first()
                && let Some(tab) = workspace.tabs.first()
                && let Some(pane) = tab.panes.first()
            {
                return Ok(Self {
                    stream,
                    workspace: workspace.id.clone(),
                    tab: tab.id.clone(),
                    pane: pane.id.clone(),
                });
            }
        }
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        codec::encode(
            &mut self.stream,
            &ClientMsg::Resize {
                workspace: self.workspace.clone(),
                tab: self.tab.clone(),
                cols,
                rows,
            },
        )
    }

    pub(crate) fn input(&mut self, event: InputEvent) -> io::Result<()> {
        codec::encode(
            &mut self.stream,
            &ClientMsg::TerminalInput {
                workspace: self.workspace.clone(),
                tab: self.tab.clone(),
                pane: self.pane.clone(),
                input: TerminalInput::new(event),
            },
        )
    }

    // The runtime drops a window that does not read its screen updates.
    pub(crate) fn drain_in_background(&self) -> io::Result<()> {
        let mut reader = self.stream.try_clone()?;
        thread::spawn(
            move || {
                while codec::decode::<_, ServerMsg>(&mut reader).is_ok() {}
            },
        );
        Ok(())
    }
}

pub(crate) struct Guest {
    stream: TcpStream,
}

impl Guest {
    pub(crate) fn join(room: SocketAddr, user: &str, credential: &str) -> io::Result<Self> {
        let mut stream = TcpStream::connect(room)?;
        stream.set_read_timeout(Some(WAIT))?;
        codec::encode(
            &mut stream,
            &ClientMsg::Hello {
                user_id: user.into(),
                credential: credential.into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        )?;
        let ServerMsg::Welcome { .. } = codec::decode(&mut stream)? else {
            return Err(io::Error::other("the room did not welcome the guest"));
        };
        Ok(Self { stream })
    }

    pub(crate) fn wait_published(&mut self, host: &str) -> io::Result<()> {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            codec::encode(&mut self.stream, &ClientMsg::ListPeople)?;
            let ServerMsg::People { people } =
                self.wait_for(|m| matches!(m, ServerMsg::People { .. }))?
            else {
                continue;
            };
            if people.iter().any(|p| p.user_id == host && p.peekable) {
                return Ok(());
            }
            thread::sleep(RETRY);
        }
        Err(timeout("the host never published a runtime"))
    }

    pub(crate) fn watch(&mut self, host: &str, pane: &str, cols: u16, rows: u16) -> io::Result<()> {
        codec::encode(
            &mut self.stream,
            &ClientMsg::Watch {
                user: host.into(),
                pane: pane.into(),
                cols,
                rows,
            },
        )
    }

    pub(crate) fn wait_for(
        &mut self,
        matches: impl Fn(&ServerMsg) -> bool,
    ) -> io::Result<ServerMsg> {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            let (_, message) = self.read_frame()?;
            if matches(&message) {
                return Ok(message);
            }
        }
        Err(timeout("the expected room message never arrived"))
    }

    pub(crate) fn set_read_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.stream.set_read_timeout(Some(timeout))
    }

    // Returns the bytes of one frame as they arrive on the socket: the 4 byte
    // length prefix plus the JSON body.
    pub(crate) fn read_frame(&mut self) -> io::Result<(usize, ServerMsg)> {
        let mut prefix = [0; 4];
        self.stream.read_exact(&mut prefix)?;
        let length = u32::from_be_bytes(prefix) as usize;
        let mut body = vec![0; length];
        self.stream.read_exact(&mut body)?;
        let message = serde_json::from_slice(&body).map_err(io::Error::other)?;
        Ok((length + 4, message))
    }
}

fn timeout(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, reason)
}
