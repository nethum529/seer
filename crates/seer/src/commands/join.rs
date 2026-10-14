use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::fd::AsRawFd;

use seer_core::proto::{ClientMsg, ServerMsg};

use super::{CommandError, connect, handshake, receive_reply, send};
use crate::capsule::{self, Capsule};
use crate::store::{ServerEntry, ServerStore};

#[derive(Clone)]
pub struct Invitation(Capsule);

#[derive(Debug)]
pub enum JoinError {
    BadInvite,
    InvalidSeat,
    NameInUse,
    InvalidName,
    Unreachable,
    Refused(String),
    Other(String),
}

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadInvite => "This is not a Seer invite. Paste the full line from the host.",
            Self::InvalidSeat => "This invite was used or has expired. Ask the host for a new one.",
            Self::NameInUse => "That name is in use. Choose another.",
            Self::InvalidName => "Use 1 to 32 letters, numbers, underscores, or hyphens.",
            Self::Unreachable => {
                "Seer cannot reach the room. Check that it is running, then try again."
            }
            Self::Refused(message) | Self::Other(message) => message,
        })
    }
}

impl std::error::Error for JoinError {}

impl Invitation {
    pub fn parse(line: &str) -> Result<Self, JoinError> {
        let line = line.trim();
        let capsule = if let Some(link) = line.strip_prefix("seer://join/") {
            link
        } else if line.starts_with("seer://") {
            return Err(JoinError::BadInvite);
        } else {
            line.split_whitespace()
                .rfind(|word| word.starts_with("SEER"))
                .unwrap_or(line)
        };
        capsule::parse(capsule)
            .map(Self)
            .map_err(|_| JoinError::BadInvite)
    }

    pub fn join(&self, name: &str) -> Result<ServerEntry, JoinError> {
        self.check_version()?;
        self.exchange(name)
    }

    fn check_version(&self) -> Result<(), JoinError> {
        let endpoint = self.0.endpoint.to_string();
        match handshake::probe_room_version(&endpoint).map_err(|_| JoinError::Unreachable)? {
            Some(message) => Err(JoinError::Refused(message)),
            None => Ok(()),
        }
    }

    fn exchange(&self, name: &str) -> Result<ServerEntry, JoinError> {
        let endpoint = self.0.endpoint.to_string();
        let mut stream = connect(&endpoint).map_err(|_| JoinError::Unreachable)?;
        send(
            &mut stream,
            &ClientMsg::Join {
                seat_token: self.0.token.clone(),
                name: name.to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            },
        )
        .map_err(|_| JoinError::Unreachable)?;
        match receive_reply(&mut stream).map_err(|_| JoinError::Unreachable)? {
            ServerMsg::Joined {
                user_id,
                credential,
                name,
            } => self.save(user_id, credential, name),
            ServerMsg::Refused { reason } => Err(match reason.as_str() {
                "invalid seat" => JoinError::InvalidSeat,
                "name is in use" => JoinError::NameInUse,
                "invalid name" => JoinError::InvalidName,
                _ => JoinError::Refused("The room refused the join. Ask the host for help.".into()),
            }),
            _ => Err(JoinError::Other(
                "The room sent an unexpected reply.".into(),
            )),
        }
    }

    fn save(
        &self,
        user_id: String,
        credential: String,
        name: String,
    ) -> Result<ServerEntry, JoinError> {
        let mut store = ServerStore::load().map_err(|_| save_error())?;
        let server = ServerEntry {
            endpoint: self.0.endpoint.to_string(),
            alias: self.0.alias.clone(),
            user_id,
            name,
            credential,
            current: true,
        };
        store.make_current(server.clone());
        store.save().map_err(|_| save_error())?;
        Ok(server)
    }
}

pub fn system_user_name() -> Option<String> {
    ["USER", "LOGNAME"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|name| !name.is_empty()))
}

fn save_error() -> JoinError {
    JoinError::Other("Joined, but Seer could not save the room on this computer.".into())
}

fn command_error(error: JoinError) -> CommandError {
    match error {
        JoinError::InvalidSeat
        | JoinError::NameInUse
        | JoinError::InvalidName
        | JoinError::Refused(_) => CommandError::usage(error.to_string()),
        _ => CommandError::system(error),
    }
}

pub(crate) fn run(invitation: Option<&str>) -> Result<(), CommandError> {
    let line = match invitation {
        Some(line) => line.to_owned(),
        None => crate::prompt::hidden("Invitation: ").map_err(CommandError::system)?,
    };
    let invitation = Invitation::parse(&line).map_err(CommandError::system)?;
    invitation.check_version().map_err(command_error)?;
    let terminal = interactive_terminal().map_err(CommandError::system)?;
    let default = system_user_name();
    loop {
        let name = crate::prompt::visible_with_default("Name", default.as_deref())
            .map_err(CommandError::system)?;
        match invitation.exchange(&name) {
            Err(JoinError::NameInUse) => println!("That name is in use."),
            Err(error) => return Err(command_error(error)),
            Ok(server) => {
                println!("Joined as {}. Attaching...", server.name);
                let (stream, _) = super::authenticate(&server)?;
                return super::finish_session(terminal, Some(stream), None, &server);
            }
        }
    }
}

fn interactive_terminal() -> io::Result<bool> {
    let Ok(terminal) = OpenOptions::new().read(true).write(true).open("/dev/tty") else {
        return Ok(false);
    };
    io::stdout().flush()?;
    for descriptor in [libc::STDIN_FILENO, libc::STDOUT_FILENO] {
        // SAFETY: the terminal is live and dup2 replaces only stdin or stdout.
        if unsafe { libc::dup2(terminal.as_raw_fd(), descriptor) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(true)
}
