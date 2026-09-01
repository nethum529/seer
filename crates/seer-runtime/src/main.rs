use std::env;
use std::fs::File;
use std::io;
use std::io::Read;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::path::Path;
use std::thread;

use seer_runtime::UserSession;

const USAGE: &str = "usage: seer-runtime <socket-path> <user> <shell>";
const LIFELINE_FD_ENV: &str = "SEER_BROKER_LIFELINE_FD";

fn main() -> io::Result<()> {
    let (socket_path, user, shell) = arguments()?;
    start_lifeline_watch()?;
    let listener = seer_runtime::bind(Path::new(&socket_path))?;
    seer_runtime::serve(listener, UserSession::new(user, shell))
}

fn start_lifeline_watch() -> io::Result<()> {
    let Some(fd) = lifeline_fd()? else {
        return Ok(());
    };
    // SAFETY: The broker passes ownership of this inherited descriptor to the runtime.
    let lifeline = unsafe { File::from_raw_fd(fd) };
    set_close_on_exec(lifeline.as_raw_fd())?;
    thread::Builder::new()
        .name("broker-lifeline".into())
        .spawn(move || watch_lifeline(lifeline))?;
    Ok(())
}

fn lifeline_fd() -> io::Result<Option<RawFd>> {
    let Some(value) = env::var_os(LIFELINE_FD_ENV) else {
        return Ok(None);
    };
    let value = value
        .into_string()
        .map_err(|_| invalid_lifeline("lifeline file descriptor is not UTF-8"))?;
    value
        .parse()
        .map(Some)
        .map_err(|_| invalid_lifeline("lifeline file descriptor is invalid"))
}

fn set_close_on_exec(fd: RawFd) -> io::Result<()> {
    // SAFETY: fd is open for the duration of this call, and fcntl does not retain it.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is open for the duration of this call, and fcntl does not retain it.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn watch_lifeline(mut lifeline: File) {
    let mut byte = [0];
    loop {
        match lifeline.read(&mut byte) {
            Ok(1) => {}
            Ok(_) | Err(_) => std::process::exit(0),
        }
    }
}

fn invalid_lifeline(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
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
