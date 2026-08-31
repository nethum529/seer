# Luvus server lifecycle, multi-client state, and persistence

## Scope and source notation

This report checks the read-only Luvus clone at commit
[`d1013d16f48cdd724b8df40c7c4c83dc306dc5d6`](https://github.com/RizRiyz/luvus/commit/d1013d16f48cdd724b8df40c7c4c83dc306dc5d6).

Source references use these prefixes:

- `L:` means `/home/nethum/Projects/_research/luvus/`.
- `P:` means this repository.

The target is one server for several human users. Each human needs a separate
workspace, tab, and pane tree. The target platforms are macOS and Linux.

## Direct answer

| Question | Answer today | Evidence |
| --- | --- | --- |
| Can several display clients attach to one server? | Yes. The listener creates one thread and one `ClientState` for each accepted connection. The server renders every attached client. | [L:src/ipc/server.rs:207-246] [L:src/ipc/server.rs:972-983] [L:src/ipc/server.rs:824-860] |
| Does each client have its own viewport size and frame state? | Yes. Size, render buffer, prior frame, resync state, activity order, and animation mask are in `ClientState`. | [L:src/ipc/server.rs:207-238] |
| Does each client have its own workspace tree? | MISSING. One `App` owns one `workspaces` vector and one `active_ws`. All display clients send input into that `App`. | [L:src/ipc/server.rs:322-384] [L:src/ipc/server.rs:690-747] [L:src/app/mod.rs:1469-1500] |
| Does each client have its own focus, terminal scroll, or selection? | No. These values live in the shared `App`, shared `TileLayout`, or shared pane terminal engine. | [L:src/app/mod.rs:1498-1500] [L:src/layout.rs:59-62] [L:src/app/mod.rs:1683-1697] [L:src/app/mod.rs:1898-1908] [L:src/terminal/pty.rs:886-905] |
| Are hit rectangles global, as the prior report said? | Yes. They are fields on the shared `App`. Passive rendering saves and restores the active client's rectangles. A passive client does not keep its own interactive rectangle set. | [P:docs/research/05-comparison.md:66] [L:src/app/mod.rs:1898-1938] [L:src/ui/mod.rs:142-205] [L:src/ui/mod.rs:286-337] |
| Is there an active client lease? | Yes, in behavior. The source calls it `foreground`, not a lease type. One client ID owns interactive geometry and PTY sizing. Any key, mouse, or paste from another client takes it over. | [L:src/ipc/server.rs:384-389] [L:src/ipc/server.rs:690-744] [L:src/ipc/server.rs:820-850] |
| Is session persistence implemented? | Yes. `session.json` is versioned JSON written through a temporary file and rename. It saves structure, launch context, and one bounded visible ANSI screen per PTY pane. | [L:src/persist.rs:17-104] [L:src/persist.rs:790-983] [L:src/persist.rs:986-1035] |
| Do live processes survive server death? | No. Restore creates new PTYs and shells. Supported native agent sessions can be resumed. | [L:src/app/mod.rs:2516-2619] [L:src/terminal/pty.rs:385-417] |
| Is the task ledger durable and multi-actor? | Yes for multiple pane workers. No for multiple human principals. Tasks and leases name pane IDs and task IDs, not users. | [L:src/orch/mod.rs:63-90] [L:src/orch/mod.rs:112-137] [L:src/orch/mod.rs:581-632] |
| Is UHP transport neutral? | Yes for automation clients. UHP Access forwards normal UHP frames through a provider-selected secure byte stream. The existing rendered TUI protocol is separate from UHP. | [L:protocol/README.md:3-10] [L:protocol/uhp/v1/access/README.md:1-40] |
| Is there a user, account, identity, or ACL model? | MISSING. Local authority is the operating-system account. Delegated UHP tokens contain scopes and expiry, but no user principal. The display handshake contains only protocol version and terminal size. | [L:src/ipc/api.rs:121-137] [L:src/ipc/api.rs:283-313] [L:src/ipc/protocol.rs:16-35] |

The result is clear. Luvus already gives us one server with several attached
terminals. It does not give us one server with several isolated human users.
Named sessions do not close this gap. Each named session is a separate server
namespace, not a user tree inside one server. [L:src/session.rs:1-5]

## Server creation and lifecycle

### Default start and detached ownership

The default command selects a session, checks the client endpoint, starts a
server when needed, opens the launch directory as a workspace, and attaches the
thin client. [L:src/main.rs:64-123] [L:src/main.rs:498-506]

Server creation starts the current executable with the internal `server` role.
On Unix it disconnects standard input, output, and error, then calls `setsid`.
The server therefore survives the client process. [L:src/main.rs:680-708]

Startup is serialized per session directory with an advisory `server.lock`.
The lock remains on disk, while the operating system releases the held lock on
process exit or crash. The server checks both endpoints before it restores panes.
This prevents two servers from owning the same snapshot and PTYs. [L:src/ipc/transport.rs:25-86]
[L:src/ipc/server.rs:248-264]

The server owns one `App`, all panes, and all PTYs. API and display listeners
run around that single application event loop. [L:src/ipc/server.rs:248-346]
[L:src/ipc/server.rs:384-440]

### Detach and stop

A UI detach action sets `App.detach_requested`. The server removes only the
foreground display client and sends it `ServerMessage::Detach`. It then chooses
the most recently active remaining client. The server, `App`, and panes stay
alive. [L:src/app/keys.rs:689-689] [L:src/ipc/server.rs:502-511]

The wire protocol also has `ClientMessage::Detach`. The server treats that
message and read failure in the same way. Both generate `ClientDetach`, remove
the client state, and leave the application running. [L:src/ipc/protocol.rs:20-35]
[L:src/ipc/server.rs:674-688] [L:src/ipc/server.rs:1113-1165]

`server stop` is different. It asks the server to stop, waits until both sockets
are gone, and returns only after shutdown. The event loop performs a final
session save before it returns. Dropping a pane sends SIGHUP to its live Unix
child. [L:src/main.rs:865-910] [L:src/ipc/server.rs:624-625]
[L:src/terminal/pty.rs:198-231]

### Named sessions and discovery

A named session is an independent server namespace. Session selection happens
once, before normal routing, so its sockets, snapshot, PTYs, and child processes
use one namespace. [L:src/session.rs:1-5] [L:src/session.rs:40-117]

The default session uses the Luvus root. A named session uses
`<LUVUS_HOME>/sessions/<name>`. Every namespace has separate API and display
socket names. Session names are at most 64 bytes and allow only ASCII letters,
digits, `.`, `_`, and `-`. [L:src/session.rs:154-205]

`session list` does not use a global session manager. It always reports the
default namespace, scans the `sessions` directory for valid subdirectories,
sorts their names, and probes each API and display endpoint. [L:src/session.rs:245-288]

Attaching an unknown valid name creates its directory and server through the
ordinary default start path. Stopping a named session sends `server.stop` only
to that namespace. Deleting is allowed only after it stops. [L:src/session.rs:40-62]
[L:src/main.rs:122-123] [L:src/session.rs:291-360]

This model can give each human a separate tree only by running one named server
per human. That does not meet the stated one-server target. MISSING: a user key
inside one server-owned state model. [L:src/session.rs:1-5]
[L:src/app/mod.rs:1469-1500]

## Socket ownership and permissions

Release builds default to `~/.luvus`. Debug builds default to `~/.luvus-dev`.
`LUVUS_HOME` overrides both. [L:src/persist.rs:235-247]

On Unix, server startup requires a real session directory owned by the current
effective UID. It forces mode `0700` and fails closed if it cannot verify the
directory or permissions. [L:src/persist.rs:520-567]

The startup lock and both socket files use mode `0600`. A connecting Unix
client rejects a path that is not a socket, is owned by another UID, or does not
have mode `0600`. It also verifies that the connected server peer has the same
effective UID. Linux uses `SO_PEERCRED`. macOS uses `getpeereid`. [L:src/ipc/transport.rs:34-51]
[L:src/ipc/transport.rs:260-345] [L:src/ipc/transport.rs:584-603]
[L:src/ipc/transport.rs:633-669]

The API accept loop validates each accepted peer against the same UID. The
display accept loop does not call `validate_peer`; it relies on the owner-only
directory and socket before it starts the display handshake. [L:src/ipc/api.rs:1542-1554]
[L:src/ipc/server.rs:972-982] [L:src/ipc/transport.rs:672-675]

Long Unix socket paths use a stable UID-scoped alias under `/tmp` on Linux or
`/private/tmp` on macOS. The alias parent is also protected as an owner-only
directory. [L:src/session.rs:215-237] [L:src/persist.rs:506-532]

These checks implement one operating-system-account boundary. They intentionally
reject another local account. MISSING: a server identity, client principal, and
authorization policy that can admit several humans without making them one Unix
account. [L:src/ipc/transport.rs:271-279] [L:src/ipc/transport.rs:309-345]

## Display client attach, detach, and drop handling

### Attach handshake

The private display protocol is length-prefixed bincode. Its protocol version is
5. The first client message is `Hello { version, cols, rows }`. There is no user,
credential, session principal, or capability field. [L:src/ipc/protocol.rs:1-35]

The handshake is:

1. The client reads its terminal size and sends `Hello`. [L:src/ipc/client.rs:106-120]
2. The server rejects a version mismatch with `Welcome { error }`. Otherwise it
   sends a successful `Welcome`. [L:src/ipc/server.rs:1004-1046]
3. The server sends `Ready { probe_terminal }`. If requested, the client probes
   terminal colors and answers with `TerminalColors`. [L:src/ipc/server.rs:1048-1059]
   [L:src/ipc/client.rs:151-169]
4. The server creates a message channel and sends `ClientConnected` to the app
   loop. The app loop inserts a new `ClientState`. [L:src/ipc/server.rs:1061-1111]
   [L:src/ipc/server.rs:638-672]
5. The client starts an input thread and paints full frames and diffs until the
   socket closes or the server sends a lifecycle message. [L:src/ipc/client.rs:171-269]

A new connection becomes foreground immediately. Existing connections remain
attached. There is no rejection based on another attached client. [L:src/ipc/server.rs:638-672]

### Detach and dropped connections

An explicit client `Detach` or any read error sends `ClientDetach` to the app
loop. The loop removes the matching client. If it was foreground, the most
recently active remaining client becomes foreground. [L:src/ipc/server.rs:674-688]
[L:src/ipc/server.rs:1113-1165]

A failed frame write marks that client disconnected. Rendering removes it and
repairs foreground ownership if needed. A slow client has at most one pending
frame. A dropped update marks it behind, and its next successful update is a
full frame. [L:src/ipc/server.rs:172-204] [L:src/ipc/server.rs:810-860]
[L:src/ipc/server.rs:896-954]

No client drop removes a workspace, tab, pane, PTY, task, or path lease. Pane
leases are released when a pane closes, not when a display client disconnects.
[L:src/ipc/server.rs:674-688] [L:src/app/mod.rs:5586-5607]

## Multi-client behavior today

Two or more clients can attach at once. The focused tests render two clients at
different sizes and keep separate frame baselines. They also prove that a
passive small client does not resize the shared PTY. [L:src/ipc/server.rs:1376-1414]

The phrase "independent viewport" has a narrow meaning in Luvus. Each display
gets a frame sized for its own terminal. It does not mean independent navigation
or terminal scroll state. The public mobile guide also states that a passive
render cannot replace active mouse geometry or resize the shared PTY.
[L:src/ipc/server.rs:207-238] [L:website/src/content/docs/docs/guides/mobile.mdx:69-74]

### The active client lease

The prior architecture report identifies independent client viewports. The
comparison report leaves the related PTY-size lease as an open question.
[P:docs/research/03-luvus-architecture.md:215]
[P:docs/research/05-comparison.md:187] Current Luvus answers it with the
`foreground` variable and `interactive_size`. [L:src/ipc/server.rs:384-389]

| Lease question | Current rule | Evidence |
| --- | --- | --- |
| Who holds it? | One attached client ID in `foreground: Option<u64>`. The value is server memory only. | [L:src/ipc/server.rs:384-389] |
| Initial grant | Every newly connected client becomes foreground, even when another client is active. | [L:src/ipc/server.rs:638-672] |
| Activity tracking | Every input message updates that client's monotonic `last_activity` order. | [L:src/ipc/server.rs:690-696] |
| Background resize | A resize updates only that client's dimensions and full-frame flag. It does not take foreground. | [L:src/ipc/server.rs:697-710] [L:src/ipc/server.rs:1456-1482] |
| Implicit takeover | A key, mouse event, or paste from a background client immediately makes it foreground. Before input is handled, the server renders that client's view as interactive. | [L:src/ipc/server.rs:713-744] [L:src/ipc/server.rs:1484-1498] |
| Explicit takeover | MISSING. The display protocol has no acquire, release, deny, timeout, or consent message. | [L:src/ipc/protocol.rs:20-35] |
| Idle timeout | MISSING. Foreground changes on input, attach, detach, or failed connection. No time-based expiry is defined. | [L:src/ipc/server.rs:638-744] |
| Disconnect fallback | If foreground disconnects, the attached client with the greatest `last_activity` becomes foreground. | [L:src/ipc/server.rs:674-688] [L:src/ipc/server.rs:755-760] |
| What it controls | It controls the interactive render, global hit rectangles, compact mode, the terminal palette used by the shared app, and the PTY dimensions derived from its layout. Its input mutates the shared app. | [L:src/ipc/server.rs:713-744] [L:src/ipc/server.rs:762-771] [L:src/ipc/server.rs:820-850] [L:src/ui/mod.rs:407-464] [L:src/ui/mod.rs:520-550] |

This is last-interaction-wins control. It is not a lock that blocks other
clients. Two people typing can take control from each other on every input event.
[L:src/ipc/server.rs:690-744]

Detach and session-switch commands apply only to foreground. Notifications,
sound, URL requests, and clipboard writes are broadcast to all attached clients.
This means a selection made by one active client can send clipboard text to all
clients. [L:src/ipc/server.rs:502-549]

For several humans, MISSING: an explicit control policy. We must decide whether
control is per user, per tree, per pane, or per client. We also need acquire,
deny, release, disconnect, and timeout rules. The current implicit takeover is
not an authorization boundary. [L:src/ipc/protocol.rs:20-35]
[L:src/ipc/server.rs:690-744]

## Per-client and global state

| State | Current owner | Consequence | Evidence |
| --- | --- | --- | --- |
| Terminal columns and rows | Per `ClientState` | Each client receives a correctly sized frame. | [L:src/ipc/server.rs:207-238] |
| Render buffer and prior frame | Per `ClientState` | Diff baselines and full-frame recovery are independent. | [L:src/ipc/server.rs:207-238] [L:src/ipc/server.rs:865-954] |
| Backpressure and resync | Per `ClientState` | One slow client does not replace another client's frame baseline. | [L:src/ipc/server.rs:172-204] [L:src/ipc/server.rs:896-954] |
| Terminal color probe | Stored per client, applied from foreground to shared `App` | Passive colors are retained. Foreground colors control the rendered terminal theme and pane appearance. | [L:src/ipc/server.rs:207-238] [L:src/ipc/server.rs:762-771] |
| Workspace focus | Global `App.active_ws` | One user changing workspace changes it for every client. | [L:src/app/mod.rs:1498-1500] |
| Tab focus | Global `Workspace.active_tab` | One active tab exists per shared workspace, not per client. | [L:src/persist.rs:27-38] [L:src/persist.rs:970-977] |
| Pane focus | Global `TileLayout.focus` | One pane is focused in each shared tab. | [L:src/layout.rs:59-62] |
| Terminal scroll offset | Global in the pane's one `VtEngine` | Scrolling a pane changes the viewport rendered to every client. | [L:src/terminal/pty.rs:159-196] [L:src/terminal/pty.rs:886-905] [L:src/terminal/vt/mod.rs:252-270] |
| Mouse selection | Global `App.selection` | There is one active drag selection for the server app. | [L:src/app/mod.rs:1683-1685] |
| Keyboard copy selection | Global `App.copy_mode` | Copy mode and its navigation are shared. | [L:src/app/mod.rs:1686-1688] |
| Mouse grab and link hover | Global `App` fields | One client's pointer interaction can replace another client's transient pointer state. | [L:src/app/mod.rs:1689-1717] |
| Hit rectangles | Global `App` fields | Only foreground geometry is interactive. | [L:src/app/mod.rs:1898-1938] |
| UI list and modal scroll | Global `App` fields | Passive projections cannot change them, but foreground users share them. | [L:src/ui/mod.rs:142-173] [L:src/ui/mod.rs:286-307] |
| Compact/mobile presentation | Derived for each rendered size, but only foreground value remains in `App` | A passive phone gets a mobile frame. Its render does not make the desktop's interactive state mobile. | [L:src/ui/mod.rs:142-152] [L:src/ui/mod.rs:458-464] |

The prior claim about hit rectangles is verified. The implementation improved
passive rendering by saving and restoring the foreground rectangles. It did not
move rectangles into `ClientState`. [L:src/ui/mod.rs:175-205]
[L:src/ui/mod.rs:308-337]

For the target system, MISSING: at least two levels of state. A stable human
principal must own a workspace tree and durable navigation state. A connection
must own ephemeral viewport, render, pointer, and selection state. The current
`App` and `ClientState` split does not represent that model. [L:src/ipc/server.rs:207-246]
[L:src/app/mod.rs:1469-1500]

## PTY sizing with several clients

Each pane owns one PTY, one terminal engine, and one stored size. There is no
PTY instance per client. [L:src/terminal/pty.rs:159-196]

The foreground client renders first with `interactive = true`. Interactive
rendering computes pane content rectangles and resizes each shared pane PTY and
terminal engine to that content size. [L:src/ipc/server.rs:820-850]
[L:src/ui/mod.rs:520-550] [L:src/terminal/pty.rs:1088-1115]

Every other client uses `render_projection`. That path uses its own display
dimensions but passes `resize_panes = false`. It cannot resize the shared PTYs.
[L:src/ipc/server.rs:865-887] [L:src/ui/mod.rs:142-151]
[L:src/ui/mod.rs:407-407] [L:src/ui/mod.rs:536-545]

A background resize changes only its stored client size. Its first key, mouse,
or paste promotes it, commits its geometry, and resizes the PTYs before the
input reaches `App`. [L:src/ipc/server.rs:697-744]

This rule is coherent for several mirrors of one human session. It is not
coherent for separate human trees because there is only one global tree and one
global foreground lease. MISSING: select PTY size from the client that controls
that user's tree or pane. Different users must not resize each other's PTYs.
[L:src/app/mod.rs:1469-1500] [L:src/ipc/server.rs:820-850]

## Persistence

### On-disk layout

| Path under `LUVUS_HOME` | Scope and format | Evidence |
| --- | --- | --- |
| `session.json` | Default session snapshot. Pretty JSON. | [L:src/persist.rs:580-581] [L:src/persist.rs:986-1035] |
| `orch.json` | Default session task and lease ledger. Pretty JSON. | [L:src/orch/mod.rs:669-734] |
| `sessions/<name>/session.json` | Named session snapshot. | [L:src/session.rs:185-193] [L:src/persist.rs:580-581] |
| `sessions/<name>/orch.json` | Named session task and lease ledger. | [L:src/session.rs:185-193] [L:src/orch/mod.rs:732-734] |
| `server.lock`, `server.pid`, `luvus.sock`, `luvus-client.sock` | Per-session lifecycle and local IPC state. These are not snapshot data. | [L:src/ipc/transport.rs:25-51] [L:src/persist.rs:443-477] [L:src/session.rs:196-205] |
| `config.json` | One configuration shared by all named sessions in the OS account. It is not per session or per client. | [L:src/config.rs:542-553] |
| `modules.json` | One installed-module registry shared by the OS account. | [L:src/module/registry.rs:1-18] [L:src/module/registry.rs:102-127] |

There is no per-user directory inside a session. MISSING: a durable layout keyed
by stable user ID, plus rules for shared and private resources. [L:src/session.rs:185-205]
[L:src/persist.rs:20-25]

### Session snapshot schema

`SessionSnapshot.version` is currently 1. The root stores `active_ws` and an
ordered workspace list. Each workspace stores a stable ID, name, cwd, active tab,
tabs, and pin state. [L:src/persist.rs:17-38]

Each tab stores a stable ID, layout tree, focused pane ID, pane records, dashboard
kind, and optional name. Git, orchestration, and Mission Control tabs are saved
as type flags. Their live data is refreshed, loaded elsewhere, or re-derived.
[L:src/persist.rs:40-62] [L:src/persist.rs:796-837]

Each ordinary pane record stores:

- cwd and command;
- live pane name;
- native agent kind and session ID when ownership can be proved;
- captured agent launch flags;
- one ANSI screen image smaller than 256 KiB;
- module identity, or native file, diff, or preview view state.

[L:src/persist.rs:72-104] [L:src/persist.rs:839-968]

The saved terminal screen contains only the terminal engine's displayed rows.
It trims trailing blank rows. It does not serialize the full retained scrollback
grid. [L:src/terminal/vt/mod.rs:353-355]
[L:src/terminal/vt/alacritty.rs:968-1005]

The `command` field is written but is not used to restart an ordinary shell
pane. Restore starts the currently configured shell. Only a recognized native
agent session gets a generated resume command. [L:src/persist.rs:73-90]
[L:src/app/mod.rs:2516-2534] [L:src/app/mod.rs:2566-2592]
[L:src/terminal/pty.rs:385-417]

### Save triggers

Structural mutations such as pane creation and close set `session_dirty`.
Persisted pane names also set it. PTY output does not set it. [L:src/app/mod.rs:3326-3349]
[L:src/app/mod.rs:4368-4382] [L:src/app/mod.rs:5586-5608]
[L:src/app/input.rs:618-632]

The server saves a dirty session after a two-second debounce. Closing the last
project workspace requests one immediate attempt. A failed save stays dirty and
is retried on the normal cadence. Server shutdown performs a final save.
[L:src/ipc/server.rs:486-500] [L:src/app/mod.rs:5674-5683]
[L:src/ipc/server.rs:624-625]

There is no periodic save caused only by terminal output. After a prior
structural save, a hard crash can therefore lose newer visible-screen replay
data even though the workspace tree remains. MISSING: a bounded screen-dirty
save policy if recent screen replay is a requirement. [L:src/app/input.rs:618-632]
[L:src/ipc/server.rs:486-500]

Task, heartbeat, and lease mutations call `OrchState::save` when the mutation
commits. Pane close also saves released leases and unbinds its assigned task.
[L:src/app/dispatch.rs:3801-3834] [L:src/app/dispatch.rs:3849-3988]
[L:src/app/mod.rs:5586-5607]

### Versioning and atomic writes

Session snapshots have a version field. Load ignores a snapshot from a newer
version. Added fields use targeted serde defaults. There is no explicit session
migration function for old snapshot versions. [L:src/persist.rs:17-25]
[L:src/persist.rs:27-61] [L:src/persist.rs:1047-1054]

`session.json` is serialized to `session.json.tmp`, written, flushed, and renamed.
The code does not call `sync_all` on the file or parent directory. This is atomic
replacement, but it is not an explicit power-loss durability protocol.
[L:src/persist.rs:1004-1035]

`orch.json` also uses a temporary file and rename. Its write is best effort.
Write and rename errors are not returned to the caller. The ledger has no schema
version field. An unreadable or unparsable file loads as an empty ledger.
[L:src/orch/mod.rs:121-137] [L:src/orch/mod.rs:686-715]

MISSING: a versioned orchestration envelope, explicit migrations, reported save
errors, file and directory sync where required, and recovery tests for torn or
corrupt writes. [L:src/orch/mod.rs:121-137] [L:src/orch/mod.rs:686-715]

### Exact cold-restore boundary

Detach preserves the live server, PTYs, terminal grids, and child processes.
Cold restore after server stop, crash, or reboot does not. [L:src/ipc/server.rs:502-511]
[L:src/terminal/pty.rs:198-231]

Cold restore preserves or rebuilds:

| Preserved or rebuilt | Exact behavior | Evidence |
| --- | --- | --- |
| Workspace and tab order, IDs, names, cwd, pins, and active indices | Loaded from `session.json`; invalid indices are clamped. | [L:src/persist.rs:20-62] [L:src/app/mod.rs:2642-2662] |
| Pane layout and focus | Saved runtime pane IDs are remapped to new IDs. A corrupt tab is dropped without discarding other valid tabs. | [L:src/app/mod.rs:2411-2418] [L:src/app/mod.rs:2620-2640] |
| Ordinary shell pane | A new PTY and the current configured shell start in the saved cwd, or workspace cwd, or home fallback. | [L:src/app/mod.rs:2551-2598] [L:src/terminal/pty.rs:385-417] |
| Supported native agent session | Luvus builds a resume command from saved agent and session ID. If it cannot build one, the pane becomes an ordinary shell. | [L:src/app/mod.rs:2516-2534] [L:src/app/mod.rs:2566-2617] |
| Visible terminal appearance | Saved ANSI is replayed into a new empty terminal engine before the replacement child is ready. | [L:src/terminal/pty.rs:385-417] [L:src/terminal/pty.rs:559-588] |
| Module pane | Its entrypoint is re-run if still available. Otherwise restore falls back to a shell. | [L:src/app/mod.rs:2535-2598] |
| File, preview, and diff views | View specifications return. Current source or patch content is read again. | [L:src/app/mod.rs:2419-2514] |
| Git dashboard | It returns only if the cwd is still a repository, then refetches data. | [L:src/app/mod.rs:2332-2338] [L:src/app/mod.rs:2365-2381] |
| Mission Control | The tab returns. Usage and agent rows are re-derived. | [L:src/persist.rs:55-58] [L:src/app/mod.rs:2397-2409] |
| Orchestration board | The tab returns and reads its data from separate `orch.json`. | [L:src/persist.rs:51-54] [L:src/app/mod.rs:2383-2395] |

Cold restore loses:

| Lost state | Evidence |
| --- | --- |
| Live child process, PID, process tree, open files, kernel PTY, signals, and in-process program memory | Restore allocates a new `Pane`, PTY, process, and runtime identity. Pane drop hangs up the old child. [L:src/terminal/pty.rs:159-231] [L:src/app/mod.rs:2516-2619] |
| Unsaved shell or program execution state | An ordinary pane starts the configured shell. The saved `command` is not replayed. [L:src/app/mod.rs:2551-2598] [L:src/terminal/pty.rs:385-417] |
| Full terminal scrollback and terminal mode state | The snapshot stores one visible ANSI image, not the terminal grid or history. [L:src/persist.rs:88-90] [L:src/terminal/vt/alacritty.rs:968-1005] |
| Terminal scroll offset | No offset exists in `PaneSnap`. [L:src/persist.rs:72-104] |
| Mouse selection, copy mode, mouse grab, hover, modal state, and hit rectangles | Restore initializes these fields to empty values. [L:src/app/mod.rs:2703-2729] [L:src/app/mod.rs:2763-2779] |
| Display clients, frame baselines, foreground lease, and client sizes | These values are created only in the live server loop and `ClientState`. [L:src/ipc/server.rs:207-246] [L:src/ipc/server.rs:384-389] |
| Ephemeral agent detection, process scan, waiters, and usage caches | Restore creates new empty maps and queues, then re-derives live state. [L:src/app/mod.rs:2682-2691] [L:src/app/mod.rs:2760-2795] |

The ANSI replay is a visual landing screen. It is not proof that the old process
or application state survived. [L:src/terminal/pty.rs:385-417]

## Orchestration task ledger and path leases

`OrchState` is a separate single-writer state machine. All mutations run on the
application loop. Its only I/O is `orch.json` in the selected session directory.
[L:src/orch/mod.rs:1-9] [L:src/orch/mod.rs:669-734]

Tasks persist ID, title, status, pane assignee, dependencies, path intentions,
gate, bounded outputs and notes, worktree, branch, context usage, and timestamps.
Leases persist ID, pane, task, path patterns, and acquisition time.
[L:src/orch/mod.rs:63-119]

The ledger already assumes several actors in the form of pane workers. A task
can be claimed by one pane. A lease is granted for a pane and task only when it
does not overlap another task's active lease. Two claims are serialized by the
single app loop. [L:src/orch/mod.rs:265-300] [L:src/orch/mod.rs:581-632]

The ledger does not assume several human identities. `Task.assignee` and
`Lease.pane` are raw runtime pane IDs. Neither type has a user, account, owner,
tenant, or permission field. [L:src/orch/mod.rs:63-90]
[L:src/orch/mod.rs:112-137]

Pane IDs are reallocated after restart. On load, the app reconciles stale pane
bindings. A closed pane releases its path leases and unbinds its task. An
interrupted merge returns to its saved prior safe status, or `done` when no safe
prior status is available. [L:src/app/mod.rs:2325-2328]
[L:src/app/mod.rs:5586-5607] [L:src/orch/mod.rs:686-729]

For multiple humans, MISSING: a durable actor ID separate from pane ID, task and
lease ownership by actor, ACL checks for claim and release, and audit metadata.
Pane identity can remain the worker execution identity, but it cannot be the
human security principal. [L:src/orch/mod.rs:63-90]
[L:src/orch/mod.rs:112-137]

## UHP transport neutrality and remote clients

UHP 1.0 is the public automation protocol. The Unix socket is only its local
transport. The private binary display protocol is explicitly outside UHP.
[L:protocol/README.md:1-10]

UHP Access makes ordinary UHP frames transport neutral. A foreground command
binds an ephemeral `127.0.0.1` TCP port, creates a one-use pairing code, and
creates an expiring delegated token. A provider can forward that endpoint over
SSH, a private overlay, or another authenticated and encrypted ordered byte
stream. [L:src/uhp/mod.rs:13-35] [L:src/uhp/mod.rs:39-116]
[L:src/uhp/gateway.rs:78-115]
[L:website/src/content/docs/docs/guides/uhp-access.mdx:32-71]

The gateway checks the token and a bounded method allowlist before it opens the
owner-only local socket. Read-only mode allows safe reads. Control mode adds
workspace, tab, and pane focus, agent prompt, and terminal control methods.
[L:src/uhp/gateway.rs:250-340] [L:src/uhp/gateway.rs:372-385]

UHP delegated tokens are process-memory records with an ID, scopes, and expiry.
They are authorization capabilities, not user identities. A request without a
token on the owner-only local transport has full owner authority.
[L:src/ipc/api.rs:121-137] [L:src/ipc/api.rs:283-313]

Could UHP carry a remote client?

- Yes, for a remote automation or terminal-backend client. The gateway forwards
  ordinary requests, events, terminal observe streams, and terminal control
  streams. [L:src/uhp/gateway.rs:293-340]
- No, not for the existing rendered TUI client as-is. Its bincode `Hello`,
  `Frame`, `FrameDiff`, clipboard, notification, and lifecycle messages are a
  separate protocol. [L:src/ipc/protocol.rs:16-105]
  [L:protocol/README.md:7-10]
- The existing display client is already byte-stream adaptable. Its `attach`
  function accepts any `Read` and `Write`, and current remote attach carries it
  through an SSH bridge. [L:src/ipc/client.rs:46-60]
  [L:src/main.rs:609-672]

MISSING for the multi-human goal: bind every remote display or UHP connection
to an authenticated human principal. Pairing tokens currently identify granted
scope only. The display `Hello` has no authentication field. [L:src/ipc/api.rs:121-126]
[L:src/ipc/protocol.rs:20-35]

## User, account, identity, and permission model

There is no Luvus user or account object in the server state. The relevant
ownership types are:

| Object | Identity used today | Missing for several humans | Evidence |
| --- | --- | --- | --- |
| Display client | Process-local numeric client ID | Stable principal and authenticated session | [L:src/event.rs:61-83] |
| Workspace tree | One `App.workspaces` vector | User or tenant owner key | [L:src/app/mod.rs:1469-1500] |
| Pane | Runtime `PaneId` | Human owner and execution principal | [L:src/terminal/pty.rs:159-196] |
| Task | Optional runtime pane assignee | Human creator, owner, and allowed actors | [L:src/orch/mod.rs:63-90] |
| Path lease | Runtime pane and task | Human principal and authorization policy | [L:src/orch/mod.rs:112-119] |
| Local API caller | Same operating-system UID | Application principal | [L:src/ipc/api.rs:283-291] [L:src/ipc/transport.rs:283-345] |
| Delegated UHP caller | Bearer token scopes and expiry | Account identity and resource ACLs | [L:src/ipc/api.rs:121-137] [L:src/ipc/api.rs:283-313] |

The current permission model answers "may this OS account or token call this
method?" It does not answer "may this human access this workspace, pane, task,
or file?" [L:src/ipc/api.rs:283-313]

SSH remote display attach also does not add application identity. SSH selects
the remote operating-system account, then the remote bridge forwards the binary
display bytes to that account's Luvus socket. The `Hello` frame still carries no
principal. [L:src/main.rs:630-672] [L:src/ipc/protocol.rs:20-35]

## What we must build for one server and several humans

| Required component | Status | Minimum responsibility |
| --- | --- | --- |
| Stable user principal | MISSING | Add a non-reused `UserId`. Authenticate every display and UHP connection before it reaches shared state. The current display handshake has no identity. [L:src/ipc/protocol.rs:20-35] |
| Per-user workspace tree | MISSING | Replace the one global workspace vector and active index with user-owned durable trees. [L:src/app/mod.rs:1469-1500] |
| Per-connection presentation state | PARTIAL | Keep current size, frame, and backpressure state. Move pointer, selection, hit rectangles, modal presentation, and client-local scroll state out of global `App` where the product requires independent views. [L:src/ipc/server.rs:207-238] [L:src/app/mod.rs:1683-1717] [L:src/app/mod.rs:1898-1938] |
| Resource authorization | MISSING | Check the principal on every workspace, tab, pane, task, lease, file, Git, module, and terminal operation. Token scopes alone do not identify a resource owner. [L:src/ipc/api.rs:283-313] |
| Explicit control lease | MISSING | Replace last-input-wins takeover with named acquire, release, denial, disconnect, and timeout rules. Scope the lease to a user's tree or a shared pane. [L:src/ipc/server.rs:690-744] |
| PTY execution isolation | MISSING | Decide which Unix identity runs each user's PTYs and file operations. The current server and children run inside one OS-account boundary. [L:src/main.rs:680-708] [L:src/terminal/pty.rs:159-196] |
| Multi-user persistence schema | MISSING | Add a versioned root keyed by stable user ID. Define private and shared trees. Migrate current version 1 snapshots. [L:src/persist.rs:17-38] |
| Durable task actor identity | MISSING | Store human principal separately from pane worker identity in tasks, leases, and audit records. [L:src/orch/mod.rs:63-137] |
| Auth lifecycle | PARTIAL | Reuse bounded expiring UHP capabilities, but bind them to principals and resources. Add durable account/session policy only if required. Current tokens are memory-only. [L:src/ipc/api.rs:121-137] [L:src/ipc/api.rs:361-400] |
| Multi-user discovery and administration | MISSING | Add safe user provisioning, revocation, session listing, and recovery. Named session directory scans discover server namespaces, not users. [L:src/session.rs:245-288] |

## Decision for our design

Reuse these Luvus parts:

- One detached server and one single-writer mutation loop. [L:src/ipc/server.rs:248-440]
- Separate API and display protocols. [L:protocol/README.md:7-10]
- Per-client frame buffers, diff baselines, and bounded frame backpressure.
  [L:src/ipc/server.rs:172-238]
- Foreground-first PTY sizing as a starting mechanism, but only inside one
  user's tree or one explicitly shared pane. [L:src/ipc/server.rs:820-850]
- Versioned structural snapshots and separate orchestration persistence.
  [L:src/persist.rs:17-25] [L:src/orch/mod.rs:669-734]
- UHP's versioned schemas, capabilities, bounded streams, and transport-provider
  boundary. [L:protocol/uhp/v1/README.md:1-19]

Do not copy these parts unchanged:

- One global `App` presentation and workspace tree for every connection.
  [L:src/app/mod.rs:1469-1500]
- Last-input-wins foreground takeover. [L:src/ipc/server.rs:690-744]
- Global terminal scroll, selection, pointer state, and hit rectangles.
  [L:src/app/mod.rs:1683-1717] [L:src/app/mod.rs:1898-1938]
- OS-account ownership as the only human boundary. [L:src/ipc/api.rs:283-291]
- Pane IDs as the only orchestration actor identity. [L:src/orch/mod.rs:63-119]
- Unversioned, best-effort `orch.json` for security-relevant multi-user leases.
  [L:src/orch/mod.rs:121-137] [L:src/orch/mod.rs:686-715]

Luvus proves the detached server, multi-display rendering, bounded frame stream,
snapshot, orchestration ledger, and remote protocol seams. The multi-human data
and security model is MISSING. It must be designed before we make workspace,
pane, task, or persistence contracts stable. [L:src/ipc/server.rs:207-246]
[L:src/app/mod.rs:1469-1500] [L:src/ipc/api.rs:121-137]
