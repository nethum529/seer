use std::env;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_net::{Session, Socket, Stream};

const ENDPOINT_VAR: &str = "SEER_ROOM_ENDPOINT";
const CREDENTIAL_VAR: &str = "SEER_ROOM_CREDENTIAL";
const KEY_VAR: &str = "SEER_ROOM_KEY";
// Issue 366: only the runtime may read the room secrets, so a Seer shell
// must not inherit them.
pub(crate) const SECRET_VARS: [&str; 2] = [CREDENTIAL_VAR, KEY_VAR];
// The client saves an iroh room as iroh:<id>, the capsule form.
const IROH_PREFIX: &str = "iroh:";
const FIRST_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

pub(crate) struct RoomConfig {
    endpoint: String,
    credential: String,
    key_path: PathBuf,
}

pub(crate) fn from_environment() -> Option<RoomConfig> {
    let endpoint = non_empty(ENDPOINT_VAR)?;
    let credential = non_empty(CREDENTIAL_VAR)?;
    let key_path = PathBuf::from(non_empty(KEY_VAR)?);
    Some(RoomConfig {
        endpoint,
        credential,
        key_path,
    })
}

fn non_empty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

pub(crate) fn spawn(
    config: RoomConfig,
    user_id: String,
    generation: String,
    serve: impl Fn(UnixStream) + Send + 'static,
) -> io::Result<()> {
    thread::Builder::new()
        .name("runtime-room".into())
        .spawn(move || publish_loop(&config, &user_id, &generation, &serve))?;
    Ok(())
}

fn publish_loop(config: &RoomConfig, user_id: &str, generation: &str, serve: &impl Fn(UnixStream)) {
    let mut backoff = FIRST_BACKOFF;
    loop {
        match publish_once(config, user_id, generation, serve) {
            Ok(()) => backoff = FIRST_BACKOFF,
            Err(error) => {
                seer_core::debug_log!("room connection ended error={error}");
                eprintln!("runtime room connection ended: {error}");
            }
        }
        thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

fn publish_once(
    config: &RoomConfig,
    user_id: &str,
    generation: &str,
    serve: &impl Fn(UnixStream),
) -> io::Result<()> {
    let link = Link::connect(config)?;
    let mut control = link.open_socket()?;
    codec::encode(
        &mut control,
        &ClientMsg::PublishRuntime {
            user_id: user_id.to_owned(),
            credential: config.credential.clone(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            generation: generation.to_owned(),
        },
    )?;
    match codec::decode(&mut control)? {
        ServerMsg::Published { .. } => {}
        ServerMsg::Refused { reason } => {
            return Err(io::Error::other(format!(
                "room refused this runtime: {reason}"
            )));
        }
        _ => return Err(io::Error::other("room did not confirm the runtime")),
    }
    serve_requests(&link, config, user_id, &mut control, serve)
}

fn serve_requests(
    link: &Link,
    config: &RoomConfig,
    user_id: &str,
    control: &mut Socket,
    serve: &impl Fn(UnixStream),
) -> io::Result<()> {
    loop {
        let ServerMsg::OpenStream { token } = codec::decode(control)? else {
            continue;
        };
        // The runtime opens every shared stream. iroh gives a stream to the far
        // side only after the opener writes, so the opener must send first.
        let mut stream = link.open_socket()?;
        codec::encode(
            &mut stream,
            &ClientMsg::RuntimeStream {
                user_id: user_id.to_owned(),
                credential: config.credential.clone(),
                token,
            },
        )?;
        serve(into_unix(stream)?);
    }
}

enum Link {
    Iroh(Session),
    Tcp(std::net::SocketAddr),
}

impl Link {
    fn connect(config: &RoomConfig) -> io::Result<Self> {
        if let Some(id) = config.endpoint.strip_prefix(IROH_PREFIX) {
            return Self::iroh(id, config);
        }
        if let Some(address) = tcp_address(&config.endpoint) {
            return Ok(Self::Tcp(address));
        }
        Self::iroh(&config.endpoint, config)
    }

    fn iroh(id: &str, config: &RoomConfig) -> io::Result<Self> {
        let id = seer_net::decode_endpoint_id(id)?;
        let key = seer_net::load_or_create_secret_key(&config.key_path)?;
        Ok(Self::Iroh(seer_net::dial_session(key, id)?))
    }

    fn open_socket(&self) -> io::Result<Socket> {
        match self {
            Self::Iroh(session) => session.open().map(Socket::from),
            Self::Tcp(address) => TcpStream::connect(address).map(Socket::from),
        }
    }
}

fn tcp_address(endpoint: &str) -> Option<std::net::SocketAddr> {
    if !endpoint.contains(':') {
        return None;
    }
    endpoint.to_socket_addrs().ok()?.next()
}

// The runtime serves every connection as a Unix stream, so a TCP stream is
// copied onto one.
fn into_unix(socket: Socket) -> io::Result<UnixStream> {
    if let Socket::Iroh(stream) = &socket {
        return stream.try_clone();
    }
    let (caller, bridge) = UnixStream::pair()?;
    let mut reader = socket.clone();
    let mut writer = bridge.try_clone()?;
    thread::Builder::new()
        .name("runtime-room-read".into())
        .spawn(move || {
            let _ = io::copy(&mut reader, &mut writer);
            let _ = writer.shutdown(std::net::Shutdown::Write);
        })?;
    let mut sink = socket;
    let mut source = bridge;
    thread::Builder::new()
        .name("runtime-room-write".into())
        .spawn(move || {
            let _ = io::copy(&mut source, &mut sink);
            let _ = sink.shutdown(std::net::Shutdown::Write);
        })?;
    Ok(caller)
}
