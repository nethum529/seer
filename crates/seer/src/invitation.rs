use std::io;

use crate::{ServerEntry, commands};

const INSTALL_URL: &str =
    "https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh";

pub struct IssuedInvite {
    pub(crate) capsule: String,
    pub expires_in_secs: u64,
}

impl IssuedInvite {
    pub fn join_line(&self) -> String {
        commands::join_line(&self.capsule)
    }

    pub fn install_line(&self) -> String {
        format!("curl -fsSL {INSTALL_URL} | sh -s -- {}", self.capsule)
    }
}

pub fn create_invite(server: &ServerEntry, hours: Option<u32>) -> io::Result<IssuedInvite> {
    commands::invitation::create(server, hours).map_err(|error| io::Error::other(error.message))
}
