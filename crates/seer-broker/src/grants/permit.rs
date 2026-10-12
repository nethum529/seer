use std::io;
use std::net::Shutdown;
use std::sync::{Arc, Mutex};

use seer_net::{Socket, Stream};

#[derive(Default)]
pub(crate) struct Permit {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    revoked: bool,
    active: Vec<(Arc<()>, Socket)>,
}

impl Permit {
    pub(crate) fn start(self: &Arc<Self>, stream: &Socket) -> io::Result<Option<ActivePermit>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("grant lock is poisoned"))?;
        if state.revoked {
            return Ok(None);
        }
        let identity = Arc::new(());
        state.active.push((Arc::clone(&identity), stream.clone()));
        Ok(Some(ActivePermit {
            permit: Arc::clone(self),
            identity,
        }))
    }

    pub(super) fn revoke(&self) -> io::Result<Vec<Socket>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("grant lock is poisoned"))?;
        state.revoked = true;
        Ok(state.active.drain(..).map(|(_, stream)| stream).collect())
    }
}

pub(crate) struct ActivePermit {
    permit: Arc<Permit>,
    identity: Arc<()>,
}

impl Drop for ActivePermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.permit.state.lock() {
            state
                .active
                .retain(|(id, _)| !Arc::ptr_eq(id, &self.identity));
        }
    }
}

pub(super) fn cancel(streams: Vec<Socket>) {
    for stream in streams {
        let _ = stream.shutdown(Shutdown::Both);
    }
}
