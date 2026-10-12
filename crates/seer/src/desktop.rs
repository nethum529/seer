use std::io;
use std::process::{Command, Stdio};

use crate::ServerEntry;

pub(crate) fn peek(server: &ServerEntry, person: &str) -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let sibling = executable
        .parent()
        .map(|parent| parent.join("seer-desktop"));
    let binary = sibling
        .filter(|path| path.is_file())
        .unwrap_or_else(|| "seer-desktop".into());
    Command::new(binary)
        .args(["--user", &server.user_id, "--peek", person])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}
