use iroh::Endpoint;
use iroh::endpoint::{Incoming, RecvStream, SendStream, presets};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod stream;

pub use iroh::{EndpointId, SecretKey};
pub use stream::Stream;

pub const ALPN: &[u8] = b"seer/1";

type AcceptedStream = io::Result<(EndpointId, UnixStream)>;

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
    loop {
        tokio::select! {
            _ = control.read(&mut control_byte) => break,
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    break;
                };
                let result_sender = accepted.clone();
                connections.spawn(handle_incoming(incoming, result_sender));
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

async fn handle_incoming(incoming: Incoming, accepted: Sender<AcceptedStream>) {
    let connection = match incoming.await {
        Ok(connection) => connection,
        Err(_) => {
            let _ = accepted.send(Err(io::Error::other("could not accept connection")));
            return;
        }
    };
    let remote = connection.remote_id();
    let (send, recv) = match connection.accept_bi().await {
        Ok(streams) => streams,
        Err(_) => {
            let _ = accepted.send(Err(io::Error::other("could not accept stream")));
            return;
        }
    };
    let (caller_stream, bridge_stream) = match stream_pair() {
        Ok(streams) => streams,
        Err(error) => {
            let _ = accepted.send(Err(error));
            return;
        }
    };
    if accepted.send(Ok((remote, caller_stream))).is_ok() {
        let _ = bridge(send, recv, bridge_stream).await;
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
