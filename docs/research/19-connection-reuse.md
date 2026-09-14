# Repeated cold connection setup with iroh

Research date: 2026-09-13. Issue 390 (R-002). Code state: origin/main at
8714a51, iroh 1.1.0, tokio 1.53.1. The measurement probe comes from issue
389 (commit 367e50c) with one small extension (commit d936f8f).

## Scope

This note maps every place where Seer makes an iroh connection, and records
what a safe connection reuse must keep. It does not implement reuse. It
measures the cost of repeated setup against endpoint reuse and connection
reuse.

## What one standalone dial does

`seer_net::dial` ([lib.rs:133](../../crates/seer-net/src/lib.rs)) does these
steps each time:

1. It starts one detached OS thread, `seer-net-dialer`. The caller gets no
   join handle.
2. It builds one new tokio multi-thread runtime
   ([lib.rs:416](../../crates/seer-net/src/lib.rs)). The default worker count
   is one per CPU core. The measurement machine has 16 cores.
3. It binds one new iroh endpoint with the N0 preset
   ([lib.rs:316](../../crates/seer-net/src/lib.rs)). This makes new UDP
   sockets, a new relay client, and new discovery state.
4. It connects to the remote endpoint ID with the `seer/1` ALPN.
5. It opens one bidirectional stream and bridges it to a Unix socket pair.
6. When the bridge ends, it closes the endpoint.

Nothing stays after step 6. The next dial does all six steps again.

`seer_net::dial_session` ([lib.rs:147](../../crates/seer-net/src/lib.rs)) does
steps 1 to 4 in the same way. Then it keeps the connection, and both sides can
open more streams on it. `Listener::bind`
([lib.rs:83](../../crates/seer-net/src/lib.rs)) does steps 1 to 3 once per
process and then waits up to 4 s for the endpoint to be online.

## Who uses iroh

The owner never dials over iroh. `seer start` sets `published_addr` to
`127.0.0.1:7321` ([start.rs:143](../../crates/seer/src/start.rs)) and saves it
as the owner's server endpoint ([start.rs:458](../../crates/seer/src/start.rs)).
The owner's commands and the owner's runtime use loopback TCP.

Only a person who joined with a SEER2 invitation has an `iroh:<id>` endpoint
([commands.rs:124](../../crates/seer/src/commands.rs),
[capsule.rs:55](../../crates/seer/src/capsule.rs)). All repeated setup costs in
this note apply to that person.

## Caller map

### Client: the seer binary

The only product caller of `dial` is `connect_iroh`
([commands.rs:352](../../crates/seer/src/commands.rs)). It loads `device.key`
from the config directory and dials. Every command reaches it through
`connect` ([commands.rs:335](../../crates/seer/src/commands.rs)) or
`authenticate` ([commands.rs:301](../../crates/seer/src/commands.rs)). Each
call makes a new endpoint and a new runtime. No call shares a connection with
another call.

| Command | Call sites | Cold dials per run |
| --- | --- | --- |
| join | commands.rs:95, commands.rs:134 | 2, plus 1 for each "name is in use" retry |
| attach | tui.rs:56, tui_link.rs:107 | 1 per try in a background thread, backoff 1 s to 30 s |
| room loss in the window | tui.rs:117, tui_link.rs:107 | 1 per try, same backoff |
| peek | selection.rs:128, selection.rs:137 | 2, the first stream is dropped after ListPeople |
| invite | commands.rs:158 | 1 |
| detach | commands.rs:50 | 1 |
| leave | lifecycle.rs:10 | 1 |
| perms | lifecycle.rs:70 | 1 |
| list | commands.rs:197, commands.rs:314 | 1 per saved server, one after the other |
| stop | lifecycle.rs:35, lifecycle.rs:39 | see below |

After join, the client starts the local runtime, which makes its own
connection (next section). So a first join makes at least three cold endpoints
in two processes.

`seer stop` dials once, then polls `connect` every 25 ms for up to 5 s until
the broker is gone ([lifecycle.rs:39](../../crates/seer/src/commands/lifecycle.rs)).
Over iroh each poll is a full cold dial. The broker accepts Stop only from the
host ([forwarding.rs:175](../../crates/seer-broker/src/forwarding.rs)), and the
host uses TCP, so this loop does not run over iroh in normal use.

`first_invite` during `seer start`
([start.rs:297](../../crates/seer/src/start.rs)) uses the owner's TCP endpoint.
It makes no iroh dial.

### Runtime: the seer-runtime process

`Link::iroh` ([room.rs:139](../../crates/seer-runtime/src/room.rs)) is the only
caller of `dial_session`. It loads `runtime.key` from the runtime directory
([local.rs:147](../../crates/seer/src/local.rs)).

- `publish_once` ([room.rs:74](../../crates/seer-runtime/src/room.rs)) calls it
  once per turn of `publish_loop`
  ([room.rs:53](../../crates/seer-runtime/src/room.rs)). Each turn makes a new
  endpoint and a new runtime. When the link fails, the session drops, the
  endpoint closes, and the next turn after the backoff (1 s, doubling to 30 s)
  starts cold again.
- Existing sharing: inside one live link, the control stream
  ([room.rs:75](../../crates/seer-runtime/src/room.rs)) and every viewer stream
  ([room.rs:110](../../crates/seer-runtime/src/room.rs)) go over one QUIC
  connection through `Session::open`
  ([session.rs:25](../../crates/seer-net/src/session.rs)). A viewer stream
  does not make a new endpoint.

### Broker: the seer-broker process

The broker never dials. It loads `iroh.key` from its state directory and binds
one `Listener` for the life of the process
([server.rs:263](../../crates/seer-broker/src/server.rs), called from
[lib.rs:51](../../crates/seer-broker/src/lib.rs)). All incoming connections
share this one endpoint.

Each accepted connection comes with a `Session`
([lib.rs:347](../../crates/seer-net/src/lib.rs)). The broker uses it only for a
runtime connection: `spawn_stream_pump`
([publishing.rs:89](../../crates/seer-broker/src/publishing.rs)) accepts the
runtime's viewer streams. For a client connection, `handle_connection`
([server.rs:318](../../crates/seer-broker/src/server.rs)) never reads the
session. A second stream that a client opens on the same connection is never
served. Client connection reuse needs a broker change here.

### Tests

- [round_trip.rs:53](../../crates/seer-net/tests/round_trip.rs) dials once.
- The join test in [cli.rs:118](../../crates/seer/tests/cli.rs) binds a
  `Listener` and expects three accepted connections with the same device ID.
  It pins the current one connection per command shape. A reuse change must
  update it.

## Existing sharing, summary

| Process | Endpoint shared | Connection shared |
| --- | --- | --- |
| broker | yes, one per process | not applicable, it only accepts |
| runtime | no, one per link attempt | yes, all streams of one link |
| client | no, one per dial | no, one per dial |

## Requirements for safe reuse

### Ownership

- A CLI command is one short process. Reuse across commands needs an owner
  that lives longer than one command. Inside one process, the owner can be one
  endpoint that the first dial makes and process exit drops.
- The window process lives for the whole session. Its `Reconnects` thread
  ([tui_link.rs:100](../../crates/seer/src/tui_link.rs)) makes every room
  connection for that session, so it is the natural owner there.
- In the runtime, `publish_once` owns the session through `Link`. An endpoint
  that lives across reconnects must be owned by `publish_loop`.
- Issue 338 is settled: local input and process lifetime must not depend on
  the room connection. A shared endpoint must belong to the process that uses
  it, and its failure must not stop local terminals.

### Identity

- Client endpoint ID comes from `device.key`
  ([store.rs:56](../../crates/seer/src/store.rs)). There is one per OS user
  config directory. Every server and every seer process on that device uses it.
- Runtime endpoint ID comes from `runtime.key` in the runtime directory
  ([local.rs:147](../../crates/seer/src/local.rs)). It is not the device key.
  Do not merge the two keys.
- The broker does not bind room identity to the endpoint ID. It checks the user
  ID and credential in the first message of each stream
  ([commands.rs:303](../../crates/seer/src/commands.rs),
  [room.rs:78](../../crates/seer-runtime/src/room.rs)). The endpoint ID is only
  the connection limit key `ConnectionKey::Relay`
  ([server.rs:274](../../crates/seer-broker/src/server.rs)).
- The iroh relay keeps one active client per endpoint ID. A second endpoint
  with the same ID makes the relay mark the first one inactive with
  `SameEndpointIdConnected` (iroh-relay 1.1.0, src/server/clients.rs lines 73
  to 100). Today two seer processes on one device, for example a window that
  reconnects and a `seer invite` in another terminal, bind two endpoints with
  the same device key at the same time. A shared endpoint per process keeps
  this conflict. A shared endpoint per device removes it.
- A reuse design must keep the same secret key for each role, so invitations,
  capsules, and the broker's limit per source do not change.

### Cancellation

- `dial` has no caller deadline. It blocks on `result_rx.recv()`
  ([lib.rs:138](../../crates/seer-net/src/lib.rs)) until iroh gives up. The
  client's 5 s `NETWORK_TIMEOUT` applies only after the stream exists
  ([commands.rs:343](../../crates/seer/src/commands.rs)).
- `dial_session` has no deadline either
  ([lib.rs:155](../../crates/seer-net/src/lib.rs)). `Session::open` has a
  10 s deadline ([session.rs:10](../../crates/seer-net/src/session.rs)).
- `Reconnects::stop` sets a flag that the thread reads only between tries
  ([tui_link.rs:106](../../crates/seer/src/tui_link.rs)). A dial that has
  started runs to its end.
- `Listener::bind` is the only step with a hard deadline, 4 s
  ([lib.rs:25](../../crates/seer-net/src/lib.rs)).
- With a shared endpoint, one caller must be able to give up on its connect
  without closing the endpoint or the connections of other callers.

### Isolation

- Per remote: a connection goes to one broker endpoint ID. Each server in
  servers.toml must get its own connection. One endpoint can serve many
  remotes.
- Per stream: every Seer stream starts with its own first message (Hello,
  Join, PublishRuntime, RuntimeStream), and the broker authenticates each
  stream by that message. So streams of different commands on one connection
  stay apart at the protocol layer.
- Admission: the broker's `ConnectionLimit` counts connections, not streams
  ([server.rs:292](../../crates/seer-broker/src/server.rs)), and it releases
  the guard after the handshake
  ([server.rs:319](../../crates/seer-broker/src/server.rs)). The limits are 4
  connections in handshake and a burst of 5 handshakes per source
  ([connection_limit.rs:8](../../crates/seer-broker/src/connection_limit.rs)).
  Streams on a shared connection do not pass this limit. A reuse design must
  decide how the broker limits streams on one connection.
- Opener writes first: iroh gives a stream to the far side only after the
  opener writes ([session.rs:16](../../crates/seer-net/src/session.rs)). All
  current openers send their first message at once, so this holds.
- Failure: today one failed dial affects one command. On a shared connection,
  one connection loss ends every stream on it at the same time. The window
  already handles room loss
  ([tui.rs:115](../../crates/seer/src/tui.rs)).

### Shutdown

- `dial`: the dialer thread closes the endpoint after the bridge ends
  ([lib.rs:413](../../crates/seer-net/src/lib.rs)). The thread is detached. For
  a short command, process exit ends it, and nothing waits for a clean close.
- `dial_session`: dropping `Session` shuts down the control socket
  ([session.rs:46](../../crates/seer-net/src/session.rs)). The session thread
  then closes the connection with code 0 and closes the endpoint
  ([lib.rs:215](../../crates/seer-net/src/lib.rs)).
- `Listener`: drop writes one control byte
  ([lib.rs:127](../../crates/seer-net/src/lib.rs)). The thread waits for all
  connection tasks and then closes the endpoint
  ([lib.rs:312](../../crates/seer-net/src/lib.rs)). Drop does not join the
  thread.
- A shared endpoint must close only after its last user drops it. A check
  that the broker is gone, as in `seer stop`, must not trust a cached
  connection that is already dead.

## Measurements

### Sources

- Baseline run from issue 389: revision 367e50c, run at 2026-09-14T04:27Z, 20
  samples per workload. Method and boundaries are in
  18-transport-baseline.md. Raw samples are in perf-samples/389/.
- New run for this issue: revision d936f8f, run at 2026-09-14T04:50Z, 20
  samples per workload, 0 failures, empty errors.log. Raw samples, summary,
  and environment are in perf-samples/390/.
- Both runs: AMD Ryzen 7 7800X3D, 16 logical CPUs, Linux 7.1.6, release
  profile (lto fat, codegen-units 1), iroh 1.1.0, relay
  use1-1.relay.n0.iroh.link. One machine, so the peer is on the same host.

The baseline lacked one case: a new stream on an existing iroh connection
over the relay path. That is the path for a friend behind NAT. Commit d936f8f
adds an `iroh session` probe mode (one endpoint, one connection, a new
stream for each warm sample) and a `reuse` workload set to the driver. The
new run also repeats `iroh_dial relay` as a control. Its cold handshake
median is 193.631 ms against 193.626 ms in the baseline, so the two runs
agree.

Commands:

    flock /tmp/claude-1000/perf-run.lock \
        scripts/perf/transport.sh 20 target/perf/390-<UTC time> reuse
    awk -f scripts/perf/setup_totals.awk samples.csv > setup-totals.csv
    scripts/perf/summarize.sh setup-totals.csv

### Setup cost for each connection

`setup_total` is the sum of the setup boundaries of one sample:
runtime_start, endpoint_bind, address_lookup, dial_handshake, seer_dial,
seer_dial_session, and stream_open (which ends at the first echo). It
excludes process_spawn and the later echoes. The sum is made per sample from
the raw rows, and the median and p95 are taken over those sums. The per
sample sums are in perf-samples/390/setup-totals.csv and
perf-samples/390/baseline-389-setup-totals.csv.

Cold means a new process. Warm means a later sample in the same process.
All values are in ms.

| What repeats | Workload | Path | Run | Cold median | Warm median | Warm p95 |
| --- | --- | --- | --- | --- | --- | --- |
| New seer_net::dial each time (Seer today) | seer_dial | discovery | 389 | 290.022 | 289.868 | 421.793 |
| New connection on one shared endpoint | iroh_dial | discovery | 389 | 284.326 | 0.598 | 0.672 |
| New stream on one seer_net::Session | seer_session | discovery | 389 | 290.124 | 0.135 | 0.149 |
| New stream on one iroh connection | iroh_session | discovery | 390 | 290.517 | 0.045 | 0.056 |
| New connection on one shared endpoint | iroh_dial | relay | 390 | 289.996 | 170.359 | 170.885 |
| New stream on one iroh connection | iroh_session | relay | 390 | 292.289 | 75.644 | 75.765 |

Rows used from the baseline summary (perf-samples/389/summary.csv), in ms:

| Workload | Condition | Cache | Boundary | n | Median | p95 |
| --- | --- | --- | --- | --- | --- | --- |
| seer_dial | discovery | cold | seer_dial | 21 | 199.532 | 204.956 |
| seer_dial | discovery | cold | stream_open | 21 | 87.701 | 96.263 |
| seer_dial | discovery | warm | seer_dial | 20 | 196.545 | 205.171 |
| seer_dial | discovery | warm | stream_open | 20 | 93.062 | 223.780 |
| iroh_dial | discovery | warm | address_lookup | 20 | 0.079 | 0.090 |
| iroh_dial | discovery | warm | dial_handshake | 20 | 0.354 | 0.398 |
| iroh_dial | discovery | warm | stream_open | 20 | 0.166 | 0.217 |
| seer_session | discovery | cold | seer_dial_session | 21 | 203.994 | 204.935 |
| seer_session | discovery | warm | stream_open | 20 | 0.135 | 0.149 |

Rows from the new run (perf-samples/390/summary.csv), in ms:

| Workload | Condition | Cache | Boundary | n | Median | p95 |
| --- | --- | --- | --- | --- | --- | --- |
| iroh_dial | relay | warm | dial_handshake | 20 | 78.393 | 78.623 |
| iroh_dial | relay | warm | stream_open | 20 | 91.933 | 92.096 |
| iroh_dial | relay | warm | echo_rtt | 400 | 78.027 | 78.541 |
| iroh_session | relay | warm | stream_open | 20 | 75.644 | 75.765 |
| iroh_session | relay | warm | echo_rtt | 400 | 75.642 | 76.026 |
| iroh_session | discovery | warm | stream_open | 20 | 0.045 | 0.056 |

### N sequential connections in one process

Sum of the 20 warm setup_total values of one process:

| What repeats | Path | Run | Sum of 20, ms |
| --- | --- | --- | --- |
| New seer_net::dial each time | discovery | 389 | 6205.542 |
| New connection on one shared endpoint | discovery | 389 | 12.022 |
| New stream on one seer_net::Session | discovery | 389 | 2.638 |
| New connection on one shared endpoint | relay | 390 | 3403.561 |
| New stream on one iroh connection | relay | 390 | 1513.213 |

### What the numbers say

- A repeated standalone dial gets no benefit from the earlier one. The warm
  seer_dial median (289.868 ms) is equal to the cold median (290.022 ms)
  within the spread. Every dial pays the full setup again.
- On the direct path, endpoint reuse removes almost all of it: 0.598 ms for
  a new connection on a shared endpoint. A new stream on a shared connection
  costs 0.135 ms through seer-net and 0.045 ms in plain iroh.
- On the relay path, endpoint reuse still pays a relay handshake (78.393 ms)
  and a first stream (91.933 ms): 170.359 ms each time. Connection reuse
  pays only the first echo, 75.644 ms. That is equal to one relay echo round
  trip (75.642 ms). So on the relay path, connection reuse saves 94.715 ms
  more for each connection than endpoint reuse (difference of the two warm
  medians).
- These numbers come from one host. The discovery path finds a direct path
  on the same machine, so its warm numbers show local cost only. Between two
  machines, a new connection on a shared endpoint will add at least one
  network round trip. This pass did not measure that (lan was not
  available).

### Material or not, by caller

Material means the cost is on a path that a person waits for, and reuse can
remove it without a new long lived process.

- join (commands.rs:95 and :134) and peek (selection.rs:128 and :137):
  material. Each run makes two dials in one process. The second dial pays
  289.868 ms (median) that one shared endpoint or connection in the process
  would cut to under 1 ms on the direct path, or to 75.644 ms on the relay
  path. Each extra "name is in use" retry in join pays the same again.
  Endpoint reuse needs no broker change. Connection reuse needs the broker to
  serve a second client stream on one connection (see Broker above).
- list: material when there is more than one saved server. It pays one full
  setup per server, one after the other. The servers are different remotes,
  so only endpoint reuse applies, not connection reuse.
- attach and the room reconnect in the window: one dial per try. Reuse in
  one process saves nothing on the first try. On a later try it saves the
  setup, but the backoff between tries is 1 s to 30 s, which is larger. Not
  material.
- invite, detach, leave, perms: one dial per process. Reuse in the process
  saves nothing. Reuse across commands needs an owner that lives longer than
  one command, which is new scope. Not material for this issue.
- Runtime reconnect (room.rs:74): one dial_session per try, 203.994 ms
  median plus the first stream, and the backoff is 1 s to 30 s. Viewer
  streams already share the connection at 0.135 ms. Not material.
- Broker: one endpoint for the process, no dial. Not affected.
- Repeated setup does not explain the 3.1 s listener wait from issue 389.
  That wait is relay_online on bind, and reuse of dials does not change it.

### Not measured

- Two machines (lan or internet). All peers here are on one host.
- Relay and loopback conditions for the seer api. seer_net::dial takes only
  an endpoint ID, so it cannot force a path.
- The threads and memory that each standalone dial holds while its stream
  is open.
- Full seer join, seer peek, and seer list commands end to end.
- Cold DNS. The systemd-resolved cache was not flushed.
