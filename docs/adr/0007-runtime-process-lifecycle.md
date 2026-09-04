# ADR 0007: The broker owns the runtime process lifecycle

Date: 2026-09-04
Status: Accepted

## Context

The process model is settled: one broker plus one runtime for each
person. The broker owns identity, routing, supervision, and metadata.
It never owns a PTY or an agent child process. Every PTY lives in a
runtime. This ADR records how that split works in the code: who starts,
watches and stops a runtime, how a runtime learns that its broker
died, and who removes the socket file at each exit path.

## Decision

### The broker starts a runtime on demand

A runtime starts only when the broker needs one. connect in
crates/seer-broker/src/runtime.rs first tries the runtime socket of
that person. When the connect works, the runtime is alive and nothing
is started. When it fails, the broker prepares the state directory
and the socket directory for the mapped OS account, then spawns a
runtime only when no live process is recorded for that person. See
spawn and connect_with_retries.

The broker passes the socket path, the user id, and the shell as
arguments, and the snapshot directory as an environment variable.
After the spawn, connect_with_retry tries up to 500 times with a 10
millisecond pause, so the broker waits about 5 seconds for the
runtime to bind its socket.

### The broker supervises

RuntimeManager::new starts one supervisor thread. Every 250
milliseconds it calls try_wait on each child. A child that exited is
dropped from the map and its status is logged. See spawn_supervisor,
supervise_runtimes, and runtime_is_running.

The supervisor holds a weak reference to the process map, so it
returns when the broker drops the manager. It does not restart a
runtime. The next connect starts a new one.

### The runtime learns that the broker died

The broker gives the runtime the read end of a pipe as stdin and
keeps the write end alive in the RuntimeProcess record. The runtime
reads stdin forever in a thread. A read of zero bytes or an error
means the write end is gone, so the broker is gone, and the runtime
calls exit(0). See the _lifeline field in
crates/seer-broker/src/runtime.rs and watch_lifeline in
crates/seer-runtime/src/lib.rs. The runtime never polls the broker.

### Socket file ownership and cleanup

The runtime owns its socket file. Before it binds the path given by
the broker, remove_stale_socket tries to connect to that path. A
successful connect means a live runtime holds it, and the bind fails
with AddrInUse. A failed connect means the file is stale, so the
runtime removes it and binds. See bind in
crates/seer-runtime/src/server.rs.

Issue 264 recorded the problem: 137 stale socket files were left
under the runtime directory after a day of test brokers, while only
two runtimes were alive. A runtime removed only its own path, and
only on the next start. The fix (PR 270) makes the runtime unlink its
own socket when it exits. install_sigterm_cleanup in
crates/seer-runtime/src/lib.rs stores the path as a leaked C string
and installs a SIGTERM handler. The handler calls unlink and then
_exit. Both calls are async signal safe.

The exit paths are:

- seer stop, or a broker stop: SIGTERM reaches the whole process
  group, and the runtime handler unlinks its own socket.
- The runtime exits for another reason: the supervisor sees the dead
  child and calls remove_runtime_socket, which removes the file only
  when a connect to it fails.
- A crash with no signal handling: the file stays, and the next
  runtime for that person removes it before it binds.

### Stopping the group

seer start puts the broker in a new session and process group with
setsid before exec (crates/seer/src/start.rs). seer stop reads the
recorded pid and signals the whole group with a negative pid: first
SIGTERM, then a wait, then SIGKILL for anything still alive. ESRCH is
treated as success. See stop_process_group and signal_process_group
in crates/seer/src/start/stop.rs. The group covers the broker and
every runtime it started, so one stop ends all of them.

### The size owner rule for attach connections

A runtime connection that is not read only can be the size owner. The
size owner viewport sets the PTY size and names the active tab, which
the status reply reports as the foreground process. The first such
connection becomes the size owner. When the owner is stale for longer
than the lease timeout, or when it goes away, the next connection
that is not read only takes the role. See
crates/seer-runtime/src/server/connection.rs.

A connection that never reports a viewport must not become the size
owner. Issue 263 recorded the failure: a person who joined and stayed
attached from the join showed a dash in the foreground column of the
drawer, while the idle time updated. The broker attached the join
connection to the runtime, it became the size owner with no viewport,
and the status reply had no active tab. The fix (PR 265) makes the
broker close the join connection after it sends Joined. The client
opens a normal attach connection after the join.

## Consequences

- A runtime that is not needed is never started. The first connect
  pays the spawn cost, up to about 5 seconds.
- A runtime cannot outlive its broker.
- The runtime, not the broker, is the normal owner of socket cleanup.
  The broker path stays as a second line for a runtime that died
  without the handler. A SIGKILL still leaves a socket file, which
  the next start for that person removes.
- A control connection that does not drive a screen must close, or
  never claim the size owner role.
