use std::fs;
use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, ServerMsg, codec};
use sha2::{Digest, Sha256};

use super::binary;

pub(crate) const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(25);
static NEXT: AtomicUsize = AtomicUsize::new(0);
static BROKER: OnceLock<PathBuf> = OnceLock::new();
static RUNTIME: OnceLock<PathBuf> = OnceLock::new();

pub(crate) const ALICE_SECRET: &str = "alice-secret";
pub(crate) const BOB_SECRET: &str = "bob-secret";

/// One room on this machine: a broker plus the runtimes people publish to it.
///
/// Nothing here starts a shell on the broker. Each runtime is a separate
/// process that connects outward, which is what the product does.
pub(crate) struct Room {
    pub(crate) root: PathBuf,
    pub(crate) address: SocketAddr,
    config: PathBuf,
    broker: Option<Child>,
    runtimes: Vec<Child>,
}

impl Room {
    pub(crate) fn start() -> Self {
        let counter = NEXT.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must be after the epoch")
            .as_nanos()
            % 1_000_000_000;
        let root = PathBuf::from(format!(
            "/tmp/seer-room-{}-{counter}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("room directory must be created");
        let state_dir = root.join("state");
        fs::create_dir(&state_dir).expect("state directory must be created");
        write_people(&state_dir);
        let address = unused_address();
        let config = root.join("broker.toml");
        fs::write(
            &config,
            format!(
                "listen = \"{address}\"\npublished_addr = \"{address}\"\nremote = false\nstate_dir = \"{}\"\nowner_name = \"alice\"\n",
                state_dir.display()
            ),
        )
        .expect("broker config must write");
        let mut room = Self {
            root,
            address,
            config,
            broker: None,
            runtimes: Vec::new(),
        };
        room.start_broker();
        room
    }

    pub(crate) fn start_broker(&mut self) {
        let log = fs::File::create(self.root.join("broker.log")).expect("broker log must open");
        let errors = log.try_clone().expect("broker log must clone");
        let child = Command::new(BROKER.get_or_init(|| binary::build("seer-broker")))
            .arg(&self.config)
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(errors))
            .spawn()
            .expect("broker must start");
        self.broker = Some(child);
        wait_for_port(self.address);
    }

    pub(crate) fn stop_broker(&mut self) {
        if let Some(mut child) = self.broker.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Starts a runtime on this machine for one person and waits until the
    /// broker reports it published.
    pub(crate) fn publish(&mut self, user: &str, credential: &str) -> UnixStream {
        let directory = self.root.join(user);
        fs::create_dir_all(&directory).expect("runtime directory must be created");
        let socket = directory.join("socket");
        let child = Command::new(RUNTIME.get_or_init(|| binary::build("seer-runtime")))
            .arg(&socket)
            .arg(user)
            .arg("sh")
            .arg(format!("gen-{user}"))
            .env("SEER_SNAPSHOT_DIR", &directory)
            .env("SEER_ROOM_ENDPOINT", self.address.to_string())
            .env("SEER_ROOM_CREDENTIAL", credential)
            .env("SEER_ROOM_KEY", directory.join("runtime.key"))
            .current_dir(&directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("runtime must start");
        self.runtimes.push(child);
        own_window(&socket)
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.stop_broker();
        for mut child in self.runtimes.drain(..) {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// The person's own window: a private local socket, never the room.
pub(crate) fn own_window(socket: &std::path::Path) -> UnixStream {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Ok(mut stream) = UnixStream::connect(socket) {
            stream
                .set_read_timeout(Some(WAIT))
                .expect("read timeout must set");
            if let Ok(ServerMsg::RuntimeReady { .. }) = codec::decode::<_, ServerMsg>(&mut stream) {
                codec::encode(&mut stream, &ClientMsg::AttachRuntime).expect("attach must send");
                return stream;
            }
        }
        thread::sleep(POLL);
    }
    panic!("the local runtime did not answer");
}

pub(crate) fn own_tree(window: &mut UnixStream) -> Tree {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let ServerMsg::Tree { tree } = decode(window)
            && tree
                .workspaces
                .first()
                .is_some_and(|workspace| !workspace.tabs.is_empty())
        {
            return tree;
        }
    }
    panic!("the local runtime did not send a tree with a terminal");
}

pub(crate) fn join_room(address: SocketAddr, user: &str, credential: &str) -> TcpStream {
    let mut stream = TcpStream::connect(address).expect("room must accept");
    stream
        .set_read_timeout(Some(WAIT))
        .expect("read timeout must set");
    codec::encode(
        &mut stream,
        &ClientMsg::Hello {
            user_id: user.into(),
            credential: credential.into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
    )
    .expect("hello must send");
    let ServerMsg::Welcome { .. } = decode(&mut stream) else {
        panic!("the room must welcome a known person");
    };
    stream
}

pub(crate) fn decode<S: Read>(stream: &mut S) -> ServerMsg {
    codec::decode(stream).expect("a room message must decode")
}

pub(crate) fn send<S: io::Write>(stream: &mut S, message: &ClientMsg) {
    codec::encode(stream, message).expect("a message must send");
}

pub(crate) fn wait_for_published(stream: &mut TcpStream, user: &str) {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        send(stream, &ClientMsg::ListPeople);
        if let ServerMsg::People { people } = wait_for(stream, |message| {
            matches!(message, ServerMsg::People { .. })
        }) && published(&people, user)
        {
            return;
        }
        thread::sleep(POLL);
    }
    panic!("{user} never published a runtime to the room");
}

fn published(people: &[Person], user: &str) -> bool {
    people
        .iter()
        .any(|person| person.user_id == user && person.peekable)
}

pub(crate) fn wait_for(stream: &mut TcpStream, matches: impl Fn(&ServerMsg) -> bool) -> ServerMsg {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let message = decode(stream);
        if matches(&message) {
            return message;
        }
    }
    panic!("the expected room message never arrived");
}

pub(crate) fn unused_address() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .expect("port probe must bind")
        .local_addr()
        .expect("port probe must have an address")
}

fn wait_for_port(address: SocketAddr) {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if TcpStream::connect(address).is_ok() {
            return;
        }
        thread::sleep(POLL);
    }
    panic!("the broker never opened its port");
}

fn write_people(state_dir: &std::path::Path) {
    let alice = hash(ALICE_SECRET);
    let bob = hash(BOB_SECRET);
    let people = format!(
        "[{{\"user_id\":\"alice\",\"name\":\"alice\",\"credential_hash\":\"{alice}\",\"created_at\":1,\"is_owner\":true}},\
{{\"user_id\":\"bob\",\"name\":\"bob\",\"credential_hash\":\"{bob}\",\"created_at\":2,\"is_owner\":false}}]\n"
    );
    fs::write(state_dir.join("people.json"), people).expect("people registry must write");
    fs::write(state_dir.join("seats.json"), "[]\n").expect("seat registry must write");
}

fn hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}
