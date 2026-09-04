use std::collections::HashMap;
use std::io;
use std::net::Shutdown;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use seer_core::proto::{ClientInfo, ServerMsg, codec};
use seer_net::Stream;

use crate::registry::random_hex;

const MAX_ATTACHMENTS_PER_USER: usize = 8;

pub(crate) type ClientWriter = Arc<Mutex<Box<dyn Stream>>>;

#[derive(Default)]
pub(crate) struct Attachments(Mutex<HashMap<String, HashMap<String, Attachment>>>);

impl Attachments {
    pub(crate) fn attach(
        &self,
        user_id: &str,
        writer: ClientWriter,
    ) -> io::Result<AttachmentGuard<'_>> {
        let mut people = self.lock()?;
        let clients = people.entry(user_id.to_owned()).or_default();
        if clients.len() >= MAX_ATTACHMENTS_PER_USER {
            return Err(io::Error::other("per-user attachment limit reached"));
        }
        let client_id = random_hex::<16>()?;
        clients.insert(
            client_id.clone(),
            Attachment {
                connected_at: Instant::now(),
                writer,
            },
        );
        Ok(AttachmentGuard {
            attachments: self,
            user_id: user_id.to_owned(),
            client_id,
        })
    }

    pub(crate) fn count(&self, user_id: &str) -> u32 {
        let Ok(people) = self.0.lock() else {
            return 0;
        };
        people
            .get(user_id)
            .map(HashMap::len)
            .and_then(|count| u32::try_from(count).ok())
            .unwrap_or(0)
    }

    pub(crate) fn clients(
        &self,
        user_id: &str,
        excluded_client_id: &str,
    ) -> io::Result<Vec<ClientInfo>> {
        let people = self.lock()?;
        let mut clients: Vec<_> = people
            .get(user_id)
            .into_iter()
            .flat_map(HashMap::iter)
            .filter(|(client_id, _)| client_id.as_str() != excluded_client_id)
            .map(|(client_id, attachment)| {
                (
                    attachment.connected_at,
                    ClientInfo {
                        client_id: client_id.clone(),
                        connected_secs: attachment.connected_at.elapsed().as_secs(),
                    },
                )
            })
            .collect();
        clients.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.client_id.cmp(&right.1.client_id))
        });
        Ok(clients.into_iter().map(|(_, client)| client).collect())
    }

    pub(crate) fn detach_client(&self, user_id: &str, client_id: &str) -> io::Result<bool> {
        let writer = {
            let mut people = self.lock()?;
            let Some(clients) = people.get_mut(user_id) else {
                return Ok(false);
            };
            let Some(attachment) = clients.remove(client_id) else {
                return Ok(false);
            };
            if clients.is_empty() {
                people.remove(user_id);
            }
            attachment.writer
        };
        let mut stream = lock_writer(&writer)?;
        let send_result = codec::encode(
            &mut *stream,
            &ServerMsg::Bye {
                reason: "detached".into(),
            },
        );
        let shutdown_result = stream.shutdown(Shutdown::Both);
        send_result.and(shutdown_result)?;
        Ok(true)
    }

    fn remove(&self, user_id: &str, client_id: &str) {
        if let Ok(mut people) = self.0.lock()
            && let Some(clients) = people.get_mut(user_id)
        {
            clients.remove(client_id);
            if clients.is_empty() {
                people.remove(user_id);
            }
        }
    }

    fn lock(&self) -> io::Result<MutexGuard<'_, HashMap<String, HashMap<String, Attachment>>>> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("attachment lock is poisoned"))
    }
}

pub(crate) struct AttachmentGuard<'a> {
    attachments: &'a Attachments,
    user_id: String,
    client_id: String,
}

impl AttachmentGuard<'_> {
    pub(crate) fn client_id(&self) -> &str {
        &self.client_id
    }
}

impl Drop for AttachmentGuard<'_> {
    fn drop(&mut self) {
        self.attachments.remove(&self.user_id, &self.client_id);
    }
}

struct Attachment {
    connected_at: Instant,
    writer: ClientWriter,
}

pub(crate) fn lock_writer(writer: &ClientWriter) -> io::Result<MutexGuard<'_, Box<dyn Stream>>> {
    writer
        .lock()
        .map_err(|_| io::Error::other("client writer lock is poisoned"))
}
