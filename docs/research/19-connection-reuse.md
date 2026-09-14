# Repeated cold connection setup with iroh

Research date: 2026-09-13. Issue 390 (R-002). Code state: origin/main at
8714a51, iroh 1.1.0, tokio 1.53.1.

## Scope

This note maps every place where Seer makes an iroh connection, and records
what a safe connection reuse must keep. It does not implement reuse. The
measurement section waits for the baseline harness from issue 389.

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

Pending. This section waits for the baseline harness from issue 389. It will
hold N sequential standalone dials against one shared endpoint, at least 10
samples each, on a release build, with raw samples under
docs/research/perf-samples/390/.
