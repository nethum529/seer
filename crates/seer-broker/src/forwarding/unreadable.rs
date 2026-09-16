use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use seer_core::proto::ServerMsg;

use super::{Coordinator, RuntimeConnection};
use crate::runtime::{Registration, RuntimeManager};

// Issue 425: a runtime that sends a message this room cannot decode sends it
// again on every new stream, so wait for a new publish or this long.
const RETRY: Duration = Duration::from_secs(30);

pub(super) const NOTICE: &str = "This terminal cannot be shown: the room cannot read its runtime. It may run another Seer version.";

#[derive(Default)]
pub(super) struct Unreadable {
    runtimes: HashMap<String, (Option<Weak<Registration>>, Instant)>,
}

impl Unreadable {
    pub(super) fn record(&mut self, manager: &RuntimeManager, user: &str) {
        let publish = manager.registration(user).ok().map(|r| Arc::downgrade(&r));
        self.runtimes
            .insert(user.to_owned(), (publish, Instant::now()));
    }

    pub(super) fn blocked(&mut self, manager: &RuntimeManager, user: &str) -> bool {
        let Some((publish, since)) = self.runtimes.get(user) else {
            return false;
        };
        let same_publish = match manager.registration(user) {
            Ok(current) => publish
                .as_ref()
                .is_some_and(|publish| Weak::ptr_eq(publish, &Arc::downgrade(&current))),
            Err(_) => true,
        };
        if same_publish && since.elapsed() < RETRY {
            return true;
        }
        self.runtimes.remove(user);
        false
    }

    pub(super) fn users(&self) -> Vec<String> {
        self.runtimes.keys().cloned().collect()
    }
}

impl Coordinator<'_> {
    pub(super) fn connect_runtime(&mut self, user: &str) -> io::Result<RuntimeConnection> {
        if self.unreadable.blocked(self.broker.runtimes(), user) {
            return Err(io::Error::other(NOTICE));
        }
        let person = self.person(user)?;
        RuntimeConnection::connect(self.broker, &person, &self.event_sender).map_err(|error| {
            if error.kind() != io::ErrorKind::InvalidData {
                return error;
            }
            self.unreadable.record(self.broker.runtimes(), user);
            io::Error::other(NOTICE)
        })
    }

    pub(super) fn runtime_lost(&mut self, user: &str, error: &io::Error) -> io::Result<()> {
        seer_core::debug_log!(
            "runtime lost user={user} client={} error={error:?}",
            self.client_id
        );
        self.close_runtime(user)?;
        if error.kind() != io::ErrorKind::InvalidData {
            return Ok(());
        }
        self.unreadable.record(self.broker.runtimes(), user);
        self.notify_unreadable(user)
    }

    pub(super) fn refresh_unreadable(&mut self) -> io::Result<()> {
        for user in self.unreadable.users() {
            if !self.watched(&user) {
                continue;
            }
            if !self.runtimes.contains_key(&user) && !self.lists.contains(&user) {
                let _ = self.runtime(&user);
            }
            if self.unreadable.blocked(self.broker.runtimes(), &user) {
                self.notify_unreadable(&user)?;
            }
        }
        Ok(())
    }

    fn notify_unreadable(&self, user: &str) -> io::Result<()> {
        if !self.watched(user) {
            return Ok(());
        }
        self.write(&ServerMsg::Refused {
            reason: NOTICE.into(),
        })
    }

    fn watched(&self, user: &str) -> bool {
        self.watches.keys().any(|(owner, _)| owner == user)
    }
}
