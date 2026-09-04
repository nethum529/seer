use std::io;
use std::os::unix::net::UnixStream;
use std::time::Instant;

use seer_core::proto::{ClientMsg, Person, PersonState, ServerMsg, codec};

use super::{SharedSession, lock};

pub(super) fn handle_status_query(
    stream: &mut UnixStream,
    shared: &SharedSession,
) -> io::Result<()> {
    let status = shared.status()?;
    codec::encode(stream, &status)
}

impl SharedSession {
    pub(super) fn record_input(&self, message: &ClientMsg) {
        if matches!(message, ClientMsg::TerminalInput { .. })
            && let Ok(mut last_input) = self.last_input.lock()
        {
            *last_input = Instant::now();
        }
    }

    fn status(&self) -> io::Result<ServerMsg> {
        let active = self
            .targets()?
            .into_iter()
            .find(|target| target.active)
            .map(|target| (target.workspace, target.tab));
        let idle_secs = lock(&self.last_input)?.elapsed().as_secs();
        let session = lock(&self.session)?;
        let foreground = active.map_or_else(String::new, |(workspace, tab)| {
            session.foreground(&workspace, &tab)
        });
        Ok(ServerMsg::People {
            people: vec![Person {
                user_id: session.user.clone(),
                name: String::new(),
                attached_clients: 0,
                peekable: true,
                state: PersonState::Away,
                tabs: session.tab_count(),
                foreground,
                idle_secs,
            }],
        })
    }
}
