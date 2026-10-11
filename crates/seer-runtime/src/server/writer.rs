use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use seer_core::proto::{ServerMsg, codec};

use crate::outbox::{Outbox, Outgoing};

const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn spawn(id: u64, mut stream: UnixStream, output: Arc<Outbox>) -> io::Result<()> {
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    thread::Builder::new()
        .name(format!("runtime-writer-{id}"))
        .spawn(move || write_output(id, &mut stream, &output))?;
    Ok(())
}

fn write_output(id: u64, stream: &mut UnixStream, output: &Outbox) {
    while let Some(item) = output.pop() {
        let written = match item {
            Outgoing::Message(bytes) => stream.write_all(&bytes),
            Outgoing::Raw(event) => {
                codec::encode_raw(&event).and_then(|frame| stream.write_all(&frame))
            }
        };
        if let Err(error) = written {
            eprintln!("runtime evicted slow connection {id}: {error}");
            break;
        }
    }
    output.close();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

pub(super) fn encode(message: &ServerMsg) -> io::Result<Arc<[u8]>> {
    let mut output = Vec::new();
    codec::encode(&mut output, message)?;
    Ok(Arc::from(output))
}
