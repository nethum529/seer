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
    pub(crate) fn leave(&self, user_id: &str, caller: &str) -> io::Result<()> {
        self.registry.remove_person(user_id)?;
        self.runtimes.remove(user_id)?;
        for client in self.attachments.clients(user_id, caller)? {
            let _ = self.attachments.detach_client(user_id, &client.client_id);
        }
        self.publish_people()
    }

    pub(crate) fn require_host(&self, user_id: &str) -> io::Result<()> {
        if self
            .registry
            .person(user_id)?
            .is_some_and(|person| person.is_owner)
        {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Only the host can stop the server. Use seer leave to leave the room.",
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
