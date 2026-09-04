use std::io;
use std::sync::{Mutex, MutexGuard};

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
