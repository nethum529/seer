use iroh::Endpoint;
use iroh::endpoint::{Connection, Incoming, RecvStream, SendStream, presets};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

mod dialer;
mod session;
mod stream;

pub use dialer::dial;
pub use iroh::{EndpointId, SecretKey};
pub use session::Session;
pub use stream::{Socket, Stream};

pub const ALPN: &[u8] = b"seer/1";
const ONLINE_TIMEOUT: Duration = Duration::from_secs(4);
// Issue 410: a dial to a peer that published its address and then stopped
// did not fail within 15 s. A stale address makes a good handshake take about
// 1.2 s (docs/research/20-address-lookup.md).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);

const PRE_STREAM_CONNECTION_LIMIT: usize = 32;
const PRE_STREAM_TIMEOUT: Duration = Duration::from_secs(5);

type AcceptedStream = io::Result<(EndpointId, UnixStream, Session)>;

struct PreStreamLimit(Arc<AtomicUsize>);

impl Default for PreStreamLimit {
    fn default() -> Self {
        Self(Arc::new(AtomicUsize::new(0)))
    }
}

impl PreStreamLimit {
    fn try_acquire(&self) -> Option<PreStreamGuard> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < PRE_STREAM_CONNECTION_LIMIT).then_some(active + 1)
            })
            .ok()?;
        Some(PreStreamGuard(Arc::clone(&self.0)))
    }
}

struct PreStreamGuard(Arc<AtomicUsize>);

impl Drop for PreStreamGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn load_or_create_secret_key(path: &Path) -> io::Result<SecretKey> {
    match fs::read(path) {
        Ok(bytes) => secret_key_from_bytes(&bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => create_secret_key(path),
        Err(_) => Err(io::Error::other("could not read secret key")),
    }
}

pub fn encode_endpoint_id(id: EndpointId) -> String {
    id.to_string()
}

pub fn decode_endpoint_id(value: &str) -> io::Result<EndpointId> {
    EndpointId::from_str(value).map_err(|_| io::Error::other("endpoint ID is invalid"))
}

pub struct Listener {
    id: EndpointId,
    accepted: Receiver<AcceptedStream>,
    control: UnixStream,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    pub fn bind(secret_key: SecretKey) -> io::Result<Self> {
        let (control, worker_control) = UnixStream::pair()?;
        worker_control.set_nonblocking(true)?;
        let (ready_tx, ready_rx) = mpsc::channel();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("seer-net-listener".to_owned())
            .spawn(move || listener_thread(secret_key, worker_control, ready_tx, accepted_tx))?;

        match ready_rx.recv_timeout(ONLINE_TIMEOUT) {
            Ok(Ok(id)) => Ok(Self {
                id,
                accepted: accepted_rx,
                control,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
                Err(io::Error::other("listener thread stopped"))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "endpoint did not become ready within 4 seconds",
            )),
        }
    }

    pub fn id(&self) -> EndpointId {
        self.id
    }

    pub fn accept(&self) -> AcceptedStream {
        self.accepted
            .recv()
            .map_err(|_| io::Error::other("listener stopped"))?
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.control.write_all(&[0]);
        let _ = self.control.shutdown(std::net::Shutdown::Both);
        self.thread.take();
    }
}

/// Opens one persistent connection that both sides can add streams to.
///
/// The runtime uses this to reach the broker. Dropping the session closes the
/// connection.
pub fn dial_session(secret_key: SecretKey, remote: EndpointId) -> io::Result<Session> {
    let (control, worker_control) = UnixStream::pair()?;
    worker_control.set_nonblocking(true)?;
    let (session, parts) = session::session_pair(Some(control));
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::Builder::new()
        .name("seer-net-session".to_owned())
        .spawn(move || {
            run_attempt(
                &ready_tx,
                run_session(secret_key, remote, worker_control, &ready_tx, parts),
            );
        })?;
    ready_rx
        .recv()
        .map_err(|_| io::Error::other("session thread stopped"))??;
    Ok(session)
}

// The error goes back only after the runtime is dropped, so a failed attempt
// holds no endpoint when the caller retries with the same key.
fn run_attempt<T>(reply: &Sender<io::Result<T>>, attempt: impl Future<Output = io::Result<()>>) {
    let outcome = build_runtime().and_then(|runtime| runtime.block_on(attempt));
    if let Err(error) = outcome {
        let _ = reply.send(Err(error));
    }
}

async fn run_session(
    secret_key: SecretKey,
    remote: EndpointId,
    control: UnixStream,
    ready: &Sender<io::Result<()>>,
    parts: session::SessionParts,
) -> io::Result<()> {
    let control = tokio::net::UnixStream::from_std(control)
        .map_err(|_| io::Error::other("could not open session control"))?;
    let endpoint = bind_endpoint(secret_key).await?;
    let connection = connect(&endpoint, remote).await?;
    if ready.send(Ok(())).is_ok() {
        tokio::select! {
            () = session::serve_connection(connection.clone(), parts) => {}
            () = session::shutdown_on_control(control) => {}
        }
        connection.close(0_u8.into(), b"seer session closed");
    }
    endpoint.close().await;
    Ok(())
}

fn create_secret_key(path: &Path) -> io::Result<SecretKey> {
    let secret_key = SecretKey::generate();
    let open_result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path);
    match open_result {
        Ok(mut file) => {
            file.write_all(&secret_key.to_bytes())?;
            file.sync_all()?;
            Ok(secret_key)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let bytes = fs::read(path)?;
            secret_key_from_bytes(&bytes)
        }
        Err(_) => Err(io::Error::other("could not create secret key")),
    }
}

fn secret_key_from_bytes(bytes: &[u8]) -> io::Result<SecretKey> {
    SecretKey::try_from(bytes).map_err(|_| io::Error::other("secret key must contain 32 bytes"))
}

fn listener_thread(
    secret_key: SecretKey,
    control: UnixStream,
    ready: Sender<io::Result<EndpointId>>,
    accepted: Sender<AcceptedStream>,
) {
    let runtime = match build_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    runtime.block_on(run_listener(secret_key, control, ready, accepted));
}

async fn run_listener(
    secret_key: SecretKey,
    control: UnixStream,
    ready: Sender<io::Result<EndpointId>>,
    accepted: Sender<AcceptedStream>,
) {
    let mut control = match tokio::net::UnixStream::from_std(control) {
        Ok(control) => control,
        Err(_) => {
            let _ = ready.send(Err(io::Error::other("could not open listener control")));
            return;
        }
    };
    let endpoint = match bind_endpoint(secret_key).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let mut control_byte = [0_u8; 1];
    tokio::select! {
        _ = endpoint.online() => {}
        _ = control.read(&mut control_byte) => {
            endpoint.close().await;
            return;
        }
    }
    if ready.send(Ok(endpoint.id())).is_err() {
        endpoint.close().await;
        return;
    }

    let mut connections = tokio::task::JoinSet::new();
    let pre_streams = PreStreamLimit::default();
    loop {
        tokio::select! {
            _ = control.read(&mut control_byte) => break,
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    break;
                };
                let Some(pre_stream) = pre_streams.try_acquire() else {
                    incoming.refuse();
                    continue;
                };
                let result_sender = accepted.clone();
                connections.spawn(handle_incoming(incoming, result_sender, pre_stream));
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    while connections.join_next().await.is_some() {}
    endpoint.close().await;
}

async fn bind_endpoint(secret_key: SecretKey) -> io::Result<Endpoint> {
    Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|_| io::Error::other("could not bind endpoint"))
}

// On failure dial_session drops its endpoint instead of closing it. No peer
// answered, and a close waits about 3 s for the abandoned handshake to drain.
async fn connect(endpoint: &Endpoint, remote: EndpointId) -> io::Result<Connection> {
    match timeout(CONNECT_TIMEOUT, endpoint.connect(remote, ALPN)).await {
        Ok(Ok(connection)) => Ok(connection),
        Ok(Err(_)) => Err(io::Error::other("could not connect to endpoint")),
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "could not connect to endpoint within 6 seconds",
        )),
    }
}

async fn handle_incoming(
    incoming: Incoming,
    accepted: Sender<AcceptedStream>,
    pre_stream: PreStreamGuard,
) {
    let connection = match timeout(PRE_STREAM_TIMEOUT, incoming).await {
        Ok(Ok(connection)) => connection,
        _ => return,
    };
    let remote = connection.remote_id();
    let (send, recv) = match timeout(PRE_STREAM_TIMEOUT, connection.accept_bi()).await {
        Ok(Ok(streams)) => streams,
        _ => {
            connection.close(0_u8.into(), b"seer stream deadline");
            return;
        }
    };
    drop(pre_stream);
    let (caller_stream, bridge_stream) = match stream_pair() {
        Ok(streams) => streams,
        Err(_) => return,
    };
    let (session, parts) = session::session_pair(None);
    if accepted.send(Ok((remote, caller_stream, session))).is_err() {
        connection.close(0_u8.into(), b"seer listener stopped");
        return;
    }
    let first = tokio::spawn(async move {
        let _ = bridge(send, recv, bridge_stream).await;
    });
    session::serve_connection(connection, parts).await;
    first.abort();
}

fn build_runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| io::Error::other("could not start network runtime"))
}

pub(crate) fn stream_pair() -> io::Result<(UnixStream, tokio::net::UnixStream)> {
    let (caller_stream, bridge_stream) = UnixStream::pair()?;
    bridge_stream.set_nonblocking(true)?;
    let bridge_stream = tokio::net::UnixStream::from_std(bridge_stream)?;
    Ok((caller_stream, bridge_stream))
}

pub(crate) async fn bridge(
    send: SendStream,
    recv: RecvStream,
    unix: tokio::net::UnixStream,
) -> io::Result<()> {
    let (unix_read, unix_write) = unix.into_split();
    let mut to_unix = tokio::spawn(copy_to_unix(recv, unix_write));
    let mut to_quic = tokio::spawn(copy_to_quic(unix_read, send));

    tokio::select! {
        result = &mut to_unix => {
            to_quic.abort();
            task_result(result)
        }
        result = &mut to_quic => {
            to_unix.abort();
            task_result(result)
        }
    }
}

async fn copy_to_unix(
    mut recv: RecvStream,
    mut unix: tokio::net::unix::OwnedWriteHalf,
) -> io::Result<()> {
    tokio::io::copy(&mut recv, &mut unix).await?;
    unix.shutdown().await
}

async fn copy_to_quic(
    mut unix: tokio::net::unix::OwnedReadHalf,
    mut send: SendStream,
) -> io::Result<()> {
    tokio::io::copy(&mut unix, &mut send).await?;
    send.shutdown().await?;
    send.stopped()
        .await
        .map_err(|_| io::Error::other("QUIC send did not close"))?;
    Ok(())
}

fn task_result(result: Result<io::Result<()>, tokio::task::JoinError>) -> io::Result<()> {
    result.map_err(|_| io::Error::other("bridge task stopped"))?
}
