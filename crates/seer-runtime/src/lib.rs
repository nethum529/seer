use std::env;
use std::ffi::{CString, c_char, c_int};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, Ordering};

mod input;
pub mod pane_grid;
mod pane_host;
mod persistence;
mod pty;
mod room;
mod server;
mod snapshot;
mod user_session;

pub use pane_grid::{Cell, Color, PaneGrid};
pub use pane_host::PaneHost;
pub use pty::PtySession;
pub use server::{bind, serve};
pub use user_session::UserSession;

const USAGE: &str = "usage: seer-runtime <socket-path> <user> <shell> <generation>";
const SNAPSHOT_DIRECTORY_VAR: &str = "SEER_SNAPSHOT_DIR";

pub fn run() -> io::Result<()> {
    let (socket_path, user, shell, generation) = arguments()?;
    let snapshot_dir = snapshot_directory();
    let session = persistence::load_session(user, shell, snapshot_dir.as_deref())?;
    let listener = bind(Path::new(&socket_path))?;
    install_sigterm_cleanup(&socket_path);
    server::serve_with_generation(listener, session, generation, room::from_environment())
}

fn snapshot_directory() -> Option<PathBuf> {
    env::var_os(SNAPSHOT_DIRECTORY_VAR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn arguments() -> io::Result<(String, String, String, String)> {
    let mut values = env::args().skip(1);
    let socket_path = values.next().ok_or_else(usage_error)?;
    let user = values.next().ok_or_else(usage_error)?;
    let shell = values.next().ok_or_else(usage_error)?;
    let generation = values.next().ok_or_else(usage_error)?;

    if values.next().is_some() {
        return Err(usage_error());
    }

    Ok((socket_path, user, shell, generation))
}

fn usage_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, USAGE)
}

static SOCKET_PATH: AtomicPtr<c_char> = AtomicPtr::new(std::ptr::null_mut());

fn install_sigterm_cleanup(socket_path: &str) {
    let Ok(path) = CString::new(socket_path) else {
        return;
    };
    SOCKET_PATH.store(path.into_raw(), Ordering::SeqCst);
    // SAFETY: the handler is a valid extern "C" function and the path is stored first.
    unsafe {
        libc::signal(
            libc::SIGTERM,
            remove_socket_and_exit as *const () as libc::sighandler_t,
        );
    }
}

extern "C" fn remove_socket_and_exit(_signal: c_int) {
    let path = SOCKET_PATH.load(Ordering::SeqCst);
    if !path.is_null() {
        // SAFETY: unlink is async signal safe and the pointer is a leaked C string.
        unsafe {
            libc::unlink(path);
        }
    }
    // SAFETY: _exit is async signal safe and ends the process without cleanup.
    unsafe {
        libc::_exit(0);
    }
}
