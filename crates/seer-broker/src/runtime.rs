use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use seer_core::proto::{ServerMsg, codec};
use seer_net::{Socket, Stream};

use crate::registry::random_hex;

const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

// The broker never starts a process. It asks a runtime that already runs on
// that person's own computer to open another stream.
pub(crate) struct Registration {
    pub(crate) generation: String,
    control: Mutex<Socket>,
    pending: Mutex<HashMap<String, SyncSender<Socket>>>,
}

impl Registration {
    fn request(&self, token: String) -> io::Result<()> {
        let mut control = lock(&self.control)?;
        control.set_write_timeout(Some(CONTROL_WRITE_TIMEOUT))?;
        codec::encode(&mut *control, &ServerMsg::OpenStream { token })
    }
}

#[derive(Default)]
pub(crate) struct RuntimeManager {
    published: Mutex<HashMap<String, Arc<Registration>>>,
}

impl RuntimeManager {
    // A second computer is refused. The first one keeps running; nothing here
    // stops it.
    pub(crate) fn publish(
        &self,
        user_id: &str,
        generation: &str,
        control: Socket,
    ) -> io::Result<Arc<Registration>> {
        let mut published = lock(&self.published)?;
        if let Some(current) = published.get(user_id) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "{user_id} already publishes a runtime (generation {}). \
                     Stop it on that computer first.",
                    current.generation
                ),
            ));
        }
        let registration = Arc::new(Registration {
            generation: generation.to_owned(),
            control: Mutex::new(control),
            pending: Mutex::new(HashMap::new()),
        });
        published.insert(user_id.to_owned(), Arc::clone(&registration));
        Ok(registration)
    }

    pub(crate) fn retire(&self, user_id: &str, registration: &Arc<Registration>) {
        let Ok(mut published) = self.published.lock() else {
            return;
        };
        if published
            .get(user_id)
            .is_some_and(|current| Arc::ptr_eq(current, registration))
        {
            published.remove(user_id);
        }
    }

    pub(crate) fn remove(&self, user_id: &str) -> io::Result<()> {
        if let Some(registration) = lock(&self.published)?.remove(user_id) {
            let _ = lock(&registration.control)?.shutdown(std::net::Shutdown::Both);
            lock(&registration.pending)?.clear();
        }
        Ok(())
    }

    pub(crate) fn is_running(&self, user_id: &str) -> bool {
        self.published
            .lock()
            .is_ok_and(|published| published.contains_key(user_id))
    }

    pub(crate) fn open(&self, user_id: &str) -> io::Result<Socket> {
        let registration = self.registration(user_id)?;
        let token = random_hex::<16>()?;
        let (sender, streams) = mpsc::sync_channel(1);
        lock(&registration.pending)?.insert(token.clone(), sender);
        let requested = registration.request(token.clone());
        let stream = requested.and_then(|()| {
            streams
                .recv_timeout(OPEN_TIMEOUT)
                .map_err(|_| io::Error::other("the runtime did not open a stream"))
        });
        lock(&registration.pending)?.remove(&token);
        stream
    }

    pub(crate) fn deliver(&self, user_id: &str, token: &str, stream: Socket) -> bool {
        let Ok(registration) = self.registration(user_id) else {
            return false;
        };
        let Ok(mut pending) = registration.pending.lock() else {
            return false;
        };
        let Some(sender) = pending.remove(token) else {
            return false;
        };
        sender.send(stream).is_ok()
    }

    fn registration(&self, user_id: &str) -> io::Result<Arc<Registration>> {
        lock(&self.published)?
            .get(user_id)
            .map(Arc::clone)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "that person has no runtime in this room",
                )
            })
    }
}

fn lock<T>(value: &Mutex<T>) -> io::Result<MutexGuard<'_, T>> {
    value
        .lock()
        .map_err(|_| io::Error::other("runtime registry lock is poisoned"))
}
