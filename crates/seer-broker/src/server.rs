use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, ServerMsg, codec};

use crate::Config;
use crate::forwarding::forward;
use crate::registry::{PersonRecord, Registry};
use crate::runtime::RuntimeManager;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const INVALID_CREDENTIALS: &str = "invalid credentials";
const EXPECTED_HELLO: &str = "expected Hello";
const INVALID_MESSAGE: &str = "invalid message";

pub fn serve(listener: TcpListener, config: &Config) -> io::Result<()> {
    let (broker, owner_credential) = BrokerState::new(config)?;
    if let Some(credential) = owner_credential {
        writeln!(io::stdout().lock(), "owner-credential: {credential}")?;
    }
    let broker = Arc::new(broker);
    for connection in listener.incoming() {
        let stream = connection?;
        let broker = Arc::clone(&broker);
        thread::spawn(move || report_connection(handle_connection(stream, &broker)));
    }
    Ok(())
}

pub(crate) struct BrokerState {
    registry: Registry,
    runtimes: RuntimeManager,
    attachments: AttachmentCounts,
    published: PublishedAddress,
}

impl BrokerState {
    pub(crate) fn new(config: &Config) -> io::Result<(Self, Option<String>)> {
        let published = PublishedAddress::parse(&config.published_addr)?;
        let runtimes = RuntimeManager::new(config.shell.clone(), config.state_dir.clone())?;
        let (registry, owner_credential) = Registry::open(&config.state_dir, &config.owner_name)?;
        Ok((
            Self {
                registry,
                runtimes,
                attachments: AttachmentCounts::default(),
                published,
            },
            owner_credential,
        ))
    }

    pub(crate) fn registry(&self) -> &Registry {
        &self.registry
    }

    pub(crate) fn runtimes(&self) -> &RuntimeManager {
        &self.runtimes
    }

    pub(crate) fn invite(&self) -> io::Result<ServerMsg> {
        let token = self.registry.create_seat()?;
        Ok(ServerMsg::Seat {
            capsule: self.published.capsule(&token),
            expires_in_secs: Registry::seat_lifetime_secs(),
        })
    }

    pub(crate) fn people(&self) -> io::Result<ServerMsg> {
        let people = self
            .registry
            .people()?
            .into_iter()
            .map(|person| Person {
                attached_clients: self.attachments.count(&person.user_id),
                peekable: self.runtimes.is_running(&person.user_id),
                user_id: person.user_id,
                name: person.name,
            })
            .collect();
        Ok(ServerMsg::People { people })
    }
}

#[derive(Default)]
struct AttachmentCounts(Mutex<HashMap<String, u32>>);

impl AttachmentCounts {
    fn attach(&self, user_id: &str) -> io::Result<AttachmentGuard<'_>> {
        let mut counts = self.lock()?;
        let count = counts.entry(user_id.to_owned()).or_default();
        *count = count.saturating_add(1);
        Ok(AttachmentGuard {
            counts: self,
            user_id: user_id.to_owned(),
        })
    }

    fn count(&self, user_id: &str) -> u32 {
        self.0
            .lock()
            .ok()
            .and_then(|counts| counts.get(user_id).copied())
            .unwrap_or(0)
    }

    fn detach(&self, user_id: &str) {
        if let Ok(mut counts) = self.0.lock()
            && let Some(count) = counts.get_mut(user_id)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.remove(user_id);
            }
        }
    }

    fn lock(&self) -> io::Result<std::sync::MutexGuard<'_, HashMap<String, u32>>> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("attachment count lock is poisoned"))
    }
}

struct AttachmentGuard<'a> {
    counts: &'a AttachmentCounts,
    user_id: String,
}

impl Drop for AttachmentGuard<'_> {
    fn drop(&mut self) {
        self.counts.detach(&self.user_id);
    }
}

struct PublishedAddress {
    host: String,
    port: u16,
}

impl PublishedAddress {
    fn parse(value: &str) -> io::Result<Self> {
        let (host, port) = value.rsplit_once(':').ok_or_else(invalid_published_addr)?;
        if host.is_empty() {
            return Err(invalid_published_addr());
        }
        let port = port.parse().map_err(|_| invalid_published_addr())?;
        Ok(Self {
            host: host.to_owned(),
            port,
        })
    }

    fn capsule(&self, token: &str) -> String {
        format!("SEER1-{}-{}-{token}", self.host, self.port)
    }
}

fn invalid_published_addr() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "published_addr must contain a host and port",
    )
}

fn report_connection(result: io::Result<()>) -> bool {
    if let Err(error) = result {
        eprintln!("broker connection error: {error}");
        true
    } else {
        false
    }
}

fn handle_connection(mut stream: TcpStream, broker: &BrokerState) -> io::Result<()> {
    let Some(person) = handshake(&mut stream, broker, HANDSHAKE_TIMEOUT)? else {
        return Ok(());
    };
    stream.set_read_timeout(None)?;
    let _attachment = broker.attachments.attach(&person.user_id)?;
    forward(stream, &person, broker)
}

fn handshake(
    stream: &mut TcpStream,
    broker: &BrokerState,
    timeout: Duration,
) -> io::Result<Option<PersonRecord>> {
    let deadline = Instant::now() + timeout;
    let message = match read_message(stream, deadline) {
        Ok(message) => message,
        Err(error) if connection_was_silent(&error) => return Ok(None),
        Err(_) => return refuse(stream, INVALID_MESSAGE).map(|()| None),
    };

    match message {
        ClientMsg::Hello {
            user_id,
            credential,
        } => authenticate(stream, broker.registry(), &user_id, &credential),
        ClientMsg::Join { seat_token, name } => join(stream, broker.registry(), &seat_token, &name),
        _ => refuse(stream, EXPECTED_HELLO).map(|()| None),
    }
}

fn read_message(stream: &mut TcpStream, deadline: Instant) -> io::Result<ClientMsg> {
    codec::decode(&mut DeadlineReader { stream, deadline })
}

fn authenticate(
    stream: &mut TcpStream,
    registry: &Registry,
    user_id: &str,
    credential: &str,
) -> io::Result<Option<PersonRecord>> {
    let Some(person) = registry.authenticate(user_id, credential)? else {
        return refuse(stream, INVALID_CREDENTIALS).map(|()| None);
    };
    codec::encode(
        stream,
        &ServerMsg::Welcome {
            user_id: person.user_id.clone(),
            name: person.name.clone(),
            client_id: String::new(),
            tree: Tree::new(),
        },
    )?;
    Ok(Some(person))
}

fn join(
    stream: &mut TcpStream,
    registry: &Registry,
    seat_token: &str,
    name: &str,
) -> io::Result<Option<PersonRecord>> {
    let result = match registry.join(seat_token, name)? {
        Ok(result) => result,
        Err(error) => return refuse(stream, error.reason()).map(|()| None),
    };
    codec::encode(
        stream,
        &ServerMsg::Joined {
            user_id: result.person.user_id.clone(),
            credential: result.credential,
            name: result.person.name.clone(),
        },
    )?;
    Ok(Some(result.person))
}

pub(crate) fn refuse(stream: &mut TcpStream, reason: &str) -> io::Result<()> {
    eprintln!("refused connection: {reason}");
    codec::encode(
        stream,
        &ServerMsg::Refused {
            reason: reason.to_owned(),
        },
    )
}

fn connection_was_silent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::UnexpectedEof
    )
}

struct DeadlineReader<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
}

impl Read for DeadlineReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(handshake_timed_out)?;
        self.stream.set_read_timeout(Some(remaining))?;
        self.stream.read(buffer)
    }
}

fn handshake_timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "handshake timed out")
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    use super::{PublishedAddress, connection_was_silent, report_connection};

    #[test]
    fn published_address_creates_the_required_capsule() {
        let published =
            PublishedAddress::parse("seer.example.com:7321").expect("published address must parse");
        assert_eq!(
            published.capsule("token"),
            "SEER1-seer.example.com-7321-token"
        );
        assert!(PublishedAddress::parse("missing-port").is_err());
        assert!(PublishedAddress::parse(":7321").is_err());
        assert!(PublishedAddress::parse("host:bad").is_err());
    }

    #[test]
    fn silent_connection_reaches_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        let address = listener
            .local_addr()
            .expect("listener must have an address");
        let mut client = TcpStream::connect(address).expect("client must connect");
        let (mut server, _) = listener.accept().expect("server must accept client");
        server
            .set_read_timeout(Some(Duration::from_millis(10)))
            .expect("timeout must set");

        let error = super::read_message(&mut server, std::time::Instant::now())
            .expect_err("expired handshake must fail");
        drop(server);
        let mut byte = [0];
        let bytes_read = std::io::Read::read(&mut client, &mut byte)
            .expect("silent connection must close without a response");

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(bytes_read, 0);
    }

    #[test]
    fn identifies_silent_connection_errors() {
        for kind in [
            io::ErrorKind::TimedOut,
            io::ErrorKind::WouldBlock,
            io::ErrorKind::UnexpectedEof,
        ] {
            assert!(connection_was_silent(&io::Error::from(kind)));
        }
        assert!(!connection_was_silent(&io::Error::from(
            io::ErrorKind::InvalidData
        )));
    }

    #[test]
    fn reports_handshake_error() {
        assert!(report_connection(Err(io::Error::other("test error"))));
        assert!(!report_connection(Ok(())));
    }

    #[test]
    fn malformed_message_is_refused() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        let address = listener
            .local_addr()
            .expect("listener must have an address");
        let mut client = TcpStream::connect(address).expect("client must connect");
        let (mut server, _) = listener.accept().expect("server must accept client");

        let worker = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            let result = super::read_message(&mut server, deadline);
            assert!(result.is_err(), "invalid message must fail");
            super::refuse(&mut server, super::INVALID_MESSAGE).expect("refusal must send");
            Ok::<(), io::Error>(())
        });
        std::io::Write::write_all(&mut client, &[0, 0, 0, 1, b'{'])
            .expect("invalid frame must send");
        let response =
            seer_core::proto::codec::decode::<_, seer_core::proto::ServerMsg>(&mut client)
                .expect("refusal must decode");

        assert_eq!(
            response,
            seer_core::proto::ServerMsg::Refused {
                reason: "invalid message".into()
            }
        );
        worker
            .join()
            .expect("worker must not panic")
            .expect("worker must finish");
    }

    #[test]
    fn attachment_counts_return_to_zero() {
        let counts = super::AttachmentCounts::default();
        counts.detach("missing");
        let first = counts.attach("u1").expect("attachment must count");
        let second = counts.attach("u1").expect("attachment must count");
        assert_eq!(counts.count("u1"), 2);
        drop(first);
        assert_eq!(counts.count("u1"), 1);
        drop(second);
        assert_eq!(counts.count("u1"), 0);
    }

    #[test]
    fn authentication_and_join_write_their_success_replies() {
        let directory = test_directory("replies");
        let (registry, credential) =
            crate::registry::Registry::open(&directory, "Owner").expect("registry must open");
        let credential = credential.expect("owner credential must exist");
        let owner = registry.people().expect("people must load")[0].clone();
        let (mut server, mut client) = tcp_pair();

        let authenticated =
            super::authenticate(&mut server, &registry, &owner.user_id, &credential)
                .expect("authentication must finish")
                .expect("owner must authenticate");
        assert_eq!(authenticated.user_id, owner.user_id);
        assert!(matches!(
            seer_core::proto::codec::decode::<_, seer_core::proto::ServerMsg>(&mut client),
            Ok(seer_core::proto::ServerMsg::Welcome { .. })
        ));

        let token = registry.create_seat().expect("seat must be created");
        let joined = super::join(&mut server, &registry, &token, "Guest")
            .expect("join must finish")
            .expect("guest must join");
        assert_eq!(joined.name, "Guest");
        assert!(matches!(
            seer_core::proto::codec::decode::<_, seer_core::proto::ServerMsg>(&mut client),
            Ok(seer_core::proto::ServerMsg::Joined { .. })
        ));

        let (mut closed_server, _closed_client) = tcp_pair();
        closed_server
            .shutdown(std::net::Shutdown::Write)
            .expect("server writes must close");
        assert!(
            super::authenticate(&mut closed_server, &registry, &owner.user_id, &credential)
                .is_err()
        );
        let second_token = registry.create_seat().expect("seat must be created");
        assert!(super::join(&mut closed_server, &registry, &second_token, "Other").is_err());
        std::fs::remove_dir_all(directory).expect("state directory must be removed");
    }

    #[test]
    fn broker_rejects_an_invalid_published_address_before_state_creation() {
        let config = crate::Config {
            listen: "127.0.0.1:0".parse().expect("address must parse"),
            published_addr: "bad".into(),
            state_dir: PathBuf::from("/tmp/not-created-by-invalid-config"),
            owner_name: "Owner".into(),
            shell: "sh".into(),
        };
        assert!(super::BrokerState::new(&config).is_err());

        let directory = test_directory("serve");
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        listener
            .set_nonblocking(true)
            .expect("listener must be nonblocking");
        let config = crate::Config {
            published_addr: "host:7321".into(),
            state_dir: directory.clone(),
            ..config
        };
        assert!(super::serve(listener, &config).is_err());
        std::fs::remove_dir_all(directory).expect("state directory must be removed");
    }

    fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        let address = listener
            .local_addr()
            .expect("listener must have an address");
        let client = TcpStream::connect(address).expect("client must connect");
        let (server, _) = listener.accept().expect("server must accept client");
        (server, client)
    }

    fn test_directory(name: &str) -> PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time must be valid")
            .as_nanos()
            % 1_000_000_000;
        PathBuf::from(format!("/tmp/ss-{name}-{}-{timestamp}", std::process::id()))
    }
}
