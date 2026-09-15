# Startup readiness: local bind and relay registration

Research date: 2026-09-13. Issue 392 (R-004). Source revision: 8714a51.
iroh version: 1.1.0 (Cargo.lock).

## Scope

This note maps every caller that waits for the room listener. For each
caller it says which readiness the caller needs, what evidence proves that a
join can succeed, and how Seer must report relay delay, relay outage, and
later recovery. It also explains why the early readiness attempt on the
perf/370 branch failed.

This note does not change startup behavior. Measurements are in the section
"Measurements". The issue 222 guarantee stays in force.

## Terms

- Local bind: the broker has bound its loopback TCP listener, and a local
  client on the same computer can connect to it.
- Endpoint bind: Endpoint::builder(...).bind() in seer-net has returned. The
  UDP sockets are open. The endpoint ID is known. No relay is known yet.
- Relay selected: iroh has picked a home relay. The relay URL is now a local
  address of the endpoint. The relay connection is not complete.
- Relay registered: the home relay handshake is complete. This is what
  Endpoint::online waits for in iroh 1.1.0 (endpoint.rs, method online).
- Address published: the pkarr publisher has sent a signed record with the
  relay URL to the n0 DNS server, and a dialer can resolve it.
- Local ready: local bind is done. Owner commands and the owner runtime can
  work.
- Remote ready: relay registered and address published. A friend who has
  only the endpoint ID can dial the room.

## How the listener starts today

The broker starts in seer-broker run() (crates/seer-broker/src/lib.rs):

1. server::bind_remote_listener binds the iroh listener. This calls
   seer_net::Listener::bind.
2. Listener::bind starts a thread. The thread binds the endpoint, then waits
   for endpoint.online(). Only then does it send the endpoint ID back. The
   caller waits at most ONLINE_TIMEOUT, 4 seconds
   (crates/seer-net/src/lib.rs, lines 25 and 92).
3. If bind or the wait fails, run() returns an error and the broker exits.
4. Only after step 2 does run() bind the loopback TCP listener.
5. serve() sets remote_endpoint from the listener ID. Every invite uses it
   (BrokerState::invite, crates/seer-broker/src/server.rs).

seer start (crates/seer/src/start.rs) spawns the broker and polls the TCP
port for at most START_TIMEOUT, 5 seconds. It prints "Server started" and,
on a first start, the invitation only after the port accepts.

Result: the TCP port is the only readiness signal that seer start sees. The
order in run() makes that one signal mean "local bind and relay registered".
Any change that binds TCP first removes the remote half of the signal. The
change must then add a separate remote signal.

Issue 386 measured this path on 2026-09-10: broker spawn to port open took
3129 ms, and endpoint bind took 2 ms. So almost all of the start time is the
wait between endpoint bind and relay registered. This note does not repeat
that measurement. See "Measurements".

## What online proves and what it does not prove

From the iroh 1.1.0 source:

- online() returns when at least one home relay reports a connected status.
  It has no timeout. With no WAN it waits forever.
- online() does not wait for address lookup. The docs say so directly.
- The endpoint publishes its address when its local addresses change
  (socket.rs, publish_my_addr). The relay URL becomes a local address when
  iroh selects the home relay, in any connection state
  (transports/relay.rs, local_addr_watch). So the publish starts at relay
  selected, before relay registered.
- The N0 preset publishes with the relay_only filter. The record holds the
  relay URL only, no IP addresses. If the endpoint has no relay yet, it
  publishes nothing.
- The publish is a background HTTP request (pkarr.rs, PublisherService). On
  failure it retries after 1 s, 2 s, 3 s, and so on. Nothing in the public
  API reports that a publish succeeded.
- The record TTL is 30 seconds (DEFAULT_PKARR_TTL). The publisher sends
  again every 5 minutes, or at once when the relay changes.

A friend dials with the endpoint ID only. The capsule has no relay URL and no
IP address (ADR 0002). So the dialer must resolve the ID through pkarr or
DNS. If that lookup finds no record with a usable address, connect fails
with "No addressing information available" (ConnectWithOptsError,
NoAddress). seer-net maps this and every other connect error to "could not
connect to endpoint".

Conclusion: relay registered is necessary for a friend join, but it is not
proof. The proof also needs address published. Today Seer treats online() as
remote ready. No code checks the publish. In the issue 392 run, 20 of 20
dials made at once after online() succeeded, but each took about 124 ms more
than a dial made 5 s later. See "Measurements".

## Why the perf/370 attempt was not safe

Commit 4c3b421 on perf/370-start-latency-render made these changes:

- remote_endpoint came from SecretKey::public, not from a bound listener.
- The loopback TCP listener was bound first.
- Listener::bind moved to a background thread. A bind error was only logged.

This had three separate defects.

1. It broke the issue 222 guarantee. On a first start, seer start printed
   "Server started" and a SEER2 invitation as soon as the TCP port opened,
   about 13 ms after spawn. At that time the relay was not registered. If
   the relay then failed, the broker logged the error and kept running. The
   invitation was unusable, and the owner saw success. Issue 222 forbids
   exactly this.
2. It made invites early for every later invite too. BrokerState::invite
   used the key, not a live listener. A seer invite during the first seconds,
   or after a failed background bind, gave out an ID that nothing served.
3. It gave no retry. After one failed background bind, remote access stayed
   off until the next seer start.

Issue 386 reports a second experiment: a listener that reports ready before
relay registered, tested with the seer-net round_trip test. A dial in that
window failed about one time in three. The source explains the failure:

- The round_trip test makes new keys on every run. So no old record for the
  listener ID exists on the DNS server.
- The dialer binds its own endpoint and resolves the listener ID at once.
- The listener publishes only after relay selected. Relay selected comes
  after the first net report. This note did not measure how much of the
  roughly 3 seconds comes before relay selected.
- If the dialer lookup runs before the listener record is published, the
  lookup ends with no address and connect fails with NoAddress. In the issue
  392 run the failed dials ended 2.5 s to 2.9 s after they started, not at
  once. The seer-net error text does not name the iroh cause, so the run
  does not prove that each failure was NoAddress.
- If the lookup runs after the publish, the dial goes to the relay. It can
  succeed if the listener registers before the QUIC handshake gives up.
  This last point is from reading the source and is not verified.

So the result is a race between two background tasks on two endpoints. It is
not a fixed fault. Issue 386 reports about one in three. The issue 392 run
measured 5 failures in 20 dials. See "Measurements".

A broker with a stable key (seer start keeps iroh.key) can behave
differently. An old record from the last run can still be on the DNS server,
and it can point to the same relay. That can make an early dial succeed more
often. This is not verified. It also does not help a first start, which has
a new key.

## Caller map

"Needs" is what the caller must have before it can do its job. "Waits today"
is what the code makes it wait for.

| Caller | Path | Transport | Needs | Waits today |
|---|---|---|---|---|
| seer start, message "Server started" | start.rs, wait_for_port | TCP loopback | local ready | local and relay registered (order in run) |
| seer start, first invitation | start.rs, first_invite, then BrokerState::invite | TCP loopback to broker, capsule for friends | remote ready | relay registered |
| seer-broker process | lib.rs run, bind_remote_listener | iroh then TCP | local ready to serve the owner, remote ready to serve friends | relay registered, then exits on failure |
| seer invite and TUI invite (owner) | BrokerState::invite | TCP loopback | remote ready | nothing more; remote_endpoint is set once at start |
| Owner client: seer attach, TUI, seer stop | commands.rs connect | TCP loopback (published_addr) | local ready | local and relay registered, as a side effect |
| Owner runtime | local.rs start, SEER_ROOM_ENDPOINT = published_addr | TCP loopback | local ready | retries with backoff 1 s to 30 s (runtime room.rs) |
| Friend seer join | commands.rs join, connect_iroh, seer_net::dial | iroh | remote ready at the room | one attempt per name prompt, no retry |
| Friend seer attach | commands.rs authenticate, connect_iroh | iroh | remote ready at the room | one attempt, no retry |
| Friend TUI reconnect | tui_link.rs Reconnects::start | iroh | remote ready at the room | retries with backoff 1 s to 30 s |
| Friend runtime | runtime room.rs Link::iroh, dial_session | iroh | remote ready at the room | retries with backoff 1 s to 30 s |
| Test: seer-net round_trip | seer-net tests/round_trip.rs | iroh | remote ready | Listener::bind (relay registered), then one dial |
| Test: seer cli join | seer tests/cli.rs, join_persists_... | iroh | remote ready | Listener::bind (relay registered), then one seer join |
| Test: broker room_iroh | seer-broker tests/room_iroh.rs | TCP for the owner, iroh for the runtime | local ready for the invite, remote ready for the runtime | TCP port, then runtime retries |
| Test: seer start remote failure | seer tests/start.rs | fake broker | none (fake fails) | fake broker panics before TCP bind |
| Other broker and seer tests | many | TCP loopback, remote = false | local ready | TCP port only |

Notes on the map:

- No owner path uses iroh. The owner saves published_addr, which is
  127.0.0.1:7321, in servers.toml (start.rs, save_owner). So every owner
  command and the owner runtime need local ready only. Today they still pay
  the relay wait, because the TCP port opens after online().
- The dialer side never calls online(). A friend dial pays its own endpoint
  bind and the address lookup, then a relay or direct path. The dialer does
  not need its own relay registered before it calls connect.
- The two tests that dial once right after Listener::bind depend on the
  small gap between relay registered and address published. They pass
  because the publish usually finishes first. They are the tests most likely
  to fail if the listener ever reports ready earlier.

## Evidence that a join can succeed

Ranked from strongest to weakest:

1. A friend join completed. The broker received a Join message on an iroh
   connection and answered Joined. This is proof, but it only exists after a
   friend acts.
2. A second endpoint resolved the room ID, reached it, and finished one
   stream exchange. This is what the round_trip test does. It costs a second
   endpoint and a relay round trip. Seer does not do this at start today.
3. Relay registered, and the room ID resolves through address lookup to the
   current home relay URL. This proves both halves of remote ready without a
   second endpoint. Seer does not check the second half today.
4. Relay registered only (online() returned). This is what Seer uses today.
   It misses the publish.
5. Endpoint bind only, or an ID made from the key. This is what 4c3b421
   used. It is not evidence of remote reachability. A capsule made on this
   evidence can be unusable.

Rule: an invitation needs evidence 3 or stronger for a future design. Level
4 is the accepted level today, and a change must not go below it. Level 5 is
never enough for an invitation.

## Reporting rules

These rules define the required behavior. Where today's code differs, the
difference is listed under "Gaps". This note does not implement any rule.

### Rules for every caller

- R1. No code path makes or prints a SEER2 capsule before remote ready at
  the evidence level above. This covers the first invitation from seer start,
  seer invite, and the TUI invite.
- R2. A success message names only the state that is true. If a message
  says "Server started", the room is local ready. If it also prints an
  invitation, the room is remote ready.
- R3. A caller that needs local ready only must not wait for the relay.
- R4. A failed remote start is a failure. Issue 222 requires that
  seer-broker exits with an error and that seer start prints neither
  "Server started" nor an invitation. A design that keeps the broker running
  for local use after a relay failure must first get the owner to change the
  issue 222 contract. The "no invitation" half never changes.

### seer start and the broker

- Relay delay (registration takes longer than usual, but ends inside the
  budget): wait, print nothing yet, then report success as today. The budget
  today is ONLINE_TIMEOUT, 4 s, inside START_TIMEOUT, 5 s. So the broker has
  only about 1 s for all other start work after a slow relay.
- Relay outage at start (no registration inside the budget): the broker
  exits with "endpoint did not become ready within 4 seconds". seer start
  prints the broker log tail and exits with code 1. No "Server started", no
  invitation. This is the issue 222 behavior and it must stay.
- Relay outage after start: the broker must not give out new invitations
  while no home relay is connected (R1). It must say why, for example that
  the room cannot be reached from the internet now. Existing friend
  connections that use a direct path can continue.
- Recovery after an outage: iroh reconnects to the relay with backoff and
  publishes again when the relay changes. Invitations become available again
  when the evidence level returns. The broker must not need a restart.

### Owner client and owner runtime

- Relay delay, outage, and recovery: no effect. These paths use TCP
  loopback. After a future change that satisfies R3, they must work as soon
  as the room is local ready, with or without a relay.

### Friend seer join and seer attach

- Relay delay at the friend side or the room side: the dial can take longer.
  The command must not print "Joined" or "Attached" before the room answers.
  This is true today.
- Outage: the command fails with "Cannot reach the server. Check that the
  owner ran seer start and that you both have internet." and a non-zero
  exit code. The friend runs the command again. This is true today.
- Recovery: manual. The command does not retry. A retry loop would be a new
  feature and needs its own ticket.

### Friend TUI and friend runtime

- Delay and outage: each attempt fails, and the caller waits 1 s, then 2 s,
  up to 30 s, and tries again. The runtime writes "runtime room connection
  ended" to its log. Local input and local process lifetime do not depend on
  the room (settled decision in CLAUDE.md), so local shells keep running.
- Recovery: automatic on the next attempt after the room is remote ready
  again. The worst extra delay after recovery is one backoff step, at most
  30 s.

### Tests

- A test that dials once must wait for remote ready at the same evidence
  level that production uses. If a later change lowers what Listener::bind
  waits for, these tests must wait for the new remote signal, or they will
  fail in the same way as the issue 386 experiment.
- Tests that do not need the network must keep remote = false, as most
  broker and seer tests already do.

## Gaps in today's code

These are facts found in the source. They are not fixed here.

- G1. Owner paths pay the relay wait (breaks R3). The TCP port opens only
  after online(). This is the main start cost from issue 370.
- G2. online() does not prove address published. A dial at once after
  online() took about 124 ms more than a settled dial and did not fail in
  20 samples. The single dial tests are exposed to it.
- G3. Outage after start is not reported. remote_endpoint is set once. The
  broker keeps giving out invitations with no check of the relay status
  (breaks R1 after start).
- G4. The start budget is tight. ONLINE_TIMEOUT is 4 s and START_TIMEOUT is
  5 s. A slow but working relay near 4 s leaves little time for the rest of
  the start.
- G5. The only remote signal to seer start is the order of binds inside the
  broker. It is implicit and easy to break.

## Measurements

Two runs give the numbers. All values are in ms. p95 uses the nearest rank.
Median and p95 leave out failed samples. The fail column counts them.

- Baseline run, issue 389: revision 367e50c, 20 samples for each workload,
  raw samples in docs/research/perf-samples/389/. The method and the
  boundaries are in docs/research/18-transport-baseline.md.
- Issue 392 run: revision 3fd2f4b, the same probe and driver with the
  readiness parts added. 20 samples for each workload. Raw samples,
  summary.csv, environment.txt, and errors.log are in
  docs/research/perf-samples/392/. Command:

      flock /tmp/claude-1000/perf-run.lock scripts/perf/transport.sh 20 \
          docs/research/perf-samples/392 "local offline readiness"

Both runs: AMD Ryzen 7 7800X3D, Linux 7.1.6-1-cachyos, rustc 1.98.0,
release profile, iroh 1.1.0 with preset N0, wired link up, DNS through
systemd-resolved with no cache flush.

New workloads in the issue 392 run:

- broker_ready, local: seer-broker with remote = false. Spawn until the
  loopback TCP port accepts. This is local bind with no relay work.
- broker_ready, offline: seer-broker with remote = true in a new network
  namespace (unshare -rn) with no route out. The relay is never reached.
  The row is the time until the broker exits.
- seer_dial, ready_online: a new iroh serve with a new key announces after
  online(), as seer_net::Listener does today. A seer_net::dial starts as
  soon as the driver sees the ready line, with no settle time.
- seer_dial, ready_bind: the same, but serve announces right after endpoint
  bind, before online(). This repeats the early readiness case of issue 386.

The driver polls for the ready line every 0.1 s, so each dial starts within
one poll of the announce.

### Local bind and relay registration

| Run | Workload | Condition | Cache | Boundary | n | fail | Median | p95 |
|---|---|---|---|---|---|---|---|---|
| 389 | iroh_listen | default | cold | endpoint_bind | 21 | 0 | 1.560 | 1.642 |
| 389 | iroh_listen | default | cold | relay_online | 21 | 0 | 3118.939 | 3124.925 |
| 389 | seer_listen | default | cold | seer_listen_ready | 21 | 0 | 3120.110 | 3126.369 |
| 389 | broker_ready | default | cold | process_ready | 21 | 0 | 3121.365 | 3127.723 |
| 389 | broker_ready | default | warm | process_ready | 20 | 0 | 3122.147 | 3127.085 |
| 392 | broker_ready | local | cold | process_ready | 21 | 0 | 0.695 | 0.954 |
| 392 | broker_ready | local | warm | process_ready | 20 | 0 | 0.643 | 0.735 |
| 392 | broker_ready | offline | cold | process_ready | 0 | 21 | - | - |
| 392 | broker_ready | offline | warm | process_ready | 0 | 20 | - | - |

All 41 offline samples have status "fail:broker exited". Their ms values in
samples.csv are the time until the broker exited: minimum 4002.028, median
4002.239, maximum 4008.098. errors.log shows the cause for each: "endpoint
did not become ready within 4 seconds". The broker never opened its TCP
port.

The local rows measure until the kernel accepts a TCP connection. run()
binds the TCP port before serve() opens the registry, so the rows do not
include the first reply from the broker.

### Join success by readiness point

| Run | Workload | Condition | Boundary | n | fail | Median | p95 |
|---|---|---|---|---|---|---|---|
| 389 | seer_dial | discovery (5 s settle) | seer_dial | 21 | 0 | 199.532 | 204.956 |
| 392 | seer_dial | ready_online | seer_dial | 20 | 0 | 323.097 | 333.074 |
| 392 | seer_dial | ready_online | stream_open | 20 | 0 | 86.153 | 96.120 |
| 392 | seer_dial | ready_bind | seer_dial | 15 | 5 | 3323.961 | 3780.133 |
| 392 | seer_dial | ready_bind | stream_open | 15 | 0 | 83.529 | 219.341 |

The 5 failed ready_bind dials have status "fail:could not connect to
endpoint". They ended 2510.409, 2705.409, 2735.657, 2854.218, and 2874.965 ms
after they started. The 15 successful ready_bind dials took 3192.393 to
3780.133 ms.

### What the numbers say

- Local bind and relay registration are separate costs. Local bind for the
  broker is 0.695 ms (median, cold). Relay registration is 3118.939 ms, and
  endpoint bind is 1.560 ms. The broker with remote = true is ready after
  3121.365 ms. So the relay wait is almost all of the start time. Material:
  every owner path (seer start message, owner client, owner runtime) needs
  local ready only, and today it waits about 3.1 s more than local bind
  (gap G1).
- The issue 222 path works. With no route to the relay, every broker exited
  at the 4 s ONLINE_TIMEOUT and never opened its port, so seer start cannot
  report success. Material for gap G4: the exit comes at 4002 ms, inside the
  5000 ms seer start budget. A relay that is slower than 4 s makes start
  fail, even when a local user could work.
- Announcing at bind is not safe and does not make a join faster. 5 of 20
  dials failed (25 percent), in line with issue 386. The dials that
  succeeded took a median of 3323.961 ms from the announce, which is later
  than today's path: relay registration (3118.939 ms) plus a dial at once
  after online (323.097 ms). Material: an early announce trades failures for
  no gain at the friend side. A later design must not announce remote ready
  at bind.
- Announcing at online is safe in this run. 20 of 20 dials made at once
  after online succeeded. Each took about 124 ms more than a dial made 5 s
  later (323.097 against 199.532 ms, median). The run does not show the
  cause of the extra time. Not material for a person, who needs seconds to
  paste the line. It is relevant for tests that dial at once (gap G2): 20
  samples do not prove that the gap never causes a failure.
- A friend join today is bounded by relay registration at the room, not by
  the dial. Local work does not need either.

### Not measured

- Relay selected. It needs a boundary inside relay_online. Adding it would
  change the start point of the existing relay_online boundary, so this pass
  did not add it.
- Address published. iroh 1.1.0 has no public signal for it.
- The iroh cause of each failed ready_bind dial. seer_net::dial hides it.
- Relay delay between 0 and 4 s, and recovery after an outage at run time.
  The driver can remove the network only for the whole broker process.
- A stable room key. Every ready_* sample used a new key, so no old address
  record existed. seer start keeps its key, so a real restart can differ.
- The full seer start and seer join commands, as in the baseline.
- lan and macOS.

## Sources

- crates/seer-net/src/lib.rs: Listener::bind, run_listener, run_dialer,
  ONLINE_TIMEOUT.
- crates/seer-broker/src/lib.rs: run, order of binds.
- crates/seer-broker/src/server.rs: serve, bind_remote_listener,
  BrokerState::invite.
- crates/seer/src/start.rs: start_broker, complete_start, wait_for_port,
  save_owner.
- crates/seer/src/commands.rs: join, connect, connect_iroh.
- crates/seer/src/tui_link.rs: Reconnects::start.
- crates/seer-runtime/src/room.rs: publish_loop, Link.
- iroh 1.1.0: src/endpoint.rs (online, ConnectWithOptsError),
  src/endpoint/presets.rs (N0), src/socket.rs (publish_my_addr),
  src/socket/transports/relay.rs (local_addr_watch),
  src/address_lookup/pkarr.rs (PublisherService, DEFAULT_PKARR_TTL),
  src/socket/remote_map/remote_state.rs (address lookup on connect),
  src/socket/transports/relay/actor.rs (reconnect with backoff).
- Commit 4c3b421 on perf/370-start-latency-render.
- Issues 222, 370, 386, 389, 392. ADR 0002.
