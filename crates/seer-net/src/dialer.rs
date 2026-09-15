use crate::{bind_endpoint, bridge, build_runtime, connect, stream_pair};
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointId, SecretKey};
use std::io;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use tokio::runtime::Runtime;

// Issue 414: all dials in one process share one runtime and one endpoint for
// each key. The endpoint stays until the process exits, so a later dial does
// not bind again (docs/research/19-connection-reuse.md). Each dial still makes
// its own connection.
static RUNTIME: Mutex<Option<&'static Runtime>> = Mutex::new(None);
// Keeps one endpoint for each key for the life of the process, so callers
// must reuse one key.
static ENDPOINTS: tokio::sync::Mutex<Vec<Endpoint>> = tokio::sync::Mutex::const_new(Vec::new());

pub fn dial(secret_key: SecretKey, remote: EndpointId) -> io::Result<UnixStream> {
    let (result_tx, result_rx) = mpsc::channel();
    runtime()?.spawn(run_dialer(secret_key, remote, result_tx));
    result_rx
        .recv()
        .map_err(|_| io::Error::other("dialer task stopped"))?
}

fn runtime() -> io::Result<&'static Runtime> {
    let mut runtime = RUNTIME
        .lock()
        .map_err(|_| io::Error::other("network runtime lock is poisoned"))?;
    if let Some(runtime) = *runtime {
        return Ok(runtime);
    }
    let built = Box::leak(Box::new(build_runtime()?));
    *runtime = Some(built);
    Ok(built)
}

async fn run_dialer(
    secret_key: SecretKey,
    remote: EndpointId,
    result: Sender<io::Result<UnixStream>>,
) {
    if let Err(error) = dial_and_bridge(secret_key, remote, &result).await {
        let _ = result.send(Err(error));
    }
}

// A failed connect leaves the shared endpoint open for the other connections.
async fn dial_and_bridge(
    secret_key: SecretKey,
    remote: EndpointId,
    result: &Sender<io::Result<UnixStream>>,
) -> io::Result<()> {
    let endpoint = shared_endpoint(secret_key).await?;
    let connection = connect(&endpoint, remote).await?;
    let bridged = bridge_first_stream(&connection, result).await;
    connection.close(0_u8.into(), b"seer stream closed");
    bridged
}

async fn shared_endpoint(secret_key: SecretKey) -> io::Result<Endpoint> {
    let mut endpoints = ENDPOINTS.lock().await;
    endpoints.retain(|endpoint| !endpoint.is_closed());
    let id = secret_key.public();
    if let Some(endpoint) = endpoints.iter().find(|endpoint| endpoint.id() == id) {
        return Ok(endpoint.clone());
    }
    let endpoint = bind_endpoint(secret_key, Vec::new()).await?;
    endpoints.push(endpoint.clone());
    Ok(endpoint)
}

async fn bridge_first_stream(
    connection: &Connection,
    result: &Sender<io::Result<UnixStream>>,
) -> io::Result<()> {
    let (send, recv) = connection
        .open_bi()
        .await
        .map_err(|_| io::Error::other("could not open stream"))?;
    let (caller_stream, bridge_stream) = stream_pair()?;
    if result.send(Ok(caller_stream)).is_ok() {
        let _ = bridge(send, recv, bridge_stream).await;
    }
    Ok(())
}
