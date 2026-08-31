# Herdr server, multi-client, and persistence

## Decision

Herdr is a useful single-user server reference. It already has a detached
server, several simultaneous clients, local and SSH attach, and cold session
restore. (`H:src/server/autodetect.rs:179-218`,
`H:src/server/client_accept.rs:11-53`, `H:src/remote/host_unix.rs:8-32`,
`H:src/app/mod.rs:410-483`) It does not have human users or private per-user
workspace trees. One `HeadlessServer` owns one `App`, and that `App` owns one
`AppState` with one workspace tree. (`H:src/server/headless.rs:286-302`,
`H:src/app/state.rs:1436-1501`)

We should keep Herdr's server-owned PTYs and thin-client transport. We must add
an authenticated `UserId`, one durable user runtime per user, authorization on
every operation, and a separate client-view object for each connection. These
parts are MISSING from Herdr. Its hello message has no identity or credential,
and its connection record has no user or account field.
(`H:src/protocol/wire.rs:341-362`, `H:src/server/clients.rs:30-75`)

Do not copy Herdr's foreground policy as the multi-user boundary. In Herdr, the
latest full app client to connect, resize, or interact becomes foreground. That
client controls the shared PTY geometry, theme, and keybindings. This policy is
safe only because all full app clients operate the same shared app state.
(`H:src/server/headless.rs:301-329`, `H:src/server/headless.rs:1561-1579`,
`H:src/server/headless.rs:2921-2972`, `H:src/server/headless.rs:3055-3063`,
`H:src/server/headless.rs:3296-3307`)

The source references below use `H:` for the read-only clone at
`/home/nethum/Projects/_research/herdr`. The clone was inspected at commit
[`2290257acb2085ce6842ba5c7e3ca50c3ba64f02`](https://github.com/herdrdev/herdr/commit/2290257acb2085ce6842ba5c7e3ca50c3ba64f02).

## Server creation and discovery

The default `herdr` command uses server/client mode unless `--no-session` is
present. It checks the private client socket, starts a server when none is
listening, waits up to 15 seconds, and then runs the thin client.
(`H:src/main.rs:815-827`, `H:src/server/autodetect.rs:21-29`,
`H:src/server/autodetect.rs:280-305`)

| Step | Herdr behavior |
| --- | --- |
| Discover | Herdr tests whether the client socket path exists and accepts a local connection. A refused, timed out, missing, or stale path means that no server is available. (`H:src/server/autodetect.rs:39-91`) |
| Start | The launcher runs the current executable as `herdr server`. It passes the launch directory in `HERDR_STARTUP_CWD`. (`H:src/server/autodetect.rs:189-235`) |
| Detach | Standard input, output, and error go to null. On Linux and macOS, a pre-exec hook calls `setsid()`. (`H:src/server/autodetect.rs:210-218`, `H:src/platform/mod.rs:72-93`) |
| Wait | The launcher polls every 50 ms until the private socket accepts a connection or the 15 second deadline expires. (`H:src/server/autodetect.rs:21-29`, `H:src/server/autodetect.rs:242-273`) |
| Bind | The server binds the JSON API socket first. It then builds `App` and binds the private client socket. Both paths reject a live listener. (`H:src/server/headless.rs:5067-5128`) |
| Second start | A live API or client socket causes `AddrInUse`. The second server prints an already-running error and exits. (`H:src/ipc.rs:81-107`, `H:src/server/headless.rs:5072-5085`, `H:src/server/headless.rs:5114-5128`) |
| Stale socket | Before bind, Herdr connects to an existing path. If the connection is refused, missing, or timed out, it removes the path and binds a new listener. (`H:src/ipc.rs:81-115`) |
| Shutdown cleanup | Normal shutdown removes only the client socket whose recorded file identity still matches. `Drop` repeats that cleanup. The API handle uses the same identity-aware helper. (`H:src/ipc.rs:305-323`, `H:src/server/headless.rs:4891-4911`, `H:src/server/headless.rs:4964-4973`) |

Herdr has a default session and named sessions. The default data directory is
the Herdr config directory. A named session uses
`<config-dir>/sessions/<name>`. Each directory gets `herdr.sock`,
`herdr-client.sock`, and its own persistence files. The config directory is
`$XDG_CONFIG_HOME/herdr` when set, otherwise `$HOME/.config/herdr` in a release
build. (`H:src/config/io.rs:22-35`, `H:src/config/io.rs:61-67`,
`H:src/session.rs:157-185`)

`--session <name>` and `herdr session attach <name>` select a named session.
The name is at most 64 bytes and accepts only ASCII letters, digits, dot,
underscore, and hyphen. The name `default` maps back to the default paths.
(`H:src/session.rs:29-93`, `H:src/session.rs:425-465`)

Named sessions are independent servers, not users inside one server. Listing
sessions scans the session directories. Stopping sends `server.stop` to the
selected API socket. Deleting is allowed only after that session stops and
removes the whole named directory. (`H:src/session.rs:187-225`,
`H:src/session.rs:232-316`)

### Socket ownership and permissions

The server process creates both socket files. Herdr changes both modes to
`0600`, so only the owning OS account can open them through normal filesystem
permission checks. It records the socket device and inode so cleanup does not
remove a replacement socket. (`H:src/server/socket_paths.rs:11-12`,
`H:src/server/socket_paths.rs:60-75`, `H:src/api/server.rs:27-31`,
`H:src/api/server.rs:82-94`, `H:src/ipc.rs:25-33`,
`H:src/ipc.rs:289-323`)

Herdr does not call `chown` for these sockets. The application has no socket
owner configuration. Therefore shared access by several OS accounts is
MISSING. We would need a system service owner plus an explicit local access
policy, or one gateway that authenticates users before it reaches the private
server protocol. (`H:src/server/socket_paths.rs:60-75`,
`H:src/api/server.rs:137-147`, `H:src/ipc.rs:335-344`)

The session JSON files do not get an explicit permission mode. Herdr uses
`create_dir_all`, `write`, and `rename`; effective file permissions depend on
the process umask and existing directories. A private persistence permission
policy is therefore UNKNOWN at the application level. We would need explicit
directory and file modes and tests for both macOS and Linux.
(`H:src/persist/io.rs:48-60`)

## Client attach and detach

The private client protocol uses a four-byte little-endian length followed by a
bincode payload. The reader rejects an oversized frame and any trailing bytes
after one decoded message. (`H:src/protocol/wire.rs:900-966`)

| Phase | Client sends | Server sends or does |
| --- | --- | --- |
| Hello | Protocol version, terminal columns and rows, cell pixel size, render encoding, keybinding source, and app or direct-attach launch mode. There is no identity or credential. (`H:src/protocol/wire.rs:341-362`) | The server clamps the size, checks the version and keybindings, and sends `Welcome` with the selected version, encoding, and optional error. (`H:src/server/client_transport.rs:515-641`) |
| Register | Nothing more is required. (`H:src/server/client_transport.rs:643-689`) | The server creates separate reliable control and droppable render queues, starts one writer thread, and sends `ClientConnected` to the main loop. (`H:src/server/client_transport.rs:650-689`) |
| Normal input | Raw bytes, structured input, resize, clipboard image, direct attach commands, or `Detach`. (`H:src/protocol/wire.rs:364-432`) | The server routes app input to the shared `App`, or direct input to one terminal. It sends semantic frames or ANSI frames plus clipboard, title, notification, mode, bell, graphics, and shutdown effects. (`H:src/server/headless.rs:3131-3195`, `H:src/protocol/wire.rs:659-755`) |
| Graceful detach | `ClientMessage::Detach`, or the detach keybinding. (`H:src/protocol/wire.rs:378-391`, `H:src/server/client_transport.rs:933-949`) | An explicit detach removes the connection. The keybinding path sends graphics cleanup and `ServerShutdown { reason: "detached" }`, then closes its writer. (`H:src/server/headless.rs:2974-2990`, `H:src/server/headless.rs:3310-3319`) |
| Drop or error | EOF, a bad frame, an oversized frame, or a failed write. (`H:src/server/client_transport.rs:743-775`) | The reader or writer emits `ClientDisconnected`. The main loop removes the client. If it was foreground, the latest remaining full app client becomes foreground and the shared runtime is resized. (`H:src/server/client_transport.rs:706-729`, `H:src/server/headless.rs:1574-1579`, `H:src/server/headless.rs:1602-1645`, `H:src/server/headless.rs:3316-3319`) |

Dropping one client does not stop the server or its panes. The server continues
until a server stop, quit signal, or fatal process exit. On server shutdown it
sends a shutdown message to all remaining clients, drains them, and removes its
socket. (`H:src/server/headless.rs:4837-4894`)

## Multi-client behavior today

Yes. Two or more full app clients can attach to one Herdr server at the same
time. The accept loop assigns every connection a new numeric ID and starts a
handshake thread. The connected event inserts each accepted app client into one
`HashMap`. There is no one-client rejection or app-client count limit on this
path. (`H:src/server/client_accept.rs:11-53`,
`H:src/server/headless.rs:3001-3065`) A server test installs two clients at
120x40 and 44x20, then confirms that both receive frames at their own sizes.
(`H:src/server/headless.rs:8983-9040`)

All full app clients see and mutate the same workspace tree. This follows from
ownership, not from frame mirroring: `HeadlessServer` has one `app`, `AppState`
has one `workspaces` vector, each `Workspace` has one `active_tab`, and each
`TileLayout` has one focused pane. (`H:src/server/headless.rs:286-302`,
`H:src/app/state.rs:1436-1501`, `H:src/workspace.rs:177-209`,
`H:src/layout.rs:83-119`)

### Foreground policy

The foreground client is the full app client that most recently connected,
resized, or produced key, text, mouse, paste, or focus-gained input. Herdr gives
each interaction a monotonic activity stamp. When the foreground connection
drops, the remaining full app client with the greatest stamp takes over.
(`H:src/server/clients.rs:249-268`, `H:src/server/headless.rs:1561-1579`,
`H:src/server/headless.rs:2921-2963`, `H:src/server/headless.rs:3055-3063`,
`H:src/server/headless.rs:3296-3307`)

This policy exists to select one set of shared host facts. The field comment
names shared pane runtime size, theme, and input keybindings. Synchronization
copies the chosen client's terminal size, outer focus, cell size, keybindings,
and host appearance into the shared app. (`H:src/server/headless.rs:301-329`,
`H:src/server/headless.rs:1169-1231`)

Each full app client still gets a frame at its own terminal dimensions. Herdr
renders background clients first and the foreground client last. A background
render computes client-sized view geometry without resizing the PTYs. Herdr
also saves and restores four shared UI scroll offsets around that background
render. (`H:src/server/clients.rs:288-315`,
`H:src/server/headless.rs:4432-4491`, `H:src/ui.rs:139-156`)

### State ownership matrix

| State | Herdr owner today | Exact behavior |
| --- | --- | --- |
| Workspace tree | Global server state | One `AppState.workspaces` vector is owned by the one `App`. (`H:src/server/headless.rs:286-302`, `H:src/app/state.rs:1436-1447`) |
| Active workspace | Global server state | `AppState.active` and `AppState.selected` are single values shared by every client. (`H:src/app/state.rs:1443-1447`) |
| Active tab | Global per workspace | Each `Workspace` has one `active_tab` index. (`H:src/workspace.rs:177-209`) |
| Focused pane | Global per tab | Each `Tab` has one `TileLayout`; `TileLayout` stores one `focus` value. (`H:src/workspace/tab.rs:38-52`, `H:src/layout.rs:83-119`) |
| Outer terminal focus | Per connection, then projected globally | `ClientConnection.outer_terminal_focus` is per client. Only the foreground value is copied to `AppState.outer_terminal_focus`. (`H:src/server/clients.rs:42-53`, `H:src/server/headless.rs:1169-1225`) |
| Client frame viewport | Per connection | `terminal_size`, cell size, render baseline, graphics cache, and render-pending flags are stored on `ClientConnection`. (`H:src/server/clients.rs:30-75`) |
| App view geometry and hit areas | Global and transient | `AppState.view` is one shared `ViewState`. Each render recomputes it for that client. The foreground render runs last, so the retained shared geometry matches the foreground client. (`H:src/app/state.rs:1489-1501`, `H:src/server/clients.rs:288-315`, `H:src/server/headless.rs:4432-4491`, `H:src/server/headless.rs:9052-9074`) |
| Sidebar, agent panel, tab, and mobile switcher scroll | Global | The four offsets live on `AppState`. Herdr only preserves them around a non-foreground render. It does not keep one set per client. (`H:src/app/state.rs:1489-1495`, `H:src/server/headless.rs:4472-4491`) |
| Pane terminal scrollback viewport | Global per terminal runtime | Input calls `set_pane_scroll_offset`, which finds the shared terminal runtime and mutates its offset. (`H:src/app/input/mouse.rs:1889-1903`, `H:src/terminal/runtime.rs:266-283`) |
| Text selection and selection auto-scroll | Global | Both fields live once on `AppState`. (`H:src/app/state.rs:1495-1502`) |
| Copy mode and modal UI state | Global | `copy_mode`, `mode`, navigator, and modal fields are stored on the one `AppState`. (`H:src/app/state.rs:1443-1489`) |
| Keybindings and host theme | Per connection metadata, globally active for foreground | Each connection stores them. Foreground synchronization applies one set to the shared `App`. (`H:src/server/clients.rs:36-49`, `H:src/server/headless.rs:1201-1231`) |
| Input framing | Per connection | Each connection has its own raw input framer. (`H:src/server/clients.rs:48-53`) |
| Clipboard image staging | Per connection | Each connection owns its staged temporary file list; removal happens when that connection is removed. (`H:src/server/clients.rs:72-75`, `H:src/server/headless.rs:1602-1619`) |
| Pane PTY size | Global per terminal runtime | The foreground full app client drives the shared geometry. Direct attach is a separate exclusive size owner for one terminal. (`H:src/server/headless.rs:301-329`, `H:src/server/headless.rs:1101-1144`, `H:src/server/headless.rs:2877-2905`) |

The result is simultaneous access, not independent sessions. If Alice switches
a workspace, Bob's next frame shows that workspace. If Bob scrolls a pane or
selects text, Alice observes the same shared terminal viewport or selection.
The names Alice and Bob are examples; Herdr does not know either identity. The
shared ownership follows from the state fields in the matrix above.

## PTY sizing with several clients

For full app clients, the foreground client is the size authority. Herdr sets
`effective_size` from that client's terminal dimensions. It computes the whole
view at that size and resizes active and background tab runtimes to their
resulting pane rectangles. (`H:src/server/headless.rs:1101-1144`,
`H:src/server/headless.rs:1201-1222`, `H:src/ui.rs:158-188`,
`H:src/ui/panes.rs:198-240`, `H:src/ui/panes.rs:252-331`)

A background client gets a crop or projection at its own dimensions, but it
does not get a separate PTY size. Interaction or resize promotes that client
before input is routed, then resizes all shared pane runtimes. This avoids two
simultaneous sizes for one PTY, but rapid activity from different-sized clients
can repeatedly reflow terminal applications. (`H:src/ui.rs:139-156`,
`H:src/server/headless.rs:2921-2967`, `H:src/server/headless.rs:3296-3307`)

Direct terminal attach is different. One attach owner locks that terminal's
resize against normal app layout and directly sets its PTY size. A takeover can
replace the owner. An observer has its own render size but cannot resize the
terminal. (`H:src/server/headless.rs:2870-2905`,
`H:src/server/headless.rs:3250-3295`)

For our product, the PTY size lease is still a required design decision. The
recommended rule is one explicit size owner per user runtime, or per directly
controlled pane. Ordinary input should not silently transfer the lease. Other
clients should crop, pad, or reflow only their local projection. This avoids
Herdr's cross-user resize coupling, which follows from its interaction promotion
and shared resize paths. (`H:src/server/headless.rs:2921-2963`,
`H:src/server/headless.rs:3296-3307`)

## Persistence

### Paths and format

Herdr stores structural state in `<session-dir>/session.json`. Optional pane
history is in `<session-dir>/session-history.json`. The default session uses the
config directory. Named sessions use `<config-dir>/sessions/<name>`.
(`H:src/persist/io.rs:10-16`, `H:src/session.rs:157-185`)

Both files are pretty-printed JSON. The current snapshot version is 3. The
history file uses the same version number as the structural snapshot.
(`H:src/persist/io.rs:44-60`, `H:src/persist/snapshot.rs:11-37`)

### Saved state matrix

| Area | Saved data | Source |
| --- | --- | --- |
| Session | Version, workspaces, active workspace, selected workspace, sidebar width, sidebar split, and collapsed workspace-group keys. | `H:src/persist/snapshot.rs:14-29`, `H:src/persist/snapshot.rs:251-276` |
| Workspace | Stable workspace ID, custom name, identity cwd, worktree membership, public pane numbers, public tab numbers, next-number counters, tabs, and active tab. | `H:src/persist/snapshot.rs:49-69`, `H:src/persist/snapshot.rs:279-308` |
| Tab | Custom name, BSP layout, panes, zoom flag, focused pane raw ID, and root pane raw ID. | `H:src/persist/snapshot.rs:84-95`, `H:src/persist/snapshot.rs:311-381` |
| Pane metadata | Cwd, manual label, detected agent name, managed agent kind, native agent session reference, and launch argv. | `H:src/persist/snapshot.rs:97-118`, `H:src/persist/snapshot.rs:319-372` |
| Layout | Pane leaves and split nodes with direction and ratio. | `H:src/persist/snapshot.rs:126-142`, `H:src/persist/snapshot.rs:430-447` |
| Optional history | One ANSI string and line count per pane. It is captured from each live terminal runtime. | `H:src/persist/snapshot.rs:120-124`, `H:src/persist/snapshot.rs:384-428` |

### State that is not saved

| State | Result after cold restart | Source |
| --- | --- | --- |
| Live child process and PTY | Lost. Normal restore creates a fresh terminal runtime and shell in the saved cwd. | `H:src/persist/restore.rs:64-92`, `H:src/persist/restore.rs:575-665` |
| Arbitrary process memory and application state | Lost. Snapshot fields contain metadata, not a process image or PTY master descriptor. | `H:src/persist/snapshot.rs:14-142` |
| Terminal screen and scrollback | Lost by default. It returns only when opt-in pane history was captured and replayed. | `H:src/persist/snapshot.rs:384-428`, `H:src/persist/restore.rs:591-605` |
| Current terminal scroll position | Not in either snapshot schema. History saves ANSI text and a line count, not a viewport offset. | `H:src/persist/snapshot.rs:14-142` |
| Text selection and copy mode | Not in the snapshot schema. New `AppState` starts with no copy mode or selection. | `H:src/persist/snapshot.rs:14-142`, `H:src/app/mod.rs:600-628` |
| UI list scroll offsets | Not in the snapshot schema. New `AppState` initializes all four offsets to zero. | `H:src/persist/snapshot.rs:14-29`, `H:src/app/mod.rs:600-607` |
| Client connections and foreground client | Lost. These are runtime-only fields initialized as an empty map and `None`. | `H:src/server/headless.rs:507-530` |
| Per-client terminal size, input parser, frame baseline, theme, and clipboard staging | Lost. These fields exist only in `ClientConnection`, which is not part of the snapshot. | `H:src/server/clients.rs:30-75`, `H:src/persist/snapshot.rs:14-142` |
| Pane `seen` and right-click passthrough | Not in `PaneSnapshot`. Restored panes use `PaneState::new`, which sets `seen` true and right-click passthrough false. | `H:src/pane/state.rs:3-21`, `H:src/persist/snapshot.rs:97-110`, `H:src/persist/restore.rs:628-665` |

`launch_argv` is saved, but normal cold restore does not re-execute that command.
The cold path starts a shell. The saved argv is applied only to an imported live
handoff runtime so it can respawn a shell after that imported process exits.
(`H:src/persist/restore.rs:493-501`, `H:src/persist/restore.rs:575-635`)

Pane history is off by default. Enabling `[experimental] pane_history = true`
captures terminal output that can contain secrets. Disabling it removes an old
history file. (`H:src/config/model.rs:981-987`,
`H:src/config/model.rs:1904-1913`, `H:src/app/mod.rs:1559-1563`,
`H:src/persist/io.rs:63-75`)

### Versioning and migration

The parser rejects a version greater than 3. The loader logs and ignores a
newer or malformed file. It does not stop server startup.
(`H:src/persist/snapshot.rs:450-477`, `H:src/persist/io.rs:113-141`)

Versions at or below 3 pass through a field-default migration layer. A current
workspace has `identity_cwd`. A legacy pre-tabs workspace instead has a layout
and pane map; migration wraps it in one tab, derives an identity cwd, and fills
new identity and numbering fields with defaults. A workspace matching neither
shape is rejected. (`H:src/persist/snapshot.rs:71-82`,
`H:src/persist/snapshot.rs:144-249`)

History has no separate structural migration. It is deserialized directly and
accepted only when its version is at most 3. (`H:src/persist/snapshot.rs:461-470`)

### Write strategy, triggers, and debounce

Herdr serializes JSON, writes a fixed sibling path with extension `json.tmp`,
then renames it over the target. A failed rename removes the temp file. This
protects the prior target from a partial ordinary write.
(`H:src/persist/io.rs:44-60`)

MISSING: the write path does not sync the temporary file or parent directory.
It also writes `session.json` and `session-history.json` with two separate
renames, so the pair is not one atomic transaction. We would need file sync,
directory sync, generation IDs, and recovery rules for a mismatched structural
and history generation. (`H:src/persist/io.rs:48-75`)

The normal save debounce is five seconds. A state mutation either schedules a
save directly or marks `AppState.session_dirty`; the event loop converts that
dirty flag into the same deadline. When the deadline arrives, Herdr captures a
snapshot and saves it on one background thread. If a save is already running,
it retries after 250 ms. (`H:src/app/mod.rs:39-47`,
`H:src/app/state.rs:1622-1632`, `H:src/app/session.rs:13-84`,
`H:src/server/headless.rs:4807-4813`)

Structural operations schedule or mark saves. Examples include workspace and
tab creation, workspace and tab rename, tab move and close, pane API mutations,
layout changes, agent metadata changes, and focus or layout actions that call
`mark_session_dirty`. (`H:src/app/creation.rs:182-218`,
`H:src/app/creation.rs:221-274`, `H:src/app/api/workspaces.rs:91-105`,
`H:src/app/api/tabs.rs:150-168`, `H:src/app/api/tabs.rs:196-214`,
`H:src/app/api/tabs.rs:275-290`, `H:src/app/actions.rs:1120-1165`,
`H:src/app/api/panes.rs:1075-1109`, `H:src/app/agents.rs:50-60`)

The server performs a synchronous final save after its normal event loop exits.
If there are no workspaces, the save job removes both persistence files.
(`H:src/server/headless.rs:917-923`, `H:src/app/session.rs:39-58`,
`H:src/app/session.rs:86-107`, `H:src/persist/io.rs:96-111`)

## Cold restore boundary

At server construction, `App::new` loads the structural snapshot. It loads pane
history only when pane-history persistence is enabled. Restore starts at 80 by
24 cells before any client supplies real geometry. (`H:src/app/mod.rs:396-449`)

Restore remaps internal pane IDs, rebuilds workspace and tab identity counters,
recreates BSP layouts, restores each tab's focus and zoom, restores the active
tab, and clamps invalid workspace and tab indices. A tab with no successfully
restored panes is dropped. A workspace with no restored tabs is also dropped.
(`H:src/persist/restore.rs:257-304`, `H:src/persist/restore.rs:306-433`,
`H:src/persist/restore.rs:446-736`, `H:src/app/mod.rs:452-483`)

For an ordinary pane, Herdr uses the saved cwd when it still exists. Otherwise
it falls back to `HOME`, then `/`. It starts a new shell and can seed that new
terminal with saved ANSI history. If a pane spawn fails, it logs the failure and
prunes that pane from the restored layout. (`H:src/persist/restore.rs:469-501`,
`H:src/persist/restore.rs:575-705`)

A saved native agent session can take the stronger restore path when agent
resume is enabled and the reference produces a valid plan. Herdr de-duplicates
the same native session within one restore. A pane using native agent resume
does not replay saved ANSI history. The resume is pending until client geometry
is available. (`H:src/persist/restore.rs:502-566`,
`H:src/persist/restore.rs:739-790`, `H:src/server/headless.rs:1101-1158`)

Live handoff is a separate Unix path. It transfers PTY file descriptors to a
replacement server and disconnects clients. It is not the cold snapshot path.
(`H:src/server/headless.rs:1234-1409`, `H:src/persist/restore.rs:94-118`)

## Crash and recovery behavior

After a normal stop, Herdr saves immediately and removes its sockets. After an
abrupt crash, that final save and socket cleanup do not run. The next start can
remove stale socket files by probing them before bind. (`H:src/server/headless.rs:917-923`,
`H:src/server/headless.rs:4862-4911`, `H:src/ipc.rs:81-115`)

An abrupt crash can lose all changes since the last completed debounced save.
The upper bound is not fixed because a background save can already be running
and is retried while occupied. The next server loads only a successfully renamed
`session.json`; it does not inspect a temp file, backup, journal, or write-ahead
log. (`H:src/app/session.rs:60-84`, `H:src/persist/io.rs:48-60`,
`H:src/persist/io.rs:113-141`)

The fixed temp name and rename keep a partial JSON write away from the prior
target during ordinary filesystem operation. Power-loss durability is UNKNOWN
because there is no file or directory sync. Cross-file consistency is also
UNKNOWN because structure and history are separate writes. A recovery journal,
generation marker, backup policy, and fault-injection tests are MISSING.
(`H:src/persist/io.rs:48-75`)

Cold restart cannot reconnect to the old PTYs. It reconstructs panes as new
shells or supported native agent resumes. Live child processes survive only
detach while the original server remains alive, or a successful live handoff.
(`H:src/persist/restore.rs:64-92`, `H:src/persist/restore.rs:575-665`,
`H:src/persist/restore.rs:94-118`, `H:src/server/headless.rs:3310-3319`)

## Users, accounts, identity, and permissions

Herdr has no application user, account, tenant, role, or permission model. The
client hello identifies protocol and terminal capabilities only. The server
connection record identifies a numeric connection and its presentation state
only. The API accepts a parsed JSON request without an authentication step.
(`H:src/protocol/wire.rs:341-362`, `H:src/server/clients.rs:30-75`,
`H:src/api/server.rs:161-204`)

The only effective local boundary is the OS filesystem boundary on the two
`0600` socket files. Once a process can connect, the client protocol carries no
principal and the JSON API carries no authorization context.
(`H:src/server/socket_paths.rs:11-12`, `H:src/server/socket_paths.rs:72-75`,
`H:src/api/server.rs:27-31`, `H:src/api/server.rs:137-147`,
`H:src/protocol/wire.rs:341-362`, `H:src/api/server.rs:161-204`)

MISSING for our product:

- A stable `UserId` and an authenticated principal on every connection. The
  Herdr hello has no such fields. (`H:src/protocol/wire.rs:341-362`)
- A `UserRuntime` map in the server. Herdr owns one `App`, not one app state per
  user. (`H:src/server/headless.rs:286-302`)
- One private workspace, tab, pane, terminal, and agent namespace per user.
  Herdr has one shared workspace vector. (`H:src/app/state.rs:1436-1447`)
- Authorization checks on commands, terminal IDs, events, clipboard effects,
  persistence paths, and administrative actions. Herdr dispatches requests
  without a principal. (`H:src/api/server.rs:161-204`)
- An audit record that includes actor, action, target user, and result. Herdr's
  connection and protocol types cannot name an actor. (`H:src/server/clients.rs:30-75`,
  `H:src/protocol/wire.rs:341-362`)

## Network transport

The normal server sockets are local only. On macOS and Linux, Herdr converts a
filesystem path to an `interprocess` local socket name and binds or connects a
local listener. It does not bind a TCP address in this path.
(`H:src/ipc.rs:35-64`)

Herdr remote attach does not expose the daemon socket on the network. The local
process starts OpenSSH. On the remote host, `remote-client-bridge` connects to
the remote Unix client socket and copies SSH stdin and stdout to and from that
socket. Herdr delegates network transport and login authentication to the
`ssh` executable. (`H:src/remote/attach.rs:1817-1866`,
`H:src/remote/host_unix.rs:1-32`,
`H:docs/next/website/src/content/docs/persistence-remote.mdx:68-77`)

This SSH byte bridge can carry the current private protocol to another machine,
but it still reaches one shared Herdr `App`. The SSH login selects an OS account;
the Herdr hello still has no product user identity.
(`H:docs/next/website/src/content/docs/persistence-remote.mdx:51-58`,
`H:src/remote/host_unix.rs:8-32`,
`H:src/protocol/wire.rs:341-362`, `H:src/server/headless.rs:286-302`)

For a remote multi-user client, transport alone is not enough. We must build:

1. An authenticated session handshake that binds a transport credential to a
   stable `UserId`. This is MISSING from `ClientMessage::Hello`.
   (`H:src/protocol/wire.rs:341-362`)
2. Encryption and server identity. Reusing SSH can provide both. A direct TCP
   or QUIC listener would need TLS, certificate policy, and rotation, none of
   which exist on Herdr's local socket path. (`H:src/ipc.rs:35-64`,
   `H:src/remote/attach.rs:1817-1866`)
3. Authorization before target lookup, not after it. Herdr parses and dispatches
   API requests without a principal. (`H:src/api/server.rs:161-204`)
4. Bounded per-user and per-client queues, quotas, timeouts, reconnect tokens,
   protocol version negotiation, and audit events. Herdr keeps only one pending
   ordinary render, but its reliable control queue is an uncapped `VecDeque`.
   It has no user quota or reconnect identity.
   (`H:src/server/client_transport.rs:175-279`,
   `H:src/server/client_transport.rs:515-689`,
   `H:src/server/clients.rs:30-75`)
5. A policy for local desktop effects. Herdr forwards clipboard, notification,
   title, bell, and graphics effects to a client. A multi-user server must ensure
   that an effect goes only to a client for the same authenticated user.
   (`H:src/protocol/wire.rs:673-755`)

## Required ownership model for our server

The minimum safe split is:

| Owner | State |
| --- | --- |
| Server global | Listener lifecycle, user directory, authentication policy, protocol versions, quotas, and administration. These concepts are MISSING from Herdr's one-app server. (`H:src/server/headless.rs:286-343`) |
| User runtime | One private workspace tree, terminal registry, PTYs, agent state, persistence generation, and PTY-size leases. Herdr currently puts the equivalent state in its one shared `AppState` and runtime registry. (`H:src/app/state.rs:1436-1447`, `H:src/app/mod.rs:758-811`) |
| Client view | Active workspace, active tab, focused pane, UI viewport, list scroll, terminal scroll view, selection, copy mode, dimensions, theme, keybindings, and render baseline. Herdr keeps only the last five groups per connection; the earlier view fields are shared. (`H:src/server/clients.rs:30-75`, `H:src/app/state.rs:1443-1502`) |
| Shared user topology | Workspace, tab, pane membership, split tree, labels, cwd, and terminal attachment. Mutations need a user-scoped revision and conflict result. Herdr stores this topology in `Workspace`, `Tab`, and `TileLayout` without a user scope. (`H:src/workspace.rs:177-209`, `H:src/workspace/tab.rs:38-52`, `H:src/layout.rs:83-119`) |

Each command must carry both authenticated user identity and client identity.
User identity chooses the private runtime. Client identity chooses the view and
any explicit PTY-size lease. A target ID must resolve only inside that user's
namespace. These checks are MISSING because Herdr routes all app input into one
`App`. (`H:src/server/headless.rs:3131-3195`)

Persistence must also be per user. Use an opaque stable user ID in the path, not
an untrusted display name. Save user topology and durable metadata separately
from client views. Save a client view only if product requirements explicitly
need cross-device view restore. Herdr's current session path is selected by one
process-wide session name, and its snapshot contains one shared active and
selected workspace. (`H:src/session.rs:96-185`,
`H:src/persist/snapshot.rs:14-29`)

The earlier comparison left PTY-size ownership open. This source review closes
the question for Herdr: its current answer is the most recently foreground full
app client. It does not close our product decision because cross-user isolation
requires a stable user-scoped lease instead. (`docs/research/05-comparison.md:181-188`,
`H:src/server/headless.rs:301-329`, `H:src/server/headless.rs:1561-1579`)

No factual correction is needed for the Herdr claims from research documents
01 and 05 that this report relies on. This report adds the missing distinction
between several connections and several users. The prior architecture report
places one `AppState` under one headless server. The comparison marks
independent client views and delegated access as later work.
(`docs/research/01-herdr-architecture.md:32-57`,
`docs/research/05-comparison.md:108-113`,
`docs/research/05-comparison.md:128-128`)
