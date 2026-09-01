# Session model and lifecycle in Herdr and Luvus

## Decision

Herdr and Luvus use the same basic session model. A named session is an
independent server namespace. It is not an object inside one server. Each
namespace has one detached server process, one shared application tree, one set
of PTYs, its own sockets, and its own persistence files. (`H:src/session.rs:157-225`,
`H:src/server/headless.rs:286-343`, `L:src/session.rs:1-5`,
`L:src/session.rs:163-288`, `L:src/ipc/server.rs:248-388`)

Neither project has a separate create command. The first attach to a missing
name selects that namespace, starts its server, creates its directory, and then
connects the client. A later attach connects to the existing server. A client
detach or connection loss removes only that client. It does not stop the server
or its PTYs. (`H:src/session.rs:29-93`, `H:src/server/autodetect.rs:280-305`,
`H:src/ipc.rs:81-107`, `L:src/session.rs:40-117`, `L:src/main.rs:498-506`,
`L:src/main.rs:509-720`)

A server restart is not the same as reattachment. Reattachment keeps the
original processes. A cold restart reads a snapshot and creates replacement
PTYs and child processes. It cannot restore arbitrary process memory, file
descriptors, or an old kernel PTY. Herdr can avoid this boundary only through a
successful Unix live handoff. (`H:src/persist/restore.rs:64-118`,
`H:src/persist/restore.rs:575-665`, `L:src/app/mod.rs:2332-2342`,
`L:src/app/mod.rs:2516-2619`)

For this project, one authenticated user's live session maps to one per-user
runtime. The broker must own user identity, routing, runtime supervision, and
durable lifecycle metadata. A clone's named-session directory scan, special
`default` name, and last-input-wins client policy do not map to the broker.

The source references below use these prefixes:

- `H:` is `/home/nethum/Projects/_research/herdr` at commit
  `2290257acb2085ce6842ba5c7e3ca50c3ba64f02`.
- `L:` is `/home/nethum/Projects/_research/luvus` at commit
  `d1013d16f48cdd724b8df40c7c4c83dc306dc5d6`.
- `P:` is this repository.

## Exact session terms and objects

The word `session` has four different meanings in both clones. These meanings
must not be combined in the new protocol.

| Meaning | Herdr object | Luvus object | Lifecycle meaning |
| --- | --- | --- | --- |
| Selected server namespace | `HERDR_SESSION`, `active_name()`, path helpers, and discovery-only `SessionInfo` | `LUVUS_SESSION`, `active_name()`, path helpers, and discovery-only `SessionInfo` plus `SessionEndpoint` | Selects one process-wide directory and its two sockets before command routing. `default` normalizes to no explicit name. (`H:src/session.rs:10-27`, `H:src/session.rs:29-100`, `H:src/session.rs:157-225`, `L:src/session.rs:12-35`, `L:src/session.rs:40-160`, `L:src/session.rs:163-288`) |
| Live multiplexer session | One `HeadlessServer` owns one `App`; its `AppState` owns workspaces, tabs, terminals, focus, and UI state | One server loop owns one `App`; `App` owns panes, workspaces, active workspace, and UI state | Exists only while the selected server process lives. It owns the live PTYs and child processes. (`H:src/server/headless.rs:286-343`, `H:src/app/state.rs:1436-1501`, `L:src/ipc/server.rs:248-388`, `L:src/app/mod.rs:1469-1503`) |
| Durable session image | `SessionSnapshot` and optional `SessionHistorySnapshot` | `SessionSnapshot`, with a separate `OrchState` file | Rebuild input for a later server. It is not a process checkpoint. (`H:src/persist/snapshot.rs:11-142`, `L:src/persist.rs:17-104`, `L:src/orch/mod.rs:1-9`) |
| Native agent conversation | `PaneAgentSessionSnapshot` contains source, agent, reference kind, and value | `PaneSnap.agent_session` contains agent and session ID | Optional agent-specific resume input. It does not identify the multiplexer namespace or a human login. (`H:src/persist/snapshot.rs:97-118`, `L:src/persist.rs:72-90`) |

`SessionInfo` is not an authoritative stored record in either clone. Both
projects build it when listing. Its `running` field is the result of a current
socket probe. Its path fields are derived from the name. (`H:src/session.rs:187-225`,
`L:src/session.rs:245-288`)

This distinction matters for the target design. A durable user session needs a
stable ID and lifecycle record. A display name, a socket path, a live process,
a saved image, and an agent conversation reference are different values.

## Common lifecycle

The effective clone lifecycle is:

```text
unknown name
    |
    | first attach or explicit server start
    v
running server namespace <---- later client attach
    |        ^                     |
    |        | client detach       | client disconnect
    |        +---------------------+
    |
    | stop, crash, reboot, or failed process
    v
stopped directory and optional snapshot
    |
    | attach: start server and cold restore
    v
running replacement server
    |
    | delete, named sessions only
    v
absent directory
```

The clones do not store these as explicit states. `running` is inferred from
socket reachability. `stopped` means that a valid session directory can be
listed but its endpoints do not accept a connection. An unknown name and an
already deleted name both have no directory. (`H:src/session.rs:187-225`,
`L:src/session.rs:245-288`)

## Herdr lifecycle

### Create and name

Herdr accepts `--session <name>`, `--session=<name>`,
`herdr session attach <name>`, or inherited `HERDR_SESSION`. An explicit named
selector takes priority over socket overrides. The special name `default` maps
to the config root and is not a directory named `sessions/default`.
(`H:src/session.rs:29-100`, `H:src/session.rs:157-185`,
`H:src/server/socket_paths.rs:14-48`)

A name is 1 to 64 bytes. It permits ASCII letters, digits, `.`, `_`, and `-`.
The values `.` and `..` are invalid. This validation makes the name safe as one
path component. (`H:src/session.rs:399-465`)

There is no `session create`. `session attach` applies the name and removes the
subcommand from the argument list. Normal auto-detection then probes the client
socket. It starts a detached `herdr server` when the socket is not live. Socket
preparation creates the parent directory. (`H:src/session.rs:29-52`,
`H:src/server/autodetect.rs:179-235`, `H:src/server/autodetect.rs:280-305`,
`H:src/ipc.rs:81-107`)

Server startup loads a snapshot before it seeds a new workspace. It then binds
one API socket and one display-client socket for that namespace. A second live
server is rejected. (`H:src/app/mod.rs:410-490`,
`H:src/server/headless.rs:5067-5139`)

### List

`herdr session list` always adds `default`. It then scans
`<config-dir>/sessions`, keeps only valid directory names, sorts them, derives a
`SessionInfo` for each, and probes its API socket. Listing does not start a
server and does not read `session.json`. A valid directory is enough for a
stopped entry to remain visible. (`H:src/session.rs:187-225`,
`H:src/cli.rs:451-468`)

This is directory discovery, not a session registry. An empty directory or a
directory left after a snapshot was cleared can still be a listed session.

### Attach and detach

An attach has no persistent attachment identity. The accept loop allocates a
new process-local numeric client ID for each connection. The new connection
performs the normal handshake and joins the server's client map.
(`H:src/server/client_accept.rs:11-53`,
`H:src/server/client_transport.rs:515-689`)

An explicit detach sends `ClientMessage::Detach`. EOF, a read error, and a
writer failure use the same removal result. Only the connection record and its
per-client render resources are removed. The server event loop has no rule that
stops it when the client map becomes empty. (`H:src/protocol/wire.rs:364-391`,
`H:src/server/client_transport.rs:706-775`,
`H:src/server/headless.rs:2974-2990`, `H:src/server/headless.rs:3310-3319`)

The live workspace tree, terminal grids, PTYs, child processes, and output that
arrives while no client is attached remain in the server. Herdr has integration
tests for explicit detach, connection drop, live process survival, and output
that appears after reattach. (`H:tests/detach_reattach.rs:283-395`,
`H:tests/detach_reattach.rs:405-607`, `H:tests/detach_reattach.rs:613-665`,
`H:tests/detach_reattach.rs:835-957`)

### Stop, restart, and delete

`herdr session stop <name>` sends `server.stop` to the namespace API socket and
waits up to 15 seconds for both sockets to stop accepting connections. A stop
of an absent or unreachable session is an error. Normal server exit performs a
final save, tells attached clients that the server stopped, drains connection
writers, and removes owned socket files. (`H:src/session.rs:232-297`,
`H:src/server/headless.rs:4837-4911`)

Stop is not delete. The session directory and snapshot remain. The next attach
starts a new server and uses cold restore. Herdr has no general
`session restart` command. The ordinary sequence is stop and then attach. Live
handoff is a separate Unix update path. (`H:src/session.rs:103-145`,
`H:src/server/handoff.rs:1-80`, `H:src/persist/restore.rs:94-118`)

`herdr session delete <name>` rejects `default` and a running named session. It
removes the complete named directory. Missing directories are treated as a
successful delete. (`H:src/session.rs:299-317`, `H:src/cli.rs:503-527`)

If all workspaces are closed, Herdr removes `session.json` and
`session-history.json`, but it does not delete the named namespace directory.
The namespace can therefore remain in `session list` with no restore image.
(`H:src/app/session.rs:39-58`, `H:src/persist/io.rs:96-111`)

## Luvus lifecycle

### Create and name

Luvus accepts the same main selectors through `LUVUS_SESSION`,
`--session <name>`, and `luvus session attach <name>`. It also reads the legacy
`BOHAY_SESSION`. Selection stops before subcommand arguments so a native agent
`--session` value cannot select a server namespace. `default` maps to the Luvus
root. (`L:src/session.rs:12-18`, `L:src/session.rs:40-160`)

Luvus uses the same 1 to 64 byte name rules as Herdr. A named directory is
`<LUVUS_HOME>/sessions/<name>`. Long Unix socket paths can use stable,
owner-scoped aliases below `/tmp` or `/private/tmp`; the durable files remain in
the logical session directory. (`L:src/session.rs:154-243`)

There is no `session create`. `session attach` becomes the default launch path.
That path checks the display endpoint, checks the API endpoint when recovery is
needed, starts a detached server if absent, opens the launch directory as a
workspace, and connects the client. (`L:src/session.rs:40-66`,
`L:src/main.rs:498-506`, `L:src/main.rs:509-720`)

Luvus creates the selected directory with owner-only mode before server start.
It takes an advisory `server.lock` before it checks and binds both endpoints.
The lock prevents two first attaches from restoring duplicate PTYs. The lock
file remains after exit, but the OS releases its lock. (`L:src/persist.rs:506-566`,
`L:src/ipc/transport.rs:25-52`, `L:src/ipc/server.rs:248-346`)

### List

`luvus session list` also synthesizes `default`, scans valid named
subdirectories, sorts them, and builds current `SessionInfo` values. It reports
the namespace as running if either its API endpoint or display endpoint accepts
a connection. `session list --json` is routed before migration and other setup
work, so discovery does not start a server. (`L:src/session.rs:245-288`,
`L:src/main.rs:86-101`, `L:src/main.rs:126-131`, `L:src/cli.rs:936-992`)

Luvus adds `SessionEndpoint` with a transport and address. This is still a
derived discovery result, not a durable catalog row. (`L:src/session.rs:20-35`,
`L:src/session.rs:263-273`)

### Attach, switch, and detach

Each accepted display connection gets a new numeric ID and a `ClientState`.
Explicit `Detach` and every client read error send `ClientDetach` to the app
loop. Removing the foreground client selects the most recently active remaining
client. Removing the last client leaves the client map empty but does not end
the server loop. (`L:src/ipc/protocol.rs:19-36`, `L:src/event.rs:61-83`,
`L:src/ipc/server.rs:638-744`, `L:src/ipc/server.rs:1148-1165`)

Luvus also has an in-UI named-session switch. The source server removes only
the foreground client and sends it `SwitchSession { name }`. On Unix, the client
replaces its own process with the same launch mode and the new selector. Other
clients stay attached to the source namespace, and the source server continues
to run. (`L:src/ipc/server.rs:502-523`, `L:src/ipc/protocol.rs:61-71`,
`L:src/ipc/client.rs:248-285`)

Closing the final project workspace also does not stop a server session. In
server mode Luvus creates a neutral terminal rooted at the user's home. Only a
server stop ends the runtime. (`L:src/app/mod.rs:1589-1597`)

### Stop, restart, and delete

`luvus session stop <name>` writes a `server.stop` request, closes the request
connection, and probes both endpoints for up to five seconds. It does not wait
without a bound for an acknowledgement from a stuck event loop.
(`L:src/session.rs:290-327`)

The general `luvus server stop` path has stronger recovery behavior. It can use
`server.pid` to stop an unresponsive process after it verifies that the PID is
an owned Luvus process. A clean exit removes the PID file. A crash can leave it,
so the start marker prevents PID reuse from targeting another process.
(`L:src/persist.rs:443-477`, `L:src/main.rs:981-1101`)

`luvus server restart` stops the selected namespace, waits for socket release,
and starts a replacement. The old server makes a final save before exit. The
new server loads that image. (`L:src/main.rs:865-918`,
`L:src/ipc/server.rs:465-485`, `L:src/ipc/server.rs:624-625`)

`luvus session delete <name>` rejects `default`, unsafe names, and running
sessions. It removes deterministic external socket aliases and then the named
directory. A missing directory is a successful delete. (`L:src/session.rs:329-361`)

As in Herdr, an empty snapshot can be cleared while the named directory stays.
The persistent lock file makes a previously started Luvus namespace especially
likely to remain visible as a stopped directory. (`L:src/persist.rs:986-1004`,
`L:src/ipc/transport.rs:25-52`)

## What survives

Research 06 and 07 contain the complete field-level persistence matrices. This
section gives the lifecycle boundary without repeating those matrices.
(`P:docs/research/06-herdr-server-and-persistence.md`,
`P:docs/research/07-luvus-server-and-persistence.md`)

| Event | Herdr result | Luvus result |
| --- | --- | --- |
| One client detaches | The live server, all other clients, workspace tree, terminal grids, scrollback, PTYs, child processes, and in-memory UI state remain. Only that connection's state is lost. (`H:src/server/headless.rs:1602-1645`, `H:src/server/headless.rs:3310-3319`) | The same live objects remain. Only its `ClientState`, frame baseline, dimensions, activity stamp, and connection ID are lost. (`L:src/ipc/server.rs:207-238`, `L:src/ipc/server.rs:674-688`) |
| Client crashes or network/SSH path closes | Same as explicit detach after EOF or transport error. (`H:src/server/client_transport.rs:706-775`) | Same as explicit detach because a read error produces `ClientDetach`. (`L:src/ipc/server.rs:1148-1165`) |
| Last client leaves | Server and live processes remain. Work continues with no render target. (`H:src/server/headless.rs:4432-4491`, `H:tests/detach_reattach.rs:613-665`) | Server and live processes remain. Rendering is skipped while the client map is empty. (`L:src/ipc/server.rs:409-472`, `L:src/ipc/server.rs:597-625`) |
| Graceful server stop | A final structural save runs. All old PTYs and child processes end. (`H:src/server/headless.rs:917-923`, `H:src/server/headless.rs:4837-4911`) | A final snapshot save runs. Pane drop ends old PTYs and children. (`L:src/ipc/server.rs:465-485`, `L:src/ipc/server.rs:624-625`, `L:src/terminal/pty.rs:198-231`) |
| Abrupt server crash | Live processes and unsaved in-memory changes are lost. The next start uses the last completed snapshot and reclaims stale sockets. (`H:src/ipc.rs:81-115`, `H:src/persist/io.rs:44-60`, `H:src/persist/restore.rs:575-665`) | Live processes and unsaved changes are lost. The startup lock is released by the OS. The next start uses the last readable snapshot and can reclaim stale sockets. (`L:src/ipc/transport.rs:25-86`, `L:src/persist.rs:1047-1054`, `L:src/app/mod.rs:2332-2342`) |
| Machine reboot | Same cold-restore boundary as server death. (`H:src/persist/restore.rs:575-665`) | Same cold-restore boundary as server death. (`L:src/app/mod.rs:2332-2342`) |
| Cold start from snapshot | Structure and saved metadata return. New shells or supported agent resume commands replace old processes. Optional history returns only when enabled. (`H:src/app/mod.rs:410-483`, `H:src/persist/restore.rs:469-665`) | Structure, saved metadata, and a bounded visible ANSI image return. New shells, module processes, or supported agent resume commands replace old processes. (`L:src/persist.rs:17-104`, `L:src/app/mod.rs:2332-2662`) |
| Successful live server replacement | Unix live handoff can transfer live PTY file descriptors and preserve processes. Clients disconnect and reconnect. In-flight requests and other transient coordination can be lost. (`H:src/server/headless.rs:1234-1409`, `H:src/persist/restore.rs:94-118`) | No equivalent live PTY handoff was found. A restart is cold restore. |
| Delete | The named directory and its restore data are removed. No later attach can restore it. (`H:src/session.rs:299-317`) | The named directory, restore data, ledger, lifecycle files, and external aliases are removed. (`L:src/session.rs:329-361`) |

The important contract is exact: sessions survive disconnect as live processes,
but survive server restart as reconstructed durable state. A product statement
that says only "persistent session" is not sufficient.

## On-disk state and namespace identity

### Herdr

The default namespace stores state in the Herdr config directory. A named
namespace stores state in `<config-dir>/sessions/<name>`. Its main files are:

| File | Role |
| --- | --- |
| `herdr.sock` | Newline-delimited JSON API endpoint. Runtime only. |
| `herdr-client.sock` | Private display-client endpoint. Runtime only. |
| `session.json` | Version 3 structural snapshot. |
| `session-history.json` | Optional version 3 pane ANSI history. Off by default. |
| `herdr-server.log` | Server log for that selected data directory. |

The paths derive from the selected namespace. Snapshot writes use a sibling
temporary file and rename. There is no durable catalog that records creation,
stop time, owner, or deletion. (`H:src/session.rs:157-185`,
`H:src/persist/io.rs:10-16`, `H:src/persist/io.rs:44-75`,
`H:src/server/headless.rs:5289-5298`)

### Luvus

The default namespace stores state in `LUVUS_HOME`. A named namespace uses
`<LUVUS_HOME>/sessions/<name>`. Its main files are:

| File | Role |
| --- | --- |
| `luvus.sock` | UHP and JSON API endpoint. Runtime only. |
| `luvus-client.sock` | Private display-client endpoint. Runtime only. |
| `server.lock` | Persistent pathname with a process-scoped advisory startup lock. |
| `server.pid` | Live process evidence. Removed after clean exit. |
| `session.json` | Version 1 structural and visible-screen snapshot. |
| `orch.json` | Separate task and path-lease ledger. |

Luvus creates the session directory as mode `0700` on Unix. Snapshot replacement
uses a temporary file, flush, and rename. As in Herdr, the directory is the
discovery record. It has no stable owner ID or explicit lifecycle state.
(`L:src/persist.rs:426-581`, `L:src/persist.rs:986-1054`,
`L:src/ipc/transport.rs:25-52`, `L:src/orch/mod.rs:669-734`)

Neither format checkpoints a process. Both snapshots contain enough topology
and launch context to reconstruct a useful screen. The exact saved and lost
fields are in research 06 and 07.

## How several clients share one session

Both servers accept several simultaneous display clients into one live
namespace. Each connection gets its own transport queues, dimensions, render
buffer or baseline, and a process-local numeric ID. These are connection views,
not separate multiplexer sessions. (`H:src/server/client_accept.rs:11-53`,
`H:src/server/clients.rs:30-75`, `L:src/ipc/server.rs:207-246`,
`L:src/ipc/server.rs:638-672`)

Every full client reads and mutates the same application object:

- Herdr has one `AppState.workspaces`, one active workspace, one active tab per
  workspace, and one focus value per tab. (`H:src/app/state.rs:1436-1501`,
  `H:src/workspace.rs:177-209`, `H:src/workspace/tab.rs:38-52`)
- Luvus has one `App.workspaces`, one `active_ws`, and shared focus, selection,
  scroll, modal, and hit-test state. (`L:src/app/mod.rs:1469-1503`,
  `L:src/app/mod.rs:1580-1680`, `L:src/layout.rs:59-62`)

One foreground client controls interactive geometry and shared PTY sizes. Herdr
promotes the latest connecting, resizing, or interacting full client. Luvus
promotes a client on key, mouse, or paste input, but not on a background resize.
Other clients receive projections at their own display sizes. (`H:src/server/clients.rs:249-315`,
`H:src/server/headless.rs:2921-2972`, `L:src/ipc/server.rs:690-744`,
`L:src/ipc/server.rs:820-887`)

There is no stable client identity across reattach. A reattached client gets a
new numeric ID, a new frame baseline, and new connection-local presentation
state. It sees the current shared application state because the server remained
alive or because the server restored a snapshot.

This is suitable for several views controlled by one trusted person. It is not
a human-user isolation model. Any full client can change the shared tree and can
become its size authority. Research 06 and 07 give the detailed ownership
matrices.

## Mapping to the broker and per-user runtime

### Objects and owners

| Target object | Owner | Clone input that maps | Clone input that does not map |
| --- | --- | --- | --- |
| User session record | Broker | A namespace has a name, endpoint, and running observation. (`H:src/session.rs:20-27`, `L:src/session.rs:20-35`) | Directory scanning and socket probing are not an authoritative catalog. The special `default` name is not identity. |
| Live user session | Per-user runtime | One state owner holds the workspace tree, PTYs, terminal grids, and agents. (`H:src/server/headless.rs:286-343`, `L:src/ipc/server.rs:248-388`) | One server per arbitrary named session does not match one runtime per user. A user name must not become an unchecked path component. |
| Durable user image | Per-user runtime, with broker lifecycle metadata | Versioned snapshots and atomic replacement are useful reconstruction inputs. (`H:src/persist/snapshot.rs:11-142`, `L:src/persist.rs:17-104`) | The image is not the broker's user registry and cannot prove whether a session is allowed to start. |
| Client attachment | Broker route plus runtime client view | Clients are disposable and may reconnect to live state. (`H:src/server/client_accept.rs:11-53`, `L:src/event.rs:61-83`) | Process-local numeric IDs and last-input-wins authority cannot identify a user or grant access. |
| Native agent session | Per-user runtime pane metadata | Saved agent references can later support an adapter-specific resume. (`H:src/persist/snapshot.rs:97-118`, `L:src/persist.rs:72-90`) | An agent session ID is not a product session ID, user ID, or authentication credential. |

### Required lifecycle contract

The broker must keep a durable record keyed by an opaque stable `UserId`. The
minimum record needs the runtime generation, lifecycle state, state directory,
and last failure or stop reason. Display names can change and must not select a
filesystem path directly.

The useful runtime states are `stopped`, `starting`, `running`, `stopping`, and
`failed`. Deletion is a broker operation, not a state inferred from a missing
socket. The broker must serialize start, stop, restart, and delete for one user.
Luvus's per-directory startup lock is useful local race protection, but the
broker remains the lifecycle authority. (`L:src/ipc/transport.rs:25-86`,
`L:src/ipc/server.rs:248-264`)

First authenticated attach can start a stopped runtime. A disconnect must only
remove the connection and its client view. It must never stop the runtime. This
maps directly from both clones. The runtime must keep reading PTY output and
updating its terminal grids without a display client.

A broker restart and a user-runtime restart are different events:

1. If only the broker restarts, live per-user runtimes and PTYs should continue
   when the supervision design permits re-adoption. The new broker must discover
   and verify runtime endpoints, then route new clients to the same runtime
   generation. If re-adoption is not available, this case becomes cold restore.
2. If one user runtime restarts, its live PTYs and children end. The replacement
   runtime must load that user's last complete image and create replacement
   panes. It must report this as cold restore, not reattach.
3. If the host reboots, all runtimes cold restore. Live child-process survival
   is not part of the version 1 contract.

The session image needs a generation or transaction ID. The broker must not
route a client until the runtime reports that restore and endpoint setup are
complete. Herdr binds its API before it creates and binds all app state, while
Luvus restores before it reports server ready. The target needs one explicit
readiness event. (`H:src/server/headless.rs:5067-5139`,
`L:src/ipc/server.rs:248-377`)

Stop and delete must be separate:

- Stop asks the runtime to save, ends its PTYs, waits for process exit, and
  leaves the durable image.
- Delete requires authorization, stops the runtime if policy permits, records a
  tombstone or generation change in the broker, and then removes only that
  user's state directory.
- A stale runtime or old snapshot must not recreate a deleted session.

The clones require a separate stop before delete. That safety check maps. Their
idempotent delete of a missing directory also maps. Their refusal to delete the
special default namespace does not map because the target uses stable users,
not a special path. (`H:src/session.rs:299-317`, `L:src/session.rs:329-361`)

### Multi-client and cross-user rules

Several clients authenticated as the same user can attach to that user's one
runtime. Each connection needs its own viewport, render baseline, selection,
scroll, and focus view. Shared topology changes must use user-scoped revisions
or serialized commands. Do not copy the clones' global presentation state.

Read-only cross-user viewing must not attach the viewer as a full client of the
target runtime. The broker can route an authorized snapshot and event stream
from the target runtime to the viewer. The viewer must not send input, take the
PTY size lease, change focus, receive private clipboard effects, or address a
target pane through the viewer's own namespace.

The broker authenticates both identities for such a route:

- actor: the connected user;
- owner: the user whose runtime is viewed;
- mode: owner control or authorized read-only view;
- target: an ID resolved only inside the owner's runtime generation.

This is the main point that does not map from Herdr or Luvus. Both clones have
one trust domain and no application user in the display handshake.
(`H:src/protocol/wire.rs:341-362`, `H:src/server/clients.rs:30-75`,
`L:src/ipc/protocol.rs:19-36`, `L:src/app/mod.rs:1469-1503`)

## Reuse and reject summary

Reuse:

- detached server-side PTY ownership;
- attach-or-start behavior for a stopped user runtime;
- no shutdown when the last client disconnects;
- one serialized state owner per user runtime;
- separate connection-local render state;
- versioned structural snapshots with temporary-file replacement;
- explicit stop before delete;
- Luvus's startup serialization and readiness reporting;
- clear language that separates live reattach from cold restore.

Reject or replace:

- named session as a directory-selected independent server process;
- a special undeletable `default` namespace;
- directory scans as the session catalog;
- socket reachability as the only lifecycle state;
- process-local client IDs as user identity;
- one global foreground client and last-input-wins PTY authority;
- shared selection, focus, scroll, and modal state across clients;
- full-control attachment for read-only cross-user viewing;
- any claim that a JSON snapshot preserves a live child process.

## Answers to issue 64

- A session in each clone is primarily one selected server namespace. Its live
  object is one server-owned application tree. Its durable object is a snapshot.
  A native agent session is a separate resume reference.
- Creation is implicit on first attach or server start. Names are validated path
  components. Listing scans directories. Attach starts or joins one server.
  Detach removes one client. Stop ends the server and panes. Delete removes only
  a stopped named namespace.
- Client disconnect preserves all live server state. Server restart preserves
  only saved and reconstructable state. Old PTYs, processes, memory, descriptors,
  and unsaved changes are lost.
- Both store JSON snapshots inside the selected namespace directory. Herdr has
  optional separate history. Luvus stores visible screen data in the main image
  and has a separate orchestration ledger.
- Several clients share one `App` and one workspace tree. They have separate
  render transport state, but most navigation and terminal state is shared. One
  foreground client controls interactive geometry.
- The live one-owner application maps to a per-user runtime. Disposable clients,
  cold restore, and stop-before-delete also map. Namespace names, directory
  discovery, global foreground control, and the absence of user authorization
  do not map. The broker must supply the durable user lifecycle and security
  boundary.
