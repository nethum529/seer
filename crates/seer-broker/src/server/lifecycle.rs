use super::*;
use std::sync::atomic::Ordering;

pub(super) fn accept_loop(listener: TcpListener, broker: Arc<BrokerState>) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    while !broker.is_stopping() {
        match listener.accept() {
            Ok((connection, address)) => spawn_connection(
                Socket::from(connection),
                Arc::clone(&broker),
                ConnectionKey::direct(address),
                None,
            ),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

impl BrokerState {
    pub(crate) fn attach_client(
        &self,
        user_id: &str,
        writer: ClientWriter,
    ) -> io::Result<AttachmentGuard<'_>> {
        self.require_member(user_id)?;
        self.attachments.attach(user_id, writer)
    }

    pub(crate) fn clients(&self, user_id: &str, excluded: &str) -> io::Result<Vec<ClientInfo>> {
        self.attachments.clients(user_id, excluded)
    }

    pub(crate) fn detach_client(&self, user_id: &str, client_id: &str) -> io::Result<bool> {
        self.attachments.detach_client(user_id, client_id)
    }

    pub(crate) fn membership(&self) -> io::Result<std::sync::MutexGuard<'_, ()>> {
        self.membership
            .lock()
            .map_err(|_| io::Error::other("membership lock is poisoned"))
    }

    pub(crate) fn require_member(&self, user_id: &str) -> io::Result<()> {
        if self.registry.person(user_id)?.is_some() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "You are no longer in this room.",
            ))
        }
    }

    pub(crate) fn leave(&self, user_id: &str, caller: &str) -> io::Result<()> {
        let membership = self.membership()?;
        let result = self.depart(user_id, caller, false);
        drop(membership);
        result.and(self.publish_grants()).and(self.publish_people())
    }

    pub(crate) fn remove_person(&self, host: &str, name: &str) -> io::Result<()> {
        let membership = self.membership()?;
        self.require_host(host, "remove a person")?;
        let person = self
            .registry
            .people()?
            .into_iter()
            .find(|person| person.name == name)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("No person named {name}."))
            })?;
        if person.is_owner {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "You cannot remove the host.",
            ));
        }
        let result = self.depart(&person.user_id, "", true);
        drop(membership);
        result.and(self.publish_grants()).and(self.publish_people())
    }

    fn depart(&self, user_id: &str, caller: &str, removed_by_host: bool) -> io::Result<()> {
        self.registry.remove_person(user_id, removed_by_host)?;
        let grants = self.grants.remove_person(user_id);
        let runtime = self.runtimes.remove(user_id);
        let attachments = self.attachments.clients(user_id, caller).map(|clients| {
            for client in clients {
                let _ = self.attachments.detach_client(user_id, &client.client_id);
            }
        });
        grants.and(runtime).and(attachments)
    }

    pub(crate) fn require_host(&self, user_id: &str, action: &str) -> io::Result<()> {
        if self
            .registry
            .person(user_id)?
            .is_some_and(|person| person.is_owner)
        {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Only the host can {action}. Use seer leave to leave the room."),
            ))
        }
    }

    pub(crate) fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    pub(super) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }
}
