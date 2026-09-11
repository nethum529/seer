use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientInfo, ClientMsg, Person, PersonState, ServerMsg, codec};
use seer_net::{EndpointId, Listener, Session, Socket, Stream, load_or_create_secret_key};

use crate::attachments::{AttachmentGuard, Attachments, ClientWriter};
use crate::forwarding::forward;
use crate::published_address::PublishedAddress;
use crate::publishing::{claim_runtime_stream, publish_handshake, serve_runtime};
use crate::registry::{MAX_SEAT_LIFETIME_SECS, PersonRecord, Registry};
use crate::runtime::RuntimeManager;
use crate::{
    Config,
    connection_limit::{ConnectionGuard, ConnectionKey, ConnectionLimit},
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const STATUS_INTERVAL: Duration = Duration::from_secs(1);
const STATUS_TIMEOUT: Duration = Duration::from_millis(500);
const ACTIVE_LIMIT_SECS: u64 = 60;
const IDLE_LIMIT_SECS: u64 = 600;
pub(crate) const INVALID_CREDENTIALS: &str = "invalid credentials";
const EXPECTED_HELLO: &str = "expected Hello";
const INVALID_MESSAGE: &str = "invalid message";

pub fn serve(
    listener: TcpListener,
    remote_listener: Option<Listener>,
    config: &Config,
) -> io::Result<()> {
    let (mut broker, owner_identity) = BrokerState::new(config)?;
    broker.remote_endpoint = remote_listener.as_ref().map(Listener::id);
    if let Some((user_id, credential)) = owner_identity {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "owner-id: {user_id}")?;
        writeln!(stdout, "owner-credential: {credential}")?;
    }
    let broker = Arc::new(broker);
    spawn_status_ticker(Arc::clone(&broker));
    if let Some(remote_listener) = remote_listener {
        spawn_remote_accept_loop(remote_listener, Arc::clone(&broker));
    }
    lifecycle::accept_loop(listener, broker)?;
    match std::fs::remove_file(config.state_dir.join("broker.pid")) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

mod lifecycle;

pub(crate) struct BrokerState {
    stopping: std::sync::atomic::AtomicBool,
    registry: Registry,
    pub(crate) grants: crate::grants::Grants,
    runtimes: RuntimeManager,
    attachments: Attachments,
    published: PublishedAddress,
    remote_endpoint: Option<EndpointId>,
    connection_limit: ConnectionLimit,
    statuses: Mutex<HashMap<String, ServerMsg>>,
    published_people: Mutex<Vec<Person>>,
}

impl BrokerState {
    pub(crate) fn new(config: &Config) -> io::Result<(Self, Option<(String, String)>)> {
        let published = PublishedAddress::parse(&config.published_addr)?;
        let runtimes = RuntimeManager::default();
        let (registry, owner_identity) = Registry::open(&config.state_dir, &config.owner_name)?;
        Ok((
            Self {
                stopping: std::sync::atomic::AtomicBool::new(false),
                grants: crate::grants::Grants::open(&config.state_dir, &registry.people()?)?,
                registry,
                runtimes,
                attachments: Attachments::default(),
                published,
                remote_endpoint: None,
                connection_limit: ConnectionLimit::default(),
                statuses: Mutex::new(HashMap::new()),
                published_people: Mutex::new(Vec::new()),
            },
            owner_identity,
        ))
    }

    pub(crate) fn registry(&self) -> &Registry {
        &self.registry
    }

    pub(crate) fn runtimes(&self) -> &RuntimeManager {
        &self.runtimes
    }

    pub(crate) fn invite(&self, hours: Option<u32>) -> io::Result<ServerMsg> {
        let requested_lifetime = hours.map_or_else(Registry::seat_lifetime_secs, |hours| {
            u64::from(hours).saturating_mul(3_600)
        });
        let lifetime_secs = requested_lifetime.min(MAX_SEAT_LIFETIME_SECS);
        let token = self.registry.create_seat(lifetime_secs)?;
        let capsule = match self.remote_endpoint {
            Some(endpoint) => format!("SEER2-{endpoint}-{token}"),
            None => self.published.capsule(&token),
        };
        Ok(ServerMsg::Seat {
            capsule,
            expires_in_secs: lifetime_secs,
        })
    }

    pub(crate) fn people(&self) -> io::Result<ServerMsg> {
        let statuses = lock_statuses(&self.statuses)?;
        let people = self
            .registry
            .people()?
            .into_iter()
            .map(|person| {
                let attached_clients = self.attachments.count(&person.user_id);
                let (tabs, foreground, idle_secs) = match statuses.get(&person.user_id) {
                    Some(ServerMsg::Status {
                        tabs,
                        foreground,
                        idle_secs,
                    }) => (*tabs, foreground.clone(), *idle_secs),
                    _ => (0, String::new(), 0),
                };
                Person {
                    online: attached_clients > 0,
                    attached_clients,
                    peekable: self.runtimes.is_running(&person.user_id),
                    host: person.is_owner,
                    state: person_state(attached_clients, idle_secs),
                    tabs,
                    foreground,
                    idle_secs,
                    user_id: person.user_id,
                    name: person.name,
                }
            })
            .collect();
        Ok(ServerMsg::People { people })
    }

    fn refresh_statuses(&self) -> io::Result<()> {
        let mut statuses = HashMap::new();
        for person in self.registry.people()? {
            if !self.runtimes.is_running(&person.user_id) {
                continue;
            }
            if let Some(status) = self.query_status(&person.user_id) {
                statuses.insert(person.user_id, status);
            }
        }
        *lock_statuses(&self.statuses)? = statuses;
        self.publish_people()
    }

    fn query_status(&self, user_id: &str) -> Option<ServerMsg> {
        let mut stream = self.runtimes.open(user_id).ok()?;
        stream.set_read_timeout(Some(STATUS_TIMEOUT)).ok()?;
        codec::encode(&mut stream, &ClientMsg::QueryStatus).ok()?;
        let ServerMsg::RuntimeReady { .. } = codec::decode(&mut stream).ok()? else {
            return None;
        };
        let status = codec::decode(&mut stream).ok()?;
        let _ = stream.shutdown(std::net::Shutdown::Both);
        matches!(status, ServerMsg::Status { .. }).then_some(status)
    }

    pub(crate) fn publish_people(&self) -> io::Result<()> {
        let ServerMsg::People { people } = self.people()? else {
            return Ok(());
        };
        let mut published = self
            .published_people
            .lock()
            .map_err(|_| io::Error::other("published people lock is poisoned"))?;
        if *published == people {
            return Ok(());
        }
        for person in &people {
            if !published.iter().any(|old| {
                old.user_id == person.user_id
                    && old.online == person.online
                    && old.idle_secs == person.idle_secs
            }) {
                self.attachments.broadcast(&ServerMsg::Presence {
                    user: person.user_id.clone(),
                    online: person.online,
                    idle_secs: person.idle_secs,
                })?;
            }
        }
        published.clone_from(&people);
        drop(published);
        self.attachments.broadcast(&ServerMsg::People { people })
    }

    pub(crate) fn publish_grants(&self) -> io::Result<()> {
        for person in self.registry.people()? {
            self.attachments
                .send_to_user(&person.user_id, &self.grants.message(&person.user_id)?)?;
        }
        Ok(())
    }

    pub(crate) fn attach_client(
        &self,
        user_id: &str,
        writer: ClientWriter,
    ) -> io::Result<AttachmentGuard<'_>> {
        self.attachments.attach(user_id, writer)
    }

    pub(crate) fn clients(&self, user_id: &str, excluded: &str) -> io::Result<Vec<ClientInfo>> {
        self.attachments.clients(user_id, excluded)
    }

    pub(crate) fn detach_client(&self, user_id: &str, client_id: &str) -> io::Result<bool> {
        self.attachments.detach_client(user_id, client_id)
    }
}

fn person_state(attached_clients: u32, idle_secs: u64) -> PersonState {
    if attached_clients == 0 {
        PersonState::Away
    } else if idle_secs < ACTIVE_LIMIT_SECS {
        PersonState::Active
    } else if idle_secs < IDLE_LIMIT_SECS {
        PersonState::Idle
    } else {
        PersonState::Away
    }
}

fn lock_statuses(
    statuses: &Mutex<HashMap<String, ServerMsg>>,
) -> io::Result<std::sync::MutexGuard<'_, HashMap<String, ServerMsg>>> {
    statuses
        .lock()
        .map_err(|_| io::Error::other("status lock is poisoned"))
}

fn spawn_status_ticker(broker: Arc<BrokerState>) {
    thread::spawn(move || {
        while !broker.is_stopping() {
            thread::sleep(STATUS_INTERVAL);
            let _ = broker.refresh_statuses();
        }
    });
}

pub(crate) fn bind_remote_listener(config: &Config) -> io::Result<Option<Listener>> {
    if !config.remote {
        return Ok(None);
    }
    let listener =
        load_or_create_secret_key(&config.state_dir.join("iroh.key")).and_then(Listener::bind)?;
    Ok(Some(listener))
}

fn spawn_remote_accept_loop(listener: Listener, broker: Arc<BrokerState>) {
    thread::spawn(move || {
        loop {
            match listener.accept() {
                Ok((remote, stream, session)) => spawn_connection(
                    Socket::from(stream),
                    Arc::clone(&broker),
                    ConnectionKey::Relay(remote),
                    Some(session),
                ),
                Err(error) => {
                    eprintln!("broker remote accept error: {error}");
                    return;
                }
            }
        }
    });
}

fn spawn_connection(
    stream: Socket,
    broker: Arc<BrokerState>,
    source: ConnectionKey,
    session: Option<Session>,
) {
    let Some(connection) = broker.connection_limit.try_acquire_for(source) else {
        eprintln!("broker refused connection: connection limit reached");
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return;
    };
    thread::spawn(move || {
        report_connection(handle_connection(stream, &broker, connection, session));
    });
}

fn report_connection(result: io::Result<()>) -> bool {
    if let Err(error) = result {
        eprintln!("broker connection error: {error}");
        true
    } else {
        false
    }
}

fn handle_connection(
    mut stream: Socket,
    broker: &Arc<BrokerState>,
    connection: ConnectionGuard,
    session: Option<Session>,
) -> io::Result<()> {
    match handshake(&mut stream, broker, HANDSHAKE_TIMEOUT)? {
        Handshake::Client(person) => {
            drop(connection);
            stream.set_read_timeout(None)?;
            forward(stream, &person, broker)
        }
        Handshake::Runtime {
            user_id,
            generation,
        } => {
            drop(connection);
            serve_runtime(stream, broker, &user_id, &generation, session)
        }
        Handshake::Done => Ok(()),
    }
}

pub(crate) enum Handshake {
    Client(PersonRecord),
    Runtime { user_id: String, generation: String },
    Done,
}

fn handshake(
    stream: &mut Socket,
    broker: &BrokerState,
    timeout: Duration,
) -> io::Result<Handshake> {
    let deadline = Instant::now() + timeout;
    let message = match read_message(stream, deadline) {
        Ok(message) => message,
        Err(error) if connection_was_silent(&error) => return Ok(Handshake::Done),
        Err(_) => return refuse(stream, INVALID_MESSAGE).map(|()| Handshake::Done),
    };

    match message {
        ClientMsg::Hello {
            user_id,
            credential,
            version,
        } => match check_version(stream, &version)? {
            true => authenticate(stream, broker.registry(), &user_id, &credential)
                .map(|person| person.map_or(Handshake::Done, Handshake::Client)),
            false => Ok(Handshake::Done),
        },
        ClientMsg::PublishRuntime {
            user_id,
            credential,
            version,
            generation,
        } => match check_version(stream, &version)? {
            true => publish_handshake(stream, broker, user_id, &credential, generation),
            false => Ok(Handshake::Done),
        },
        ClientMsg::RuntimeStream {
            user_id,
            credential,
            token,
        } => claim_runtime_stream(stream, broker, &user_id, &credential, &token)
            .map(|()| Handshake::Done),
        ClientMsg::Join { seat_token, name } => {
            join(stream, broker.registry(), &seat_token, &name).map(|()| Handshake::Done)
        }
        _ => refuse(stream, EXPECTED_HELLO).map(|()| Handshake::Done),
    }
}

fn check_version(stream: &mut Socket, version: &str) -> io::Result<bool> {
    let server_version = env!("CARGO_PKG_VERSION");
    if major_minor(version) == major_minor(server_version) {
        return Ok(true);
    }
    let reason =
        format!("version mismatch: server {server_version}, client {version}. Run: seer update");
    refuse(stream, &reason).map(|()| false)
}

fn major_minor(version: &str) -> Option<(&str, &str)> {
    let (major, remainder) = version.split_once('.')?;
    let (minor, _) = remainder.split_once('.')?;
    Some((major, minor))
}

pub(crate) fn read_message<S: Stream>(stream: &mut S, deadline: Instant) -> io::Result<ClientMsg> {
    codec::decode_with_limit(
        &mut DeadlineReader { stream, deadline },
        codec::MAX_PRE_AUTH_FRAME_SIZE,
    )
}

pub(crate) fn authenticate<S: Stream>(
    stream: &mut S,
    registry: &Registry,
    user_id: &str,
    credential: &str,
) -> io::Result<Option<PersonRecord>> {
    let Some(person) = registry.authenticate(user_id, credential)? else {
        return refuse(stream, INVALID_CREDENTIALS).map(|()| None);
    };
    Ok(Some(person))
}

fn join<S: Stream>(
    stream: &mut S,
    registry: &Registry,
    seat_token: &str,
    name: &str,
) -> io::Result<()> {
    let result = match registry.join(seat_token, name)? {
        Ok(result) => result,
        Err(error) => return refuse(stream, error.reason()),
    };
    codec::encode(
        stream,
        &ServerMsg::Joined {
            user_id: result.person.user_id,
            credential: result.credential,
            name: result.person.name,
        },
    )
}

pub(crate) fn refuse<S: Stream>(stream: &mut S, reason: &str) -> io::Result<()> {
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

struct DeadlineReader<'a, S> {
    stream: &'a mut S,
    deadline: Instant,
}

impl<S: Stream> Read for DeadlineReader<'_, S> {
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
    use std::path::PathBuf;

    #[test]
    fn broker_rejects_an_invalid_published_address_before_state_creation() {
        let config = crate::Config {
            listen: "127.0.0.1:0".parse().expect("address must parse"),
            published_addr: "bad".into(),
            remote: false,
            state_dir: PathBuf::from("/tmp/not-created-by-invalid-config"),
            owner_name: "Owner".into(),
        };
        assert!(super::BrokerState::new(&config).is_err());
    }
}
