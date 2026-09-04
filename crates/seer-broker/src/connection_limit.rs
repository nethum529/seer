use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use seer_net::EndpointId;

const MAX_CONNECTIONS: usize = 32;
const MAX_CONNECTIONS_PER_SOURCE: usize = 4;
const HANDSHAKE_BURST: usize = 5;
const HANDSHAKE_INTERVAL: Duration = Duration::from_millis(500);
const MAX_SOURCE_BUCKETS: usize = MAX_CONNECTIONS;
const SOURCE_BUCKET_RETENTION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ConnectionKey {
    Direct(IpAddr),
    Relay(EndpointId),
}

#[derive(Clone)]
pub(crate) struct ConnectionLimit(Arc<Mutex<AdmissionState>>);

impl Default for ConnectionLimit {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl ConnectionLimit {
    fn new(now: Instant) -> Self {
        Self(Arc::new(Mutex::new(AdmissionState::new(now))))
    }

    pub(crate) fn try_acquire_for(&self, key: ConnectionKey) -> Option<ConnectionGuard> {
        self.try_acquire_with_time(Some(key), Instant::now())
    }

    #[cfg(test)]
    fn try_acquire_at(
        &self,
        key: ConnectionKey,
        now: Instant,
    ) -> Option<ConnectionGuard> {
        self.try_acquire_with_time(Some(key), now)
    }

    fn try_acquire_with_time(
        &self,
        key: Option<ConnectionKey>,
        now: Instant,
    ) -> Option<ConnectionGuard> {
        let mut state = self.0.lock().ok()?;
        state.prune_sources(now);
        if state.active >= MAX_CONNECTIONS {
            return None;
        }

        let source_state = key.and_then(|source| state.sources.get(&source).cloned());
        if source_state
            .as_ref()
            .is_some_and(|source| source.active >= MAX_CONNECTIONS_PER_SOURCE)
        {
            return None;
        }
        if key.is_some()
            && source_state.is_none()
            && state.sources.len() >= MAX_SOURCE_BUCKETS
        {
            return None;
        }

        state.global_bucket.refill(now);
        let mut source_bucket = source_state
            .as_ref()
            .map_or_else(|| TokenBucket::new(now), |source| source.bucket);
        source_bucket.refill(now);
        if state.global_bucket.tokens == 0 || source_bucket.tokens == 0 {
            if let Some(source) = key.and_then(|source| state.sources.get_mut(&source)) {
                source.bucket = source_bucket;
                source.last_seen = now;
            }
            return None;
        }

        state.global_bucket.tokens -= 1;
        state.active += 1;
        if let Some(key) = key {
            let entry = state.sources.entry(key).or_insert(SourceState {
                active: 0,
                bucket: source_bucket,
                last_seen: now,
            });
            entry.active += 1;
            entry.bucket = source_bucket;
            entry.last_seen = now;
        }
        Some(ConnectionGuard {
            state: Arc::clone(&self.0),
            key,
        })
    }
}

pub(crate) struct ConnectionGuard {
    state: Arc<Mutex<AdmissionState>>,
    key: Option<ConnectionKey>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.active = state.active.saturating_sub(1);
        if let Some(key) = self.key
            && let Some(source) = state.sources.get_mut(&key)
        {
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
            global_bucket: TokenBucket::new(now),
            sources: HashMap::new(),
        }
    }

    fn prune_sources(&mut self, now: Instant) {
        self.sources.retain(|_, source| {
            source.active > 0
                || now.saturating_duration_since(source.last_seen) <= SOURCE_BUCKET_RETENTION
        });
    }
}

#[derive(Clone)]
struct SourceState {
    active: usize,
    bucket: TokenBucket,
    last_seen: Instant,
}

#[derive(Clone, Copy)]
struct TokenBucket {
    tokens: usize,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(now: Instant) -> Self {
        Self {
            tokens: HANDSHAKE_BURST,
            last_refill: now,
        }
    }

    fn refill(&mut self, now: Instant) {
        let intervals = now
            .saturating_duration_since(self.last_refill)
            .as_nanos()
            / HANDSHAKE_INTERVAL.as_nanos();
        if intervals == 0 {
            return;
        }

        let added = usize::try_from(intervals.min(HANDSHAKE_BURST as u128))
            .unwrap_or(HANDSHAKE_BURST);
        self.tokens = self.tokens.saturating_add(added).min(HANDSHAKE_BURST);
        if self.tokens == HANDSHAKE_BURST {
            self.last_refill = now;
            return;
        }

        let advance_nanos = HANDSHAKE_INTERVAL
            .as_nanos()
            .saturating_mul(added as u128);
        let advance = Duration::from_nanos(u64::try_from(advance_nanos).unwrap_or(u64::MAX));
        self.last_refill = self.last_refill.checked_add(advance).unwrap_or(now);
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::{Duration, Instant};

    use seer_net::SecretKey;

    use super::{
        ConnectionKey, ConnectionLimit, HANDSHAKE_BURST, MAX_CONNECTIONS,
        MAX_CONNECTIONS_PER_SOURCE,
    };

    fn direct(source: u8) -> ConnectionKey {
        ConnectionKey::Direct(IpAddr::V4(Ipv4Addr::new(192, 0, 2, source)))
    }

    fn relay(source: u8) -> ConnectionKey {
        ConnectionKey::Relay(SecretKey::from_bytes(&[source; 32]).public())
    }

    #[test]
    fn limits_global_and_direct_connections() {
        let now = Instant::now();
        let limit = ConnectionLimit::new(now);
        let mut guards = (0..MAX_CONNECTIONS)
            .map(|source| {
                limit
                    .try_acquire_at(
                        direct(source as u8 + 1),
                        now + Duration::from_millis(source as u64 * 500),
                    )
                    .expect("global capacity must allow the connection")
            })
            .collect::<Vec<_>>();
        assert!(
            limit
                .try_acquire_at(direct(255), now + Duration::from_secs(16))
                .is_none()
        );
        let guard = guards.pop().expect("a connection guard must exist");
        drop(guard);
        assert!(
            limit
                .try_acquire_at(direct(1), now + Duration::from_secs(16))
                .is_some()
        );
    }

    #[test]
    fn separates_direct_and_relay_source_buckets() {
        let now = Instant::now();
        let limit = ConnectionLimit::new(now);
        let direct_guards = (0..MAX_CONNECTIONS_PER_SOURCE)
            .map(|index| {
                limit
                    .try_acquire_at(
                        direct(1),
                        now + Duration::from_millis(index as u64 * 500),
                    )
                    .expect("direct source capacity must allow the connection")
            })
            .collect::<Vec<_>>();
        assert!(
            limit
                .try_acquire_at(direct(1), now + Duration::from_secs(2))
                .is_none()
        );
        let relay_guards = (0..MAX_CONNECTIONS_PER_SOURCE)
            .map(|index| {
                limit
                    .try_acquire_at(
                        relay(1),
                        now + Duration::from_millis((index + 4) as u64 * 500),
                    )
                    .expect("relay source capacity must allow the connection")
            })
            .collect::<Vec<_>>();
        assert!(
            limit
                .try_acquire_at(relay(1), now + Duration::from_secs(4))
                .is_none()
        );
        drop((direct_guards, relay_guards));
    }

    #[test]
    fn applies_source_and_global_handshake_buckets() {
        let now = Instant::now();
        let limit = ConnectionLimit::new(now);
        let source = direct(1);
        for _ in 0..HANDSHAKE_BURST {
            let guard = limit
                .try_acquire_at(source, now)
                .expect("burst must allow five handshakes");
            drop(guard);
        }
        assert!(limit.try_acquire_at(source, now).is_none());
        let guard = limit
            .try_acquire_at(source, now + Duration::from_millis(500))
            .expect("source bucket must refill one token");
        drop(guard);
        assert!(
            limit
                .try_acquire_at(relay(2), now + Duration::from_millis(500))
                .is_none()
        );
    }
}
