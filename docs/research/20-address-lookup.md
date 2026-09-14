# Address lookup for room connections

Research date: 2026-09-13. Issue 391 (R-003).

## Scope

This note records how a Seer endpoint finds the room peer today, and the
requirements for any later change that adds address hints or a cache. It covers
cold and warm lookup, address lifetime, privacy, invite compatibility, and
recovery from a stale address. It does not change the invite format or the
lookup setup.

Source revision: Seer main at 8714a51. iroh 1.1.0, iroh-dns 1.1.0, and
iroh-relay 1.1.0 from Cargo.lock. The iroh paths below are relative to the
crate source in ~/.cargo/registry/src/*/iroh-1.1.0 unless the note names
another crate. The same source is on docs.rs, for example
https://docs.rs/iroh/1.1.0/src/iroh/endpoint/presets.rs.html.

## Short answer

- The SEER2 invite carries the broker public key and the seat token. It
  carries no relay URL and no direct address.
- Every Seer dial binds a new iroh endpoint. iroh keeps address state only in
  memory, inside one endpoint. Thus every join, every client command, and
  every runtime reconnect is a cold lookup at the iroh level. The warm case
  does not occur in Seer today.
- A cold lookup queries two services at the same time: a Pkarr GET over HTTPS
  to dns.iroh.link, and a DNS TXT query through the system resolver. The first
  result that gives a path lets the connect continue.
- The broker publishes only its home relay URL, not its IP addresses. A
  relay URL in the invite adds no new exposure, because the key in the invite
  already lets anyone read that URL. A direct address in the invite is new
  exposure.
- A hint added to the end of a SEER2 invite breaks old clients in a bad way:
  they read the hint as part of the seat token and the broker refuses the
  join. A hint format must use a new prefix.
- iroh already treats an address from the application as a candidate. It
  still runs lookup in parallel and uses any path that works. A stale hint
  costs a failed path attempt, not a failed connection.

Measurements are in the section "Measurements". They wait for the baseline
harness from issue 389.

## How a fresh endpoint finds the peer today

### What the invite carries

The broker makes the invite as `SEER2-<endpoint id>-<seat token>`
(crates/seer-broker/src/server.rs, fn invite). The endpoint id is the 64
character hex form of the broker iroh public key. The seat token is 64 hex
characters from 32 random bytes (crates/seer-broker/src/registry.rs,
random_hex). A seat lives one hour by default and at most seven days
(SEAT_LIFETIME_SECS and MAX_SEAT_LIFETIME_SECS in registry.rs).

The client parses the invite in crates/seer/src/capsule.rs. It keeps the key
and saves the server as `iroh:<key>` in the server store
(crates/seer/src/store.rs, ServerEntry.endpoint). Later commands and the room
runtime dial that key again. No address is stored.

### Where Seer dials

All remote paths go through seer-net (crates/seer-net/src/lib.rs):

| Caller | Function | Endpoint lifetime |
| --- | --- | --- |
| seer join and every client command (commands.rs, connect_iroh) | seer_net::dial | One new endpoint per call. Closed when the stream ends. |
| Room runtime to broker (seer-runtime/src/room.rs, Link::iroh) | seer_net::dial_session | One new endpoint per connection. A reconnect after backoff binds a new one. |
| Broker (seer-broker/src/server.rs) | Listener::bind | One endpoint for the broker lifetime. It only accepts. |

Each of these calls bind_endpoint, which is
`Endpoint::builder(presets::N0).secret_key(..).alpns(..).bind()`. The listener
then waits for `endpoint.online()` for at most 4 seconds (ONLINE_TIMEOUT). The
dialers do not wait for online. They call `endpoint.connect(remote, ALPN)`
with only the endpoint id.

### Lookup services in the N0 preset

src/endpoint/presets.rs, `impl Preset for N0` (line 116), adds:

1. PkarrPublisher::n0_dns(). It publishes to https://dns.iroh.link/pkarr
   (N0_DNS_PKARR_RELAY_PROD, src/address_lookup/pkarr.rs line 127).
2. PkarrResolver::n0_dns(). It resolves with an HTTPS GET to the same server
   (pkarr.rs line 536 and the PkarrRelayClient GET).
3. DnsAddressLookup::n0_dns(). It queries the TXT record
   `_iroh.<z32 endpoint id>.dns.iroh.link.` (src/address_lookup/dns.rs). It
   sends new queries at 200, 300, 600, 1000, 2000 and 3000 ms if no answer
   comes (DNS_STAGGERING_MS, dns.rs line 22). Each query has a 3 s timeout.
4. The default relay mode: four n0 relays, use1-1, usw1-1, euc1-1 and aps1-1
   under relay.n0.iroh.link (src/defaults.rs line 27 to 33).

Seer does not add mDNS, the Mainline DHT, or a MemoryLookup. Every Seer
endpoint, the dialers too, runs the publisher. So each client dial also
publishes the client device key and its home relay URL to dns.iroh.link.

### Sequence of one cold dial

1. Bind. The endpoint starts its net report and picks a home relay in the
   background.
2. `connect` sends ResolveRemote to the remote state actor for the peer
   (src/socket.rs, resolve_remote, line 1321).
3. The actor has no path for the peer, so it starts lookup
   (src/socket/remote_map/remote_state.rs, trigger_address_lookup, line 866).
   AddressLookupServices::resolve queries all services at the same time and
   merges the results (src/address_lookup.rs, resolve, line 553).
4. The first result that adds a path wakes the waiting connect
   (src/socket/remote_map/remote_state/path_state.rs, insert_multiple, line
   135). Slower services still add paths later.
5. The result is the broker home relay URL, because the broker publishes only
   relay addresses (next section). The dialer opens a relay connection to
   that URL and starts the QUIC handshake through the relay. A connection to a
   relay that is not the home relay closes when it is idle
   (src/socket/transports/relay/actor.rs, line 9 and 64).
6. After the handshake, the client side starts NAT traversal and tries direct
   paths (remote_state.rs, do_holepunching, line 924; "Only the client opens
   paths", line 1036).
7. If no path answers, the QUIC connect times out after 10 seconds
   (src/endpoint.rs, comment at line 1137).

A cold dial thus pays, at minimum: DNS for dns.iroh.link and a TLS handshake
for the Pkarr GET, or one TXT query; then DNS, TCP and TLS to the broker relay;
then the QUIC handshake through the relay. Issue 386 reports about 3.8 s for a
same machine join over iroh. That number includes all of this and more. This
note does not split it. The split is the job of the measurement in Phase 2.

## Caches and warm behavior

| Cache | Scope | Lifetime | Seer effect |
| --- | --- | --- | --- |
| Remote state actor paths | One endpoint, in memory | Actor stops 60 s after the last connection closes (ACTOR_MAX_IDLE_TIMEOUT, remote_state.rs line 73). All state goes when the endpoint closes. | None. Seer closes the endpoint after each dial. |
| hickory resolver cache | One DnsResolver, made per endpoint by default (src/endpoint.rs line 256) | Positive answers use the record TTL. Negative answers are not cached (negative_max_ttl zero, iroh-dns-1.1.0/src/dns.rs line 756). | None across dials, for the same reason. |
| Pkarr HTTPS GET | No iroh cache | Each lookup is a new GET. | Full cost every dial. |
| System stub resolver | Host wide | Record TTL. The Pkarr TTL is 30 s (DEFAULT_PKARR_TTL, pkarr.rs line 143). | On this machine /etc/resolv.conf points to systemd-resolved (127.0.0.53, stub mode). A second dial within 30 s can get the TXT answer from that cache. This depends on the host. |
| n0 DNS server | Remote | Not in the iroh crate. The TTL comment in pkarr.rs says the server keeps the record and ignores the TTL for it. Retention time is not verified. | Unknown. |

A warm lookup, in the sense of issue 391, means an endpoint that already knows
a path to the peer. In iroh, `resolve_remote` then returns at once
(path_state.rs line 161), and the actor starts no lookup while a path is
selected (remote_state.rs line 866). Seer never reaches this state across
dials, because it does not reuse an endpoint. Inside one dial_session, streams
after the first reuse the open connection and do no lookup.

## Address lifetime

- The broker key is stable. seer start keeps it in `<state dir>/iroh.key`
  (server.rs line 263). The client device key is stable in
  `<config dir>/device.key`. So a published record keeps its name across
  restarts.
- The publisher sends a new record when the endpoint data changes, and again
  every 5 minutes if nothing changes (DEFAULT_REPUBLISH_INTERVAL, pkarr.rs line
  146, and PublisherService::run, line 376). After a failed publish it retries
  after 1 s, 2 s, 3 s and so on.
- The record holds the home relay URL. The home relay changes only when the net
  report picks another relay. For one host on one network it is usually the same
  relay, but iroh gives no promise.
- Direct addresses are not published. If a later change put them in an invite,
  their life would be the life of the NAT mapping and the local network: a
  laptop that moves, a DHCP renew, or a router restart changes them. They can
  change many times during a seven day seat.
- When the broker stops, its record stays on the n0 server. A dialer then finds
  a relay URL where nobody listens and waits for the 10 s QUIC timeout.

## Privacy

What each item reveals, and to whom:

| Item | Today | If put in the invite |
| --- | --- | --- |
| Broker endpoint id | In the invite, in the server store, and in the n0 DNS name. | No change. |
| Broker home relay URL | Public. Anyone with the endpoint id can read it from dns.iroh.link. It shows one of four regions. | No new exposure. The invite already holds the key. |
| Broker direct addresses (LAN and public IP and port) | Not published (PkarrPublisher uses AddrFilter::relay_only, pkarr.rs line 168). A client that completes the QUIC handshake gets the broker candidates during NAT traversal. The broker accepts a QUIC handshake from any key before the seat check. | New exposure. The address goes to everyone who sees the invite text, with no network action, and it stays in chat logs after the seat expires. A public IP shows the rough location and ISP of the host. A LAN address shows the network layout. |
| Client device key and home relay URL | Published by every client dial. | Not affected by an invite change. |

Requirement: an invite must not carry a direct address by default. A relay URL
hint is acceptable on privacy grounds. If a direct address hint is ever
offered, it must be opt in by the host, and the note in the UI must say that the
invite then shows the host address.

## Invite compatibility

The parser (crates/seer/src/capsule.rs, parse and parse_iroh) does this for
SEER2: strip the prefix, split at the first hyphen, require a 64 character
lower case hex key, and take all the rest as the seat token. The token may hold
hyphens. It must not hold white space.

New invite, old client:

| Hint placement | Old client result |
| --- | --- |
| Appended to SEER2, for example `SEER2-<key>-<token>-<hint>` | Parse succeeds. The token becomes `<token>-<hint>`. The broker does not find that seat and refuses the join. The user sees a refusal, not "invalid invitation". The seat is not used. This is a bad failure: it looks like a wrong or expired invite. |
| Inside the key field, for example `SEER2-<key>.<hint>-<token>` | Parse fails: the key is not 64 hex characters. The user sees "invalid invitation". |
| New prefix, for example SEER3 | Parse fails: unknown prefix. The user sees "invalid invitation". |

Old invite, new client: a new client must keep reading SEER2 with no hint and
dial by key only, as today. A seat can live seven days, so SEER2 invites made
before an upgrade stay valid for at least that long.

Requirements:

- A hint format must use a new prefix, so that an old client fails with
  "invalid invitation" and never sends a wrong seat token. The upgrade message
  should tell the user to update Seer.
- A new client must accept SEER1 and SEER2 as today.
- The broker must be able to print a SEER2 invite, for friends on old
  versions. Which form is the default is a later decision.
- The hint must not be saved as the durable server address. ServerEntry keeps
  `iroh:<key>`. A hint may be kept as a separate, replaceable cache entry.
- The seat token and the key must parse the same way in both formats, so the
  identity check does not change.

## Stale address recovery

What iroh does with an address that the application gives
(EndpointAddr with a relay URL or IP address):

- The address goes into the path set with source App (remote_state.rs,
  handle_msg_resolve_remote, line 850). Since the path set is no longer empty,
  connect continues at once.
- Lookup still starts, because no path is selected yet (trigger_address_lookup,
  line 866). Lookup results add more candidates to the same path set.
- A path that fails is marked unusable or inactive (path_state.rs,
  abandoned_path, line 104). The connection uses any other path that works.
- iroh has a test for this case: a wrong address from an old ticket, and the
  connect still succeeds through lookup (src/address_lookup.rs,
  address_lookup_with_wrong_existing_addr, line 1122).
- The QUIC handshake proves the peer key. A hint that points to another host
  cannot pass as the broker. It can only waste time.

Requirements for any hint or cache:

- Treat every hint as a candidate. Never skip lookup because a hint exists.
- Always dial with the expected endpoint id from the invite or the server
  store. Never take the peer identity from the hint.
- Keep the relay fallback. A direct address hint must not replace the relay
  path.
- Bound the cost of a stale hint. With iroh 1.1.0 lookup runs in parallel, so
  the cost should be near zero, but this must be measured before a decision.
- A cache on disk must record when each address was seen and drop it after a
  set age. A cache hit that fails must not block the next dial.
- If the broker is down, the failure time must not grow. Today it is the 10 s
  QUIC timeout.

## Options for later work

These are not in scope for issue 391. They are here so that the measurement can
check them.

1. Reuse one endpoint per process. A second dial from the same process is then
   warm. This fits the runtime reconnect loop best. It changes seer-net only.
2. Put the broker relay URL in a new invite prefix. It saves the cold lookup
   for the first join. It adds no new exposure.
3. Keep the last good relay URL per server in the client store, next to
   ServerEntry, and pass it as a candidate. It saves the lookup for later
   commands and reconnects.
4. Add a direct address hint, opt in. Highest privacy cost. Only worth it if
   the measurement shows that the relay path, not lookup, is the main delay.

## Measurements

Status: not yet done. Phase 2 waits for the baseline harness from issue 389.

Plan:

- Release build, this machine.
- Cold: a new endpoint dials the broker by key only. At least 10 samples.
- Warm: an endpoint that already has a connection or a recent path to the
  broker dials again. At least 10 samples.
- Record the lookup time (connect call to first path), the time to the QUIC
  handshake, and which service answered first (pkarr or dns).
- Record whether the systemd-resolved cache was cold or warm for each sample.
- Raw samples go under docs/research/perf-samples/391/.

The "Ready when" line of issue 391 needs these numbers to show if the lookup
delay is material.
