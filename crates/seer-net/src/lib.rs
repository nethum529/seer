use iroh::Endpoint;
use iroh::endpoint::{Incoming, RecvStream, SendStream, presets};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

mod stream;

pub use iroh::{EndpointId, SecretKey};
pub use stream::{Socket, Stream};

pub const ALPN: &[u8] = b"seer/1";

const PRE_STREAM_CONNECTION_LIMIT: usize = 32;
const PRE_STREAM_TIMEOUT: Duration = Duration::from_secs(5);

type AcceptedStream = io::Result<(EndpointId, UnixStream)>;

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

        match ready_rx.recv() {
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
            Err(_) => {
                let _ = thread.join();
                Err(io::Error::other("listener thread stopped"))
            }
        }
    }

    pub fn id(&self) -> EndpointId {
        self.id
    }

    pub fn accept(&self) -> AcceptedStream {
        // Only a stopped listener makes accept return an error.
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

pub fn dial(secret_key: SecretKey, remote: EndpointId) -> io::Result<UnixStream> {
    let (result_tx, result_rx) = mpsc::channel();
    thread::Builder::new()
        .name("seer-net-dialer".to_owned())
        .spawn(move || dialer_thread(secret_key, remote, result_tx))?;
    result_rx
        .recv()
        .map_err(|_| io::Error::other("dialer thread stopped"))?
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
    endpoint.online().await;
    if ready.send(Ok(endpoint.id())).is_err() {
        endpoint.close().await;
        return;
    }

    let mut control_byte = [0_u8; 1];
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
        Err(_) => {
            connection.close(0_u8.into(), b"seer stream unavailable");
            return;
        }
    };
    if accepted.send(Ok((remote, caller_stream))).is_ok() {
        let _ = bridge(send, recv, bridge_stream).await;
    } else {
        connection.close(0_u8.into(), b"seer listener stopped");
    }
}

fn dialer_thread(
    secret_key: SecretKey,
    remote: EndpointId,
    result: Sender<io::Result<UnixStream>>,
) {
    let runtime = match build_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = result.send(Err(error));
            return;
        }
    };
    runtime.block_on(run_dialer(secret_key, remote, result));
}

async fn run_dialer(
    secret_key: SecretKey,
    remote: EndpointId,
    result: Sender<io::Result<UnixStream>>,
) {
    let endpoint = match bind_endpoint(secret_key).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            let _ = result.send(Err(error));
            return;
        }
    };
    let connection = match endpoint.connect(remote, ALPN).await {
        Ok(connection) => connection,
        Err(_) => {
            let _ = result.send(Err(io::Error::other("could not connect to endpoint")));
            endpoint.close().await;
            return;
        }
    };
    let (send, recv) = match connection.open_bi().await {
        Ok(streams) => streams,
        Err(_) => {
            let _ = result.send(Err(io::Error::other("could not open stream")));
            endpoint.close().await;
            return;
        }
    };
    let (caller_stream, bridge_stream) = match stream_pair() {
        Ok(streams) => streams,
        Err(error) => {
            let _ = result.send(Err(error));
            endpoint.close().await;
            return;
        }
    };
    if result.send(Ok(caller_stream)).is_ok() {
        let _ = bridge(send, recv, bridge_stream).await;
    }
    endpoint.close().await;
}

fn build_runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| io::Error::other("could not start network runtime"))
}

fn stream_pair() -> io::Result<(UnixStream, tokio::net::UnixStream)> {
    let (caller_stream, bridge_stream) = UnixStream::pair()?;
    bridge_stream.set_nonblocking(true)?;
    let bridge_stream = tokio::net::UnixStream::from_std(bridge_stream)?;
    Ok((caller_stream, bridge_stream))
}

async fn bridge(
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

#[cfg(test)]
mod tests {
    use super::{PRE_STREAM_CONNECTION_LIMIT, PreStreamLimit};

    #[test]
    fn pre_stream_limit_releases_after_connection_task_finishes() {
        let limit = PreStreamLimit::default();
        let guards = (0..PRE_STREAM_CONNECTION_LIMIT)
            .map(|_| limit.try_acquire().expect("pre-stream slot must be available"))
            .collect::<Vec<_>>();
        assert!(limit.try_acquire().is_none());
        drop(guards);
        assert!(limit.try_acquire().is_some());
    }
}
