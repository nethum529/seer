# Terminal stream adapter

Date: 2026-09-13
Issue: 393 (R-005). Related: 370, 386.

## Scope

This document maps the adapter that seer-net puts between an iroh QUIC
stream and the blocking code in the broker, the runtime, and the client.
It defines the behaviour that a replacement adapter must keep, and a test
boundary that a person can run to prove it.

This is phase 1. It uses code reading only. Every count in this document
comes from the source at commit 8714a51 and from dependency source in
Cargo.lock (iroh 1.1.0, noq 1.2.0, noq-proto 1.2.0, tokio 1.53.1). No number
here is a measurement. Phase 2 measures the cost and keeps raw samples under
docs/research/perf-samples/393/.

## The adapter today

Each iroh stream is bridged to a Unix socket pair. The caller gets one end
as a std UnixStream. A tokio task pair copies bytes between the other end and
the QUIC stream.

| Part | File and line | What it does |
| --- | --- | --- |
| Socket pair | crates/seer-net/src/lib.rs:423-428 | stream_pair makes a std UnixStream pair and gives one end to tokio |
| Bridge | crates/seer-net/src/lib.rs:430-449 | Spawns two copy tasks, waits for the first to end, aborts the other |
| QUIC to Unix | crates/seer-net/src/lib.rs:451-457 | tokio::io::copy, then shutdown of the Unix write half |
| Unix to QUIC | crates/seer-net/src/lib.rs:459-469 | tokio::io::copy, then QUIC finish, then waits for stopped |
| Runtime per endpoint | crates/seer-net/src/lib.rs:416-421 | new_multi_thread with default worker count |
| Listener first stream | crates/seer-net/src/lib.rs:343-356 | Bridge task for the first stream, then serves the session |
| Dialer stream | crates/seer-net/src/lib.rs:394-413 | Bridge for the one dial stream, endpoint closes when it ends |
| Session open | crates/seer-net/src/session.rs:101-119 | open_bi, socket pair, bridge task in a JoinSet |
| Session accept | crates/seer-net/src/session.rs:121-133 | accept_bi, socket pair, bridge task in a JoinSet |
| Socket type | crates/seer-net/src/stream.rs:7-11 | Socket::Iroh holds Arc of UnixStream |
| Stream trait | crates/seer-net/src/stream.rs:50-55, 75-91 | Blocking read and write, timeouts, shutdown; set_nodelay is a no-op on Unix |

Other socket pairs in seer-net carry only a stop signal, not terminal data:
the listener control pair (lib.rs:84) and the session control pair
(lib.rs:148, session.rs:46-52).

The runtime has a second adapter of the same shape for TCP rooms. The host
runtime reaches its own broker over loopback TCP, because seer start saves
the TCP published_addr as the host endpoint (crates/seer/src/start.rs:458,
crates/seer/src/local.rs:145). The runtime then copies that TCP stream onto
a Unix socket pair with two std threads (crates/seer-runtime/src/room.rs:162-184).

### Work per stream, from code

Per side of one iroh stream:

- One Unix socket pair, which is two file descriptors.
- Three tokio tasks: the bridge select and two copy tasks.
- Two 8 KiB copy buffers (tokio DEFAULT_BUF_SIZE, tokio io/util/mod.rs:88).

Per process: one multi-thread tokio runtime per endpoint. Its default worker
count is the CPU count. This machine has 16 CPUs. The broker has one
endpoint. Each runtime has one session endpoint. The client has one endpoint
per dial.

Per byte, in one direction, on one side, the adapter adds two copies and two
system calls compared to a caller that reads and writes the QUIC stream
directly:

- Send side. Direct: frame to noq send buffer. Adapter: frame to kernel
  (write), kernel to 8 KiB buffer (read), buffer to noq send buffer.
- Receive side. Direct: noq receive buffer to frame. Adapter: noq to 8 KiB
  buffer, buffer to kernel (write), kernel to frame (read).

Each chunk also costs one epoll wake of a tokio worker and one task wake.
The copy tasks can run on any worker, so a chunk can move between threads.
Phase 2 measures these costs. This section only counts them.

### Adapter crossings per terminal path

A crossing is one pass through one bridge in one direction. A keystroke
round trip is input one way and the echo back the other way, so it crosses
each bridge twice.

| Path | Hops | Bridges in one direction |
| --- | --- | --- |
| A person and their own terminals | client to local runtime socket | 0 |
| Guest views or types into a host terminal | host runtime, loopback TCP, broker, iroh, guest client | 2 async bridges and 1 thread bridge |
| Host views or types into a guest terminal | guest runtime, iroh session, broker, loopback TCP, host client | 2 async bridges |
| Guest views or types into another guest terminal | runtime, iroh session, broker, iroh, client | 4 async bridges |

The broker decodes and encodes every frame between its two streams
(crates/seer-broker/src/forwarding.rs). That work is not part of the adapter.

## Callers that need a concrete Unix socket

The runtime serves every connection as a std UnixStream. This is the only
hard dependency on the concrete type. All other callers use the Socket enum
and the Stream trait.

| Caller | File and line | Dependency |
| --- | --- | --- |
| Room serve callback | crates/seer-runtime/src/room.rs:45, 53, 72, 102, 119 | serve takes a UnixStream |
| Room to Unix | crates/seer-runtime/src/room.rs:162-165 | try_clone to move the UnixStream out of the Arc |
| TCP room to Unix | crates/seer-runtime/src/room.rs:166-183 | New socket pair and two copy threads |
| Connection spawn | crates/seer-runtime/src/server/room_link.rs:28-43 | Shared by room streams and the local UnixListener (server.rs:51-52) |
| Connection handler | crates/seer-runtime/src/server/connection.rs:16-68 | try_clone for each attach kind, shutdown Both at the end |
| Connection state | crates/seer-runtime/src/server/connection.rs:112, 124-126, 317-321 | Keeps the stream, try_clone for the writer, shutdown Both on drop |
| Output writer | crates/seer-runtime/src/server/writer.rs:10-31 | 2 s write timeout, evicts on error, shutdown Both |
| Query handlers | crates/seer-runtime/src/server/connection.rs:74, 352, 402; status.rs:13; size_lease.rs:136 | Take a mutable UnixStream, write only |

No caller passes a file descriptor to another process. No caller uses
SCM_RIGHTS, raw descriptors, poll, or non-blocking mode on a terminal stream.
Rust code uses as_raw_fd only for flock (crates/seer/src/local.rs:55) and
the PTY (crates/seer-runtime/src/pty.rs:66).

Socket::Iroh is not only for iroh. Local Unix streams also use it:
crates/seer/src/local.rs:76 and crates/seer/src/commands/exit.rs:20. A
replacement must keep a Unix variant for local sockets.

### Callers through Socket and Stream

| Caller | File and line | Uses |
| --- | --- | --- |
| Broker iroh accept | crates/seer-broker/src/server.rs:262-278 | Listener::accept to Socket |
| Broker handshake | crates/seer-broker/src/server.rs:455-475 | Read timeout set before each read (5 s deadline); silent kinds are TimedOut, WouldBlock, UnexpectedEof |
| Broker status query | crates/seer-broker/src/server.rs:163-172 | 500 ms read timeout, shutdown Both |
| Connection limit | crates/seer-broker/src/server.rs:291-295 | shutdown Both before any read |
| Runtime control | crates/seer-broker/src/runtime.rs:23-27, 77-82 | 2 s write timeout, shutdown Both from another thread |
| Runtime stream open | crates/seer-broker/src/runtime.rs:91-118 | Socket moves between threads through a channel |
| Session stream pump | crates/seer-broker/src/publishing.rs:76-100 | Blocking read with no timeout until close |
| Runtime reader | crates/seer-broker/src/forwarding/transport.rs:26-45, 77-81 | 5 s read and 2 s write timeouts, then no read timeout; clone to a reader thread; shutdown Both, then join |
| Client coordinator | crates/seer-broker/src/forwarding.rs:43-50, 409-425 | 2 s write timeout, clone to a reader thread; shutdown Both, then join |
| Detach | crates/seer-broker/src/attachments.rs:103-111 | Another thread writes Bye and shuts the stream down |
| Client connect | crates/seer/src/commands.rs:335-360, 410-420 | 5 s timeouts, per-read deadline |
| Client room | crates/seer/src/tui_link.rs:25-31, 37-54 | No read timeout, 2 s write timeout, clone to a reader thread |
| Client exit | crates/seer/src/tui.rs:69-75; routes.rs:84-90 | shutdown Both, then join the reader thread |
| Setup errors | crates/seer/src/terminal_session.rs:92-99 | BrokenPipe, ConnectionAborted, ConnectionReset mean the peer left |

## Required behaviour

A replacement adapter must keep these contracts. Each one comes from a
caller above.

### Blocking IO

- R1. Read blocks until at least one byte, end of stream, an error, or the
  read timeout. Write blocks until all bytes are queued, an error, or the
  write timeout.
- R2. Many threads can hold the same stream at the same time. One thread
  reads while another writes. Socket is Clone through an Arc. The runtime
  uses try_clone.
- R3. A stream can move to another thread (Send), and a handle can be
  dropped on any thread.

### Shared timeout

- T1. A read or write timeout belongs to the stream, not to one handle. A
  value set through one handle applies to every clone. forwarding/transport.rs
  sets the timeout, then gives a clone to the reader thread.
- T2. A new value applies from the next blocking call. The handshake
  readers change the read timeout before every read.
- T3. A timeout returns WouldBlock or TimedOut. The broker handshake counts
  both as a silent connection. The stream stays usable after a read timeout.
- T4. None means no timeout. The room links and the runtime reader depend
  on a read that waits without limit.

### Cross-handle shutdown

- S1. shutdown Both through any handle wakes every blocked read and write on
  every clone. A read returns end of stream or an error. A write returns an
  error. The broker and the client call shutdown, then join the reader
  thread. If the wake does not happen, the join waits forever.
- S2. shutdown works while other handles are still open. Close alone does
  not end the stream, because clones keep the descriptor open.
- S3. When the peer ends the stream, a blocked read returns end of stream,
  or an error for a partial frame. A write returns BrokenPipe,
  ConnectionReset, or ConnectionAborted.
- S4. Half-close is not required. No caller shuts down only one direction
  of an iroh stream. The current bridge does not keep half-close either.
  When the caller shuts its write side, copy_to_quic finishes the QUIC
  stream and waits for stopped. stopped resolves when the peer acknowledges
  the data (noq-1.2.0 send_stream.rs:243-255). The bridge then aborts the
  other copy task, so the return direction also ends.

### Bounded buffering

- B1. Bytes that one writer can queue ahead of the peer reader must have a
  fixed upper bound. The runtime writer (writer.rs) and the broker client
  writer use a 2 s write timeout to evict a slow viewer. The timeout starts
  to count only when every buffer after the writer is full.
- B2. A full buffer blocks the writer. It must not drop bytes and it must
  not grow without limit.

Today the bound for one iroh hop, one direction, is the sum of:

| Buffer | Size source |
| --- | --- |
| Sender Unix pair | Linux: SO_SNDBUF default net.core.wmem_default, 212992 on this machine (sysctl). macOS: net.local.stream.sendspace, not checked here |
| Sender copy buffer | 8 KiB, tokio DEFAULT_BUF_SIZE |
| QUIC stream window | noq stream_receive_window default 1,250,000 bytes (noq-proto-1.2.0 config/transport.rs:550-563). iroh does not change it (iroh-1.1.0 endpoint/quic.rs:152-162) |
| Receiver copy buffer | 8 KiB |
| Receiver Unix pair | Same as the sender Unix pair |

The payload that fits in a Unix pair is less than SO_SNDBUF, because the
kernel counts buffer overhead. Phase 2 measures it. The connection send
window is 10,000,000 bytes (8 times the stream window) and is shared by all
streams of one connection.

The macOS Unix pair is much smaller than the Linux one if the macOS default
is in effect. Then the same 2 s eviction starts at a different queue depth
on macOS. This is not verified on this machine.

### Findings from code reading

These are gaps in the current adapter. Phase 2 must confirm or reject each
one with a test run. They are not fixed here.

- F1. The caller's timeouts do not reach the QUIC side. After the runtime
  writer evicts a viewer and shuts down its end, a copy task that waits on
  QUIC flow control does not see the shutdown. It sees it on its next read
  of the Unix pair. The QUIC stream and its tasks can stay until the peer
  reads, sends, or the connection closes.
- F2. The TCP room copy threads (room.rs:166-183) have no timeouts on the
  TCP side. A stuck broker blocks the copy thread until the TCP connection
  fails.
- F3. codec::decode reads the 4 byte length and the body with two reads
  (seer-core proto/codec.rs:38-45). Through the adapter each read is a
  system call on the Unix pair. A replacement that hands each read across
  threads pays one cross-thread wake per read unless it buffers.
- F4. Each endpoint starts a runtime with one worker per CPU. On this
  machine that is 16 workers per endpoint, for a stream load of a few
  terminals.

## Compatibility boundary

The boundary is the public surface of seer-net: Listener, dial,
dial_session, Session, Socket, Stream, SecretKey, EndpointId. Issue 386 uses
the same surface. A replacement adapter can change anything behind it. It
must keep R1 to R3, T1 to T4, S1 to S3, and B1 to B2.

A person proves a replacement is compatible with these steps. Run each step
on the old build and on the new build. Record the result of each.

### Automatic checks

1. cargo test -p seer-net --test round_trip. Needs network access.
2. cargo test -p seer-broker --test room_iroh. Linux, needs network access.
3. cargo test --workspace --no-fail-fast.

### Manual checks

Use two computers, or two users on one Linux computer with a different HOME
for each. Host: seer start. Guest: paste the seer join line.

1. Input. The host gives the guest input on a host terminal that runs cat.
   The guest types a line. The line shows on both screens.
2. Flood. The host runs seq 1 3000000 in a terminal that the guest views.
   The guest view keeps updating. Ctrl-C in the host terminal stops the
   output and the prompt comes back.
3. Slow viewer. The guest suspends the guest client with Ctrl-Z while the
   host runs step 2. The host terminal stays responsive. The host
   runtime.log shows "runtime evicted slow connection". Record how many
   seconds after the flood starts it shows. Then the guest resumes with fg
   and records what the guest client shows.
4. Clean leave. The guest quits Seer. Record how many seconds until the
   host sees that the guest left.
5. Hard leave. Kill the guest client with kill -9. Record how many seconds
   until the host sees that the guest left. iroh path idle timeout is 15 s
   direct and 30 s through a relay (iroh-1.1.0 socket.rs:117, 129), so this
   depends on iroh, not on the adapter.
6. Guest to guest. With a second guest, repeat steps 1 and 2 from one
   guest to the other. This path crosses four bridges.
7. Threads. Run ps -T -p PID | wc -l for the broker, each runtime, and each
   client. A replacement must not add threads.

The old build and the new build must give the same result in steps 1, 2, 4,
and 6. In steps 3 and 5 the eviction and leave times of the new build must
not be longer. In step 7 the thread counts of the new build must not be
higher.

## Open questions for phase 2

- Real payload capacity of a Unix socket pair on this machine.
- Added latency of one bridge crossing and of a full keystroke round trip,
  compared to a direct stream.
- Wakes and thread moves per terminal frame.
- Whether F1 leaves QUIC streams open after an eviction.
