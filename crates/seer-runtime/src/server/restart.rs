use std::io;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ServerMsg, codec};

use super::{SharedSession, lock};

// seer restart signals the runtime after this reply. The window must read
// its Bye first, so the reply waits until the windows close or time runs out.
const WINDOWS_CLOSE_WAIT: Duration = Duration::from_secs(2);

pub(super) fn handle_restart(stream: &mut UnixStream, shared: &SharedSession) -> io::Result<()> {
    let bye = ServerMsg::Bye {
        reason: "restarted".into(),
    };
    let windows: Vec<u64> = lock(&shared.connections)?
        .iter()
        .filter(|connection| !connection.read_only)
        .map(|connection| connection.id)
        .collect();
    for id in &windows {
        let _ = shared.send_to(*id, &bye);
    }
    let deadline = Instant::now() + WINDOWS_CLOSE_WAIT;
    while Instant::now() < deadline
        && lock(&shared.connections)?
            .iter()
            .any(|connection| windows.contains(&connection.id))
    {
        thread::sleep(Duration::from_millis(10));
    }
    codec::encode(stream, &bye)
}
