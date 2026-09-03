use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

use seer_core::proto::{ServerMsg, codec};

const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn spawn(
    id: u64,
    mut stream: UnixStream,
    output: Receiver<Arc<[u8]>>,
) -> io::Result<()> {
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    thread::Builder::new()
        .name(format!("runtime-writer-{id}"))
        .spawn(move || write_output(id, &mut stream, &output))?;
    Ok(())
}

fn write_output(id: u64, stream: &mut UnixStream, output: &Receiver<Arc<[u8]>>) {
    while let Ok(bytes) = output.recv() {
        if let Err(error) = stream.write_all(&bytes) {
            eprintln!("runtime evicted slow connection {id}: {error}");
            break;
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

pub(super) fn encode(message: &ServerMsg) -> io::Result<Arc<[u8]>> {
    let mut output = Vec::new();
    codec::encode(&mut output, message)?;
    Ok(Arc::from(output))
}
