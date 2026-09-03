use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_CONNECTIONS: usize = 64;

#[derive(Default)]
pub(crate) struct ConnectionLimit(Arc<AtomicUsize>);

impl ConnectionLimit {
    pub(crate) fn try_acquire(&self) -> Option<ConnectionGuard> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_CONNECTIONS).then_some(active + 1)
            })
            .ok()?;
        Some(ConnectionGuard(Arc::clone(&self.0)))
    }
}

pub(crate) struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
