use std::env;
use std::io;
use std::path::Path;

use seer_runtime::UserSession;

const USAGE: &str = "usage: seer-runtime <socket-path> <user> <shell>";

fn main() -> io::Result<()> {
    let (socket_path, user, shell) = arguments()?;
    let listener = seer_runtime::bind(Path::new(&socket_path))?;
    seer_runtime::serve(listener, UserSession::new(user, shell))
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
