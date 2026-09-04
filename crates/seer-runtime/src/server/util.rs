use std::fs;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

pub(super) fn remove_stale_socket(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    match UnixStream::connect(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "runtime socket is already in use",
        )),
        Err(_) => fs::remove_file(path),
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> io::Result<MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| lock_poisoned())
}

fn lock_poisoned() -> io::Error {
    io::Error::other("runtime shared state lock is poisoned")
}

pub(super) fn connection_closed() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "runtime connection is closed")
}

pub(super) fn stop_after_snapshot_failure(error: &io::Error) -> ! {
    eprintln!("runtime stops after a snapshot save failure: {error}");
    std::process::exit(1)
}
