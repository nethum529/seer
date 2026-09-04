use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seer_net::EndpointId;

const MAX_CONNECTIONS: usize = 32;
const MAX_CONNECTIONS_PER_SOURCE: usize = 4;
const SOURCE_HANDSHAKE_BURST: usize = 5;
const GLOBAL_HANDSHAKE_BURST: usize = MAX_CONNECTIONS;
const HANDSHAKE_INTERVAL: Duration = Duration::from_millis(500);
const MAX_TRACKED_SOURCES: usize = MAX_CONNECTIONS;
const SOURCE_BUCKET_RETENTION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) enum ConnectionKey {
    Direct(IpAddr),
    Loopback(SocketAddr),
    Relay(EndpointId),
}

pub(crate) struct ConnectionLimit(Arc<Mutex<AdmissionState>>);

impl ConnectionKey {
    pub(crate) fn direct(source: SocketAddr) -> Self {
        if source.ip().is_loopback() {
            Self::Loopback(source)
        } else {
            Self::Direct(source.ip())
        }
    }
}

impl Default for ConnectionLimit {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(AdmissionState::new(Instant::now()))))
    }
}

impl ConnectionLimit {
    pub(crate) fn try_acquire_for(&self, key: ConnectionKey) -> Option<ConnectionGuard> {
        let now = Instant::now();
        let mut state = self.0.lock().ok()?;
        state.prune_sources(now);
        if state.active >= MAX_CONNECTIONS {
            return None;
        }

        let source_state = state.sources.get(&key).copied();
        if source_state
            .as_ref()
            .is_some_and(|source| source.active >= MAX_CONNECTIONS_PER_SOURCE)
        {
            return None;
        }
        if source_state.is_none() {
            state.evict_inactive_source();
        }

        state.global_bucket.refill(now);
        let mut source_bucket = source_state.as_ref().map_or_else(
            || TokenBucket::new(now, SOURCE_HANDSHAKE_BURST),
            |source| source.bucket,
        );
        source_bucket.refill(now);
        if state.global_bucket.tokens == 0 || source_bucket.tokens == 0 {
            if let Some(source) = state.sources.get_mut(&key) {
                source.bucket = source_bucket;
                source.last_seen = now;
            }
            return None;
        }

        state.global_bucket.tokens -= 1;
        state.active += 1;
        let entry = state.sources.entry(key).or_insert(SourceState {
            active: 0,
            bucket: source_bucket,
            last_seen: now,
        });
        entry.active += 1;
        entry.bucket = source_bucket;
        entry.last_seen = now;
        Some(ConnectionGuard {
            state: Arc::clone(&self.0),
            key,
        })
    }
}

pub(crate) struct ConnectionGuard {
    state: Arc<Mutex<AdmissionState>>,
    key: ConnectionKey,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.active = state.active.saturating_sub(1);
        if let Some(source) = state.sources.get_mut(&self.key) {
            source.active = source.active.saturating_sub(1);
        }
    }
}

struct AdmissionState {
    active: usize,
    global_bucket: TokenBucket,
    sources: HashMap<ConnectionKey, SourceState>,
}

impl AdmissionState {
    fn new(now: Instant) -> Self {
        Self {
            active: 0,
            global_bucket: TokenBucket::new(now, GLOBAL_HANDSHAKE_BURST),
            sources: HashMap::new(),
        }
    }

    fn prune_sources(&mut self, now: Instant) {
        self.sources.retain(|_, source| {
            source.active > 0
                || now.saturating_duration_since(source.last_seen) <= SOURCE_BUCKET_RETENTION
        });
    }

    fn evict_inactive_source(&mut self) {
        if self.sources.len() < MAX_TRACKED_SOURCES {
            return;
        }
        let oldest = self
            .sources
            .iter()
            .filter(|(_, source)| source.active == 0)
            .min_by_key(|(_, source)| source.last_seen)
            .map(|(key, _)| *key);
        if let Some(key) = oldest {
            self.sources.remove(&key);
        }
    }
}

#[derive(Clone, Copy)]
struct SourceState {
    active: usize,
    bucket: TokenBucket,
    last_seen: Instant,
}

#[derive(Clone, Copy)]
struct TokenBucket {
    tokens: usize,
    capacity: usize,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(now: Instant, capacity: usize) -> Self {
        Self {
            tokens: capacity,
            capacity,
            last_refill: now,
        }
    }

    fn refill(&mut self, now: Instant) {
        let intervals = now.saturating_duration_since(self.last_refill).as_nanos()
            / HANDSHAKE_INTERVAL.as_nanos();
        if intervals == 0 {
            return;
        }

        let added = usize::try_from(intervals.min(self.capacity as u128)).unwrap_or(self.capacity);
        self.tokens = self.tokens.saturating_add(added).min(self.capacity);
        if self.tokens == self.capacity {
            self.last_refill = now;
            return;
        }

        let advance_nanos = HANDSHAKE_INTERVAL.as_nanos().saturating_mul(added as u128);
        let advance = Duration::from_nanos(u64::try_from(advance_nanos).unwrap_or(u64::MAX));
        self.last_refill = self.last_refill.checked_add(advance).unwrap_or(now);
    }
}
