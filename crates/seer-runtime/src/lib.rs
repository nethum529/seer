use std::env;
use std::io::{self, Read};
use std::path::Path;
use std::thread;

mod input;
pub mod pane_grid;
mod pane_host;
mod pty;
mod server;
mod user_session;

pub use pane_grid::{Cell, Color, PaneGrid};
pub use pane_host::PaneHost;
pub use pty::PtySession;
pub use server::{bind, serve};
pub use user_session::UserSession;

const USAGE: &str = "usage: seer-runtime <socket-path> <user> <shell>";

pub fn run() -> io::Result<()> {
    let (socket_path, user, shell) = arguments()?;
    start_lifeline_watch()?;
    let listener = bind(Path::new(&socket_path))?;
    serve(listener, UserSession::new(user, shell))
}

fn start_lifeline_watch() -> io::Result<()> {
    thread::Builder::new()
        .name("broker-lifeline".into())
        .spawn(watch_lifeline)?;
    Ok(())
}

fn watch_lifeline() {
    let mut lifeline = io::stdin();
    let mut byte = [0];
    loop {
        match lifeline.read(&mut byte) {
            Ok(1) => {}
            Ok(_) | Err(_) => std::process::exit(0),
        }
    }
}

fn arguments() -> io::Result<(String, String, String)> {
    let mut values = env::args().skip(1);
    let socket_path = values.next().ok_or_else(usage_error)?;
    let user = values.next().ok_or_else(usage_error)?;
    let shell = values.next().ok_or_else(usage_error)?;

    if values.next().is_some() {
        return Err(usage_error());
    }

    Ok((socket_path, user, shell))
}

fn usage_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, USAGE)
}
