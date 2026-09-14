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
  did not make a connection fail, but it added about 1 s to the handshake.
- Measured on this machine: a lookup with a cold DNS cache takes 130 ms
  (median). That is 40 percent of a cold seer_net::dial (322 ms). With a warm
  DNS cache the lookup takes 1.6 ms. The delay is material for each
  connection. It is not the main cost of seer start, which waits 3.1 s for
  the relay. Details are in the section "Measurements".

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
7. A comment says that the connect times out after 10 seconds if no
   reachable address is available (src/endpoint.rs, line 1137). This does
   not cover a relay URL where the peer is gone. In the peerdown run below,
   the handshake did not fail within the 15 s probe limit.

A cold dial thus pays, at minimum: DNS for dns.iroh.link and a TLS handshake
for the Pkarr GET, or one TXT query; then DNS, TCP and TLS to the broker relay;
then the QUIC handshake through the relay. The section "Measurements" gives
the lookup time and the handshake time. It does not split them further.

## Caches and warm behavior

| Cache | Scope | Lifetime | Seer effect |
| --- | --- | --- | --- |
| Remote state actor paths | One endpoint, in memory | Actor stops 60 s after the last connection closes (ACTOR_MAX_IDLE_TIMEOUT, remote_state.rs line 73). All state goes when the endpoint closes. | None. Seer closes the endpoint after each dial. |
| hickory resolver cache | One DnsResolver, made per endpoint by default (src/endpoint.rs line 256) | Positive answers use the record TTL. Negative answers are not cached (negative_max_ttl zero, iroh-dns-1.1.0/src/dns.rs line 756). | None across dials, for the same reason. |
| Pkarr HTTPS GET | No iroh cache | Each lookup is a new GET. | Full cost every dial. |
| System stub resolver | Host wide | Record TTL. The Pkarr TTL is 30 s (DEFAULT_PKARR_TTL, pkarr.rs line 143). | On this machine /etc/resolv.conf points to systemd-resolved (127.0.0.53, stub mode). A dial within 30 s of the last lookup gets the TXT answer from that cache: 1.6 ms against 130 ms (see "Measurements"). This depends on the host. |
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
  a relay URL where nobody listens. In the peerdown run, all 10 dials were
  still waiting for the handshake when the probe stopped them at 15 s.
  seer_net::dial has no timeout of its own around connect.

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
- Bound the cost of a stale hint. Lookup runs in parallel, but a stale IP or
  relay hint still added about 1000 ms to the cold handshake (1194 ms against
  191 ms). The handshake does not move to the looked up path at once. A fresh
  hint saves at most the lookup, 130 ms with a cold DNS cache. So a hint only
  pays off if fewer than about 1 in 9 hints is stale.
- A cache on disk must record when each address was seen and drop it after a
  set age. A cache hit that fails must not block the next dial.
- If the broker is down, the failure time must not grow. Today a dial to a
  stopped broker did not fail within 15 s.

## Options for later work

These are not in scope for issue 391. The numbers come from the section
"Measurements".

1. Reuse one endpoint per process. A second dial from the same process is then
   warm: lookup 0.079 ms, handshake 0.354 ms, first stream 0.166 ms, against a
   cold seer_net::dial of 199.5 ms to 322.5 ms. It removes both the lookup and
   the relay handshake. It has no stale address risk and no privacy cost. It
   fits the runtime reconnect loop best. It changes seer-net only. It does not
   help the first dial of a new process, such as seer join.
2. Put the broker relay URL in a new invite prefix. With a correct relay hint
   the lookup leaves the critical path (0.092 ms), but the handshake does not
   change (194.3 ms). So it saves the lookup only: 130 ms with a cold DNS
   cache, 1.6 ms with a warm one. A stale relay hint costs about 1000 ms. It
   adds no new exposure.
3. Keep the last good relay URL per server in the client store, next to
   ServerEntry, and pass it as a candidate. Same saving and same stale cost as
   option 2. The hint goes stale when the broker home relay changes.
4. Add a direct address hint, opt in. Highest privacy cost. A stale IP hint
   costs about 1000 ms, the same as a stale relay hint. The LAN gain is not
   measured.

## Measurements

### Runs

| Run | Revision | Samples | Command |
| --- | --- | --- | --- |
| Baseline of issue 389 | 367e50c | docs/research/perf-samples/389/ | flock /tmp/claude-1000/perf-run.lock scripts/perf/transport.sh 20 |
| This issue | e126068 | docs/research/perf-samples/391/ | flock /tmp/claude-1000/perf-run.lock scripts/perf/lookup.sh 10 docs/research/perf-samples/391 |

Both runs: this machine (AMD Ryzen 7 7800X3D, Linux 7.1.6-1-cachyos), release
build (lto fat, codegen-units 1), iroh 1.1.0, serve and dial on the same
machine, serve home relay https://use1-1.relay.n0.iroh.link./. The method,
boundaries, and CSV format are in 18-transport-baseline.md. Revision e126068
is 367e50c plus scripts/perf/lookup.sh. The probe is the same. The run of
this issue had 0 failures in the dials that must succeed, and an empty
errors.log.

scripts/perf/lookup.sh adds these conditions. All use `transport_probe dial
--api iroh` unless the table says seer. The condition name is only a label.
The IP transports stay on.

| Condition | Dialer gets | How |
| --- | --- | --- |
| discovery_dnscold | Endpoint id only | 10 fresh processes, 35 s gap before each, so the TXT record (TTL 30 s) is not in the systemd-resolved cache. iroh and seer api. |
| relayhint | Id and the correct relay URL | cold_and_warm, as in the baseline |
| staleip | Id and 192.0.2.1:9 (TEST-NET-1, no host answers) | cold_and_warm |
| stalerelay | Id and https://euc1-1.relay.n0.iroh.link./, a relay where the peer is not | cold_and_warm |
| peerdown | Endpoint id only. The serve process was killed after it published. | 10 fresh processes, no gap |

A DNS cache flush was not possible. `resolvectl flush-caches` failed with
"Connection timed out" without root. The 35 s gap is used instead. The
peerdown run confirms the gap works: its samples come about 15 s apart, and
the lookup alternates between 128 ms and 1.6 ms, a cache hit on every second
sample. The gap makes only the TXT record miss. Other records, such as the
address of dns.iroh.link, can still be in a cache. A host that has never used
iroh can be slower. That case is not measured.

### Results

Medians and p95 in ms. Cold rows include sample 0 of the warm process. The
baseline rows are copied from perf-samples/389/summary.csv.

| Source | Workload | Condition | Cache | Boundary | n | Median | p95 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 389 | iroh_dial | discovery | cold | address_lookup | 21 | 1.649 | 129.475 |
| 389 | iroh_dial | discovery | cold | dial_handshake | 21 | 191.470 | 200.193 |
| 389 | iroh_dial | discovery | warm | address_lookup | 20 | 0.079 | 0.090 |
| 389 | iroh_dial | discovery | warm | dial_handshake | 20 | 0.354 | 0.398 |
| 389 | iroh_dial | discovery | warm | stream_open | 20 | 0.166 | 0.217 |
| 389 | seer_dial | discovery | cold | seer_dial | 21 | 199.532 | 204.956 |
| 389 | seer_dial | discovery | warm | seer_dial | 20 | 196.545 | 205.171 |
| 391 | iroh_dial | discovery_dnscold | cold | address_lookup | 10 | 129.913 | 132.855 |
| 391 | iroh_dial | discovery_dnscold | cold | dial_handshake | 10 | 191.235 | 198.888 |
| 391 | seer_dial | discovery_dnscold | cold | seer_dial | 10 | 322.475 | 330.518 |
| 391 | iroh_dial | relayhint | cold | address_lookup | 11 | 0.092 | 0.106 |
| 391 | iroh_dial | relayhint | cold | dial_handshake | 11 | 194.319 | 201.442 |
| 391 | iroh_dial | staleip | cold | address_lookup | 11 | 0.097 | 0.117 |
| 391 | iroh_dial | staleip | cold | dial_handshake | 11 | 1193.852 | 1201.347 |
| 391 | iroh_dial | staleip | warm | dial_handshake | 10 | 0.350 | 0.420 |
| 391 | iroh_dial | stalerelay | cold | address_lookup | 11 | 0.095 | 0.125 |
| 391 | iroh_dial | stalerelay | cold | dial_handshake | 11 | 1192.570 | 1200.982 |
| 391 | iroh_dial | stalerelay | warm | dial_handshake | 10 | 0.350 | 0.380 |
| 391 | iroh_dial | peerdown | cold | address_lookup | 10 | 64.540 | 134.590 |
| 391 | iroh_dial | peerdown | cold | dial_handshake | 0 ok, 10 fail | - | - |

In the baseline, 19 of the 21 cold discovery lookups took 1.5 to 1.9 ms and
2 took 129 to 131 ms. So the cold rows of the baseline mostly had a warm DNS
cache. The discovery_dnscold rows are the cold DNS case. All 10 peerdown
handshakes ended with fail:timeout at the 15 s probe limit (15000.9 to
15001.9 ms).

### What the numbers say

- Cold lookup: 129.9 ms median with a cold DNS cache, 1.6 ms with a warm one.
  The cold seer_net::dial goes from 199.5 ms to 322.5 ms, 123 ms more. So the
  lookup is 40 percent of a cold Seer dial when the DNS cache is cold.
- Warm lookup: an endpoint that knows the peer does no lookup (0.079 ms) and
  reuses the path (handshake 0.354 ms). Seer never reaches this state, because
  seer_net::dial binds a new endpoint each time. A warm seer_dial (196.5 ms)
  costs the same as a cold one.
- Relay hint: it removes the lookup (0.092 ms) and does not change the relay
  handshake (194.3 ms against 191.2 ms).
- Stale hint: the connection still succeeds, but the cold handshake takes
  about 1.19 s, about 1000 ms more than with lookup alone. This holds for a
  stale IP and for a stale relay URL.
- Stopped peer: the dial does not fail within 15 s. The time until it fails is
  not measured.

Verdict for the "Ready when" line: the address lookup delay is material for
each room connection. With a cold DNS cache it adds 130 ms, 40 percent of a
cold dial, and 65 percent of the 200 ms first connection target in issue 386.
Seer pays it on every join, client command, and runtime reconnect that comes
more than 30 s after the last lookup on the host, because each dial uses a new
endpoint. It is not the main cost of seer start or seer join. The relay online
wait of the listener is 3.1 s, and issue 386 reports about 3.8 s for a join.
A hint in the invite saves at most the 130 ms, and a stale hint costs about
1000 ms.

### Not measured

- Which lookup service answered first (Pkarr over HTTPS or DNS). The probe
  times only the merged lookup.
- A host with no cached records at all, see the note on the gap above.
- The time until a dial to a stopped peer fails, beyond 15 s.
- A graceful broker stop. The peerdown run killed the serve process.
- The lan condition, a direct address hint on a LAN, and macOS.
