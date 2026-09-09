use std::io;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use super::connection::handle_connection;
use super::util::stop_after_snapshot_failure;
use super::{SharedSession, lock};

pub(super) fn start_room(
    room: crate::room::RoomConfig,
    generation: &str,
    shared: &Arc<SharedSession>,
    ids: &Arc<AtomicU64>,
) -> io::Result<()> {
    let user = lock(&shared.session)?.user.clone();
    let shared = Arc::clone(shared);
    let ids = Arc::clone(ids);
    let generation = generation.to_owned();
    crate::room::spawn(room, user, generation.clone(), move |stream| {
        if let Err(error) = spawn_connection(stream, &shared, &ids, &generation, true) {
            eprintln!("runtime could not serve a room stream: {error}");
        }
    })
}

pub(super) fn spawn_connection(
    stream: UnixStream,
    shared: &Arc<SharedSession>,
    ids: &Arc<AtomicU64>,
    generation: &str,
    remote: bool,
) -> io::Result<()> {
    let connection_id = ids.fetch_add(1, Ordering::Relaxed);
    let connection = Arc::clone(shared);
    let generation = generation.to_owned();
    thread::Builder::new()
        .name(format!("runtime-connection-{connection_id}"))
        .spawn(move || {
            if let Err(error) =
                handle_connection(stream, &connection, connection_id, generation, remote)
            {
                if crate::persistence::is_fatal(&error) {
                    stop_after_snapshot_failure(&error);
                }
                eprintln!("runtime connection error: {error}");
            }
        })?;
    Ok(())
}
