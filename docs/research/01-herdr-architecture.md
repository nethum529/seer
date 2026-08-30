# Herdr core architecture

This document describes the Herdr source in the read-only reference clone. All
paths and line numbers below are relative to that clone.

## Component 1: Package and process entry point

- Name: `herdr` executable package (`Cargo.toml:1-5`).
- Responsibility: Build one executable and route CLI, daemon, thin-client, and
  monolithic modes. The root manifest defines package `herdr` version 0.8.2.
  It has no Cargo workspace table (`Cargo.toml:1-77`,
  `src/main.rs:568-596`, `src/main.rs:815-832`).
- Key files: `Cargo.toml`, `src/main.rs`, `src/server/autodetect.rs`
  (`Cargo.toml:1-5`, `src/main.rs:528-596`,
  `src/server/autodetect.rs:280-305`).
- Key types: The entry point is `main`. Major code areas are Rust modules in
  one package. They include `api`, `app`, `client`, `detect`, `pane`,
  `persist`, `protocol`, `pty`, `server`, `terminal`, `ui`, and `workspace`
  (`src/main.rs:57-104`).
- How it talks to other components: `main` first gives CLI commands a chance to
  run. It then routes explicit `server` and `client` modes. The default launch
  uses daemon plus client mode. `--no-session` uses monolithic mode
  (`src/main.rs:568-596`, `src/main.rs:815-832`).

The default command checks the private client socket. It starts a detached
`herdr server` process when needed. It then runs the thin client
(`src/server/autodetect.rs:280-305`). The daemon has null standard streams and
uses platform detach logic (`src/server/autodetect.rs:179-218`). The monolithic
escape path creates the JSON API, a multi-thread Tokio runtime, the Ratatui
terminal, and `App` in one process (`src/main.rs:827-910`).

## Component 2: Session daemon and application core

- Name: Headless session server (`src/server/headless.rs:286-293`).
- Responsibility: Own persistent application state, terminal runtimes, API
  service lifetime, client connections, foreground-client policy, and the main
  event loop (`src/server/headless.rs:286-343`).
- Key files: `src/server/headless.rs`, `src/app/mod.rs`, `src/app/state.rs`,
  `src/session.rs` (`src/server/headless.rs:286-343`,
  `src/app/mod.rs:396-408`, `src/app/state.rs:1436-1445`,
  `src/session.rs:157-185`).
- Key types: `HeadlessServer`, `App`, `AppState`, and `SessionInfo`
  (`src/server/headless.rs:286-343`, `src/app/mod.rs:396-408`,
  `src/app/state.rs:1436-1445`, `src/session.rs:20-27`). `AppState` contains
  terminal state, workspaces, active selection, and UI state
  (`src/app/state.rs:1436-1447`).
- How it talks to other components: `HeadlessServer` owns `App`. It accepts TUI
  clients on the private socket and receives public API work through the app API
  channel (`src/server/headless.rs:286-343`, `src/server/headless.rs:5067-5121`).
  It sends terminal and notification effects to the foreground client instead
  of performing them in the daemon (`src/server/headless.rs:5105-5112`).

Each session has its own data directory. Named sessions use
`sessions/<name>`. The public API socket is `herdr.sock`. The private TUI socket
is `herdr-client.sock` (`src/session.rs:157-185`). The server always enables
session persistence and runs `App` on a multi-thread Tokio runtime
(`src/server/headless.rs:5087-5102`).

## Component 3: Workspace, tab, pane, and terminal state

- Name: Server-owned state hierarchy (`src/app/state.rs:1436-1445`).
- Responsibility: Model user workspaces, tab groups, tiled pane views, terminal
  identity, and live terminal runtimes (`src/workspace.rs:177-209`,
  `src/workspace/tab.rs:38-52`, `src/terminal/runtime.rs:12-17`).
- Key files: `src/workspace.rs`, `src/workspace/tab.rs`, `src/layout.rs`,
  `src/terminal/state.rs`, `src/terminal/runtime.rs`
  (`src/workspace.rs:177-209`, `src/workspace/tab.rs:38-52`,
  `src/layout.rs:72-92`, `src/terminal/state.rs:107-146`,
  `src/terminal/runtime.rs:12-17`).
- Key types: `Workspace`, `Tab`, `TileLayout`, `Node`, `PaneId`, `PaneState`,
  `TerminalState`, `TerminalRuntime`, and `TerminalRuntimeRegistry`
  (`src/workspace.rs:177-209`, `src/workspace/tab.rs:38-52`,
  `src/layout.rs:10-20`, `src/layout.rs:72-92`,
  `src/terminal/state.rs:107-146`, `src/terminal/runtime.rs:12-17`).
- How it talks to other components: A workspace owns tabs. A tab owns a BSP
  layout and pane view state. A pane points to a terminal identity. Production
  terminal runtimes live outside the tab in a registry
  (`src/workspace.rs:177-209`, `src/workspace/tab.rs:38-52`).

`Workspace` has a stable ID, identity directory, tabs, active-tab index, and
public pane-number allocation (`src/workspace.rs:177-209`). `Tab` owns one
`TileLayout` and a `HashMap<PaneId, PaneState>` (`src/workspace/tab.rs:38-52`).
The layout is a binary tree. Each node is a pane or a split with direction and
ratio (`src/layout.rs:72-92`). Splits allocate globally unique pane IDs and
replace one leaf with a split node (`src/layout.rs:13-20`,
`src/layout.rs:157-172`).

A new tab allocates the layout root, starts a terminal runtime, creates a
separate `TerminalState`, and attaches the pane view to that terminal ID
(`src/workspace/tab.rs:121-179`). `TerminalRuntime` is currently a wrapper over
the older `PaneRuntime`. The wrapper is the production boundary during a source
migration (`src/terminal/runtime.rs:12-17`).

## Component 4: PTY and child-process runtime

- Name: Terminal runtime and PTY actor (`src/pane.rs:1040-1066`).
- Responsibility: Start a shell or command in a PTY, move bytes between the PTY
  and emulator, resize the PTY, watch the child, detect agents, and stop owned
  processes (`src/pane.rs:1040-1063`, `src/pane.rs:1136-1215`,
  `src/pane.rs:1236-1250`, `src/pane.rs:2026-2164`).
- Key files: `src/pane.rs`, `src/pty/backend.rs`,
  `src/pty/backend/unix.rs`, `src/pty/actor.rs`,
  `src/pty/actor/unix.rs` (`src/pane.rs:1040-1066`,
  `src/pty/backend.rs:1-39`, `src/pty/backend/unix.rs:1-41`,
  `src/pty/actor.rs:1-20`, `src/pty/actor/unix.rs:1-20`).
- Key types: `PaneRuntime`, `PaneRuntimeIo`, `PtyIoActorHandle`,
  `SpawnedPty`, and `CommandBuilder` (`src/pane.rs:1044-1072`,
  `src/pty/actor/unix.rs:66-95`, `src/pty/backend.rs:7-39`).
- How it talks to other components: `TerminalRuntime` delegates to
  `PaneRuntime`. The runtime sends `AppEvent` values to the app. PTY output is
  parsed by `PaneTerminal`. Render damage goes through `RenderSignal`
  (`src/pane.rs:1040-1063`, `src/pane.rs:2035-2047`,
  `src/pane.rs:2110-2164`).

Herdr declares `portable-pty = 0.9.0` but patches it to the vendored copy
(`Cargo.toml:34-34`, `Cargo.toml:50-51`). The Unix backend opens a PTY, keeps the
master side, and spawns the child on the slave side
(`src/pty/backend/unix.rs:7-41`). The pane launch sets its own terminal identity
instead of inheriting the outer terminal identity (`src/pane.rs:74-82`). It also
adds Herdr workspace, tab, and pane identity variables
(`src/pane.rs:130-152`).

`PaneRuntime` owns the emulated terminal, PTY actor handle, child PID, output
sequence counters, and detection task (`src/pane.rs:1040-1063`). Dropping it
aborts detection, shuts down the actor, and stops pane processes unless process
preservation was selected (`src/pane.rs:1236-1250`).

On Unix, each pane gets one named PTY actor thread and a bounded command channel
with capacity 1024 (`src/pty/actor/unix.rs:351-406`). The thread uses `poll` on
the PTY and a wake pipe. It drains control commands and buffers pending writes
in a `VecDeque` (`src/pty/actor/unix.rs:418-495`). The Windows implementation
uses separate reader, writer, control, and input threads
(`src/pty/actor.rs:128-245`).

## Component 5: Terminal emulation, scrollback, and rendering

- Name: Ghostty terminal core and Ratatui view renderer
  (`src/ghostty/mod.rs:779-871`, `src/ui.rs:389-462`).
- Responsibility: Parse VT output, retain screen and history state, expose
  terminal cells, compose the Herdr UI, and paint client frames
  (`src/pane/terminal.rs:148-210`, `src/ghostty/mod.rs:1041-1084`,
  `src/ui.rs:389-462`, `src/protocol/render_ansi.rs:55-132`).
- Key files: `build.rs`, `vendor/libghostty-vt.vendor.json`,
  `src/ghostty/mod.rs`, `src/pane/terminal.rs`, `src/ui.rs`,
  `src/protocol/render_ansi.rs`, `src/server/alt_screen_read.rs`
  (`build.rs:32-95`, `vendor/libghostty-vt.vendor.json:1-5`,
  `src/ghostty/mod.rs:779-871`, `src/pane/terminal.rs:148-210`,
  `src/ui.rs:389-462`, `src/protocol/render_ansi.rs:55-132`,
  `src/server/alt_screen_read.rs:207-290`).
- Key types: `ghostty::Terminal`, `GhosttyPaneTerminal`, `PaneTerminal`,
  `ActiveScreen`, `ScreenSnapshot`, `FrameData`, and `BlitEncoder`
  (`src/ghostty/mod.rs:320-330`, `src/ghostty/mod.rs:779-871`,
  `src/pane/terminal.rs:148-210`, `src/terminal/history_read.rs:7-42`,
  `src/protocol/render_ansi.rs:45-61`).
- How it talks to other components: PTY bytes enter `PaneTerminal`, which locks
  the Ghostty-backed core and updates render state (`src/pane/terminal.rs:148-210`).
  `ui` reads `AppState` plus runtime snapshots and writes a Ratatui frame
  (`src/ui.rs:389-462`). A thin client converts semantic frames to ANSI with a
  stateful diff encoder (`src/protocol/render_ansi.rs:55-132`).

The build script compiles the vendored `libghostty-vt` with Zig and links it as
a static library (`build.rs:32-95`). The vendor metadata identifies source
commit `c5a21edfcbc2d5b46540ad91b7980aca31f5f1f3` and distribution
`libghostty-vt-1.3.2-HEAD` (`vendor/libghostty-vt.vendor.json:1-5`). The Rust
wrapper creates the terminal with column, row, and byte-based scrollback limits.
It feeds PTY bytes to `ghostty_terminal_vt_write`
(`src/ghostty/mod.rs:779-871`).

Ghostty exposes the active primary or alternate screen, total rows, scrollback
rows, and scrollbar position (`src/ghostty/mod.rs:1041-1084`). Normal viewport
movement uses Ghostty bottom, delta, and absolute-row operations
(`src/ghostty/mod.rs:1374-1405`). The vendored screen implementation treats the
configured scrollback limit as bytes and marks zero as no scrollback
(`vendor/libghostty-vt/src/terminal/Screen.zig:280-308`).

Alternate-screen applications manage their own viewport. For API reads, Herdr
checks that the active screen remains alternate, sends synthetic wheel events,
merges successive screen snapshots, restores the prior viewport, and falls back
to the passive snapshot when traversal is unsafe
(`src/server/alt_screen_read.rs:207-290`,
`src/server/alt_screen_read.rs:367-434`).

The client performs a full first paint. Later paints compare the current frame
with the last frame and write only changed cells. Paints use synchronized output
(`src/protocol/render_ansi.rs:1-27`). The comparison includes symbols, colors,
modifiers, and sanitized hyperlinks (`src/protocol/render_ansi.rs:757-799`).

## Component 6: Agent recognition and state detection

- Name: Agent detector and state arbiter (`src/detect/mod.rs:1-24`,
  `src/terminal/state.rs:5-21`).
- Responsibility: Identify the foreground agent process, classify its live
  terminal state, combine screen evidence with integration hooks, stabilize
  transitions, and publish state changes (`src/detect/mod.rs:237-309`,
  `src/pane/agent_detection.rs:39-77`, `src/pane.rs:2496-2551`,
  `src/terminal/state.rs:5-21`).
- Key files: `src/detect/mod.rs`, `src/detect/manifest.rs`,
  `src/detect/manifests/*.toml`, `src/pane.rs`,
  `src/pane/agent_detection.rs`, `src/terminal/state.rs`
  (`src/detect/mod.rs:1-24`, `src/detect/manifest.rs:138-181`,
  `src/pane.rs:2166-2228`, `src/pane/agent_detection.rs:39-77`,
  `src/terminal/state.rs:5-21`).
- Key types: `Agent`, `AgentState`, `AgentDetection`, `AgentManifest`,
  `ManifestRule`, `HookAuthority`, `TerminalState`, and
  `PendingIdleConfirmation` (`src/detect/mod.rs:9-24`,
  `src/detect/mod.rs:41-67`, `src/detect/manifest.rs:138-181`,
  `src/pane/agent_detection.rs:23-29`, `src/terminal/state.rs:16-24`,
  `src/terminal/state.rs:107-146`).
- How it talks to other components: The pane runtime probes foreground process
  groups and reads terminal detection text. It sends process and state events to
  `App`. `TerminalState` is the central arbitration point for screen and hook
  evidence (`src/pane.rs:2292-2551`, `src/terminal/state.rs:5-8`).

The internal state enum is `Idle`, `Working`, `Blocked`, or `Unknown`
(`src/detect/mod.rs:9-20`). The detector has 23 recognized agent kinds. Twenty
one use screen manifests (`src/detect/mod.rs:41-118`). Process recognition
normalizes known executable aliases and then selects the best match from the
foreground job (`src/detect/mod.rs:193-270`).

Screen detection reads the live bottom of the terminal and also accepts OSC
title and progress strings (`src/detect/mod.rs:1-4`,
`src/detect/mod.rs:286-309`). A manifest rule can select a region, priority,
visible-state flags, skip behavior, and nested text or regular-expression gates
(`src/detect/manifest.rs:138-181`). All rules are evaluated. The highest
priority matching rule wins (`src/detect/manifest.rs:446-527`). A recognized
agent with no matching rule falls back to idle. No recognized agent falls back
to unknown (`src/detect/manifest.rs:529-583`).

Detection polls at 500 ms before recognition and 300 ms after recognition. It
uses shorter 50 ms and 100 ms waits for pending release and idle confirmation
work (`src/pane.rs:2166-2228`). A working-to-plain-idle transition needs repeated
confirmation, with a 700 ms maximum hold, unless stronger evidence ends the hold
(`src/pane/agent_detection.rs:5-77`). Stable idle panes skip screen scans when
their detection content sequence has not changed
(`src/pane/agent_detection.rs:80-103`).

Full-lifecycle integrations for selected agents are hook-authoritative while
their matching process is live (`src/detect/mod.rs:316-325`,
`src/terminal/state.rs:1788-1857`). Other integration paths can use screen
evidence as fallback. Process exit clears matching authority before state is
recomputed (`src/terminal/state.rs:5-8`).

`Done` is not a fifth detector state. It is a presentation and API status.
`Idle` plus unseen completion becomes `Done`; `Idle` plus seen remains `Idle`
(`src/app/api_helpers.rs:96-106`). A non-idle state marks a pane seen. A
completion transition can mark an inactive pane unseen. Focusing the active tab
marks its panes seen again (`src/app/actions.rs:3104-3134`,
`src/app/actions.rs:1277-1297`).

## Component 7: Public CLI and JSON API

- Name: Newline-delimited JSON control API (`src/api/client.rs:31-35`,
  `src/api/client.rs:158-173`).
- Responsibility: Expose automation operations for sessions,
  workspaces, tabs, panes, agents, integrations, plugins, and events. The
  request enum defines these method groups (`src/api/schema.rs:45-243`).
- Key files: `src/api/client.rs`, `src/api/server.rs`, `src/api/schema.rs`,
  `src/api/schema/response.rs`, `src/cli.rs`, `src/ipc.rs`
  (`src/api/client.rs:31-74`, `src/api/server.rs:82-135`,
  `src/api/schema.rs:33-45`, `src/api/schema/response.rs:24-44`,
  `src/cli.rs:762-797`, `src/ipc.rs:35-77`).
- Key types: `ApiClient`, `Request`, `Method`, `SuccessResponse`,
  `ErrorResponse`, `ResponseResult`, and `ApiRequestMessage`
  (`src/api/client.rs:31-35`, `src/api/schema.rs:33-45`,
  `src/api/schema/response.rs:24-44`, `src/api/server.rs:824-839`).
- How it talks to other components: CLI code connects to `herdr.sock`, sends one
  JSON request line, and reads one JSON response line
  (`src/api/client.rs:31-74`, `src/api/client.rs:158-189`). The API connection
  thread dispatches the decoded request to the app and waits on a one-shot
  response channel (`src/api/server.rs:824-881`).

Local transport uses `interprocess` file-path sockets on Unix and namespaced
local sockets on Windows (`src/ipc.rs:35-77`). The request JSON has an `id` plus
a flattened method enum. Serde encodes the enum as `method` and `params`
(`src/api/schema.rs:33-45`). Success JSON has `id` and `result`. Error JSON has
`id` and an `error` object with `code` and `message`. Result variants use a
snake-case `type` tag (`src/api/schema/response.rs:24-44`).

The API socket is restricted to mode 0600 on Unix. An initial request is limited
to 1 MiB (`src/api/server.rs:27-32`, `src/api/server.rs:82-94`). The listener has
one thread, and each accepted connection gets another thread
(`src/api/server.rs:96-127`). The CLI performs a ping compatibility check before
normal commands (`src/cli.rs:762-797`).

## Component 8: Private server to TUI client protocol

- Name: Versioned binary render and input protocol
  (`src/protocol/wire.rs:15-44`).
- Responsibility: Attach one or more real terminals to the headless session and
  carry input, size, frames, terminal effects, notifications, clipboard data,
  and graphics (`src/protocol/wire.rs:341-430`,
  `src/protocol/wire.rs:659-755`).
- Key files: `src/protocol/wire.rs`, `src/server/client_transport.rs`,
  `src/client/mod.rs`, `src/client/input.rs`
  (`src/protocol/wire.rs:341-430`, `src/server/client_transport.rs:540-689`,
  `src/client/mod.rs:1-8`, `src/client/input.rs:80-100`).
- Key types: `ClientMessage`, `ServerMessage`, `RenderEncoding`,
  `ClientLaunchMode`, `ClientWriter`, and `ClientWriterQueue`
  (`src/protocol/wire.rs:37-64`, `src/protocol/wire.rs:341-430`,
  `src/protocol/wire.rs:659-755`, `src/server/client_transport.rs:47-54`,
  `src/server/client_transport.rs:175-188`).
- How it talks to other components: The client uses `herdr-client.sock` and
  starts with `Hello`. The server validates protocol compatibility and replies
  with `Welcome` (`src/session.rs:183-185`,
  `src/server/client_transport.rs:540-641`). The server forwards accepted client
  input to its event loop and streams rendered output through a dedicated writer
  thread (`src/server/client_transport.rs:650-689`,
  `src/server/client_transport.rs:706-805`).

Protocol version 21 uses bincode payloads with a four-byte little-endian length
prefix (`src/protocol/wire.rs:15-31`, `src/protocol/wire.rs:900-966`). The
handshake carries terminal geometry, requested render encoding, keybinding
profile, and launch mode (`src/protocol/wire.rs:341-362`). The server can send a
semantic `FrameData` value or pre-encoded terminal ANSI, plus control effects
(`src/protocol/wire.rs:659-722`).

Control messages are reliable and have priority. Render messages are droppable
and have capacity one, so a slow client cannot build a long render backlog
(`src/server/client_transport.rs:47-54`,
`src/server/client_transport.rs:218-279`). Each client gets a writer thread.
That thread drains control before render work
(`src/server/client_transport.rs:657-662`,
`src/server/client_transport.rs:706-728`).

## Component 9: TUI and mouse input

- Name: Ratatui and Crossterm interface (`Cargo.toml:29-36`).
- Responsibility: Compute screen geometry, render navigation and pane surfaces,
  capture host input, hit-test mouse positions, and route input to Herdr UI or
  the pane application (`src/ui.rs:360-462`,
  `src/app/input/mouse.rs:72-174`).
- Key files: `Cargo.toml`, `src/main.rs`, `src/ui.rs`,
  `src/app/input/mouse.rs`, `src/app/input/mod.rs`, `src/client/input.rs`
  (`src/main.rs:883-890`, `src/ui.rs:360-462`,
  `src/app/input/mouse.rs:72-174`, `src/app/input/mod.rs:329-425`,
  `src/client/input.rs:80-100`).
- Key types: Ratatui `Frame` and `Rect`, Crossterm `MouseEvent`, `AppState`,
  `MouseAction`, and `ClientInputEvent` (`src/ui.rs:1-6`,
  `src/app/input/mouse.rs:1-15`, `src/app/input/mouse.rs:28-64`,
  `src/protocol/wire.rs:95-136`).
- How it talks to other components: The renderer computes view geometry in
  `AppState`, including hit areas, and then paints from that state
  (`src/ui.rs:360-385`, `src/ui.rs:389-462`). Mouse events are hit-tested by
  `AppState`. UI actions return to `App`; pane mouse reports go to the terminal
  runtime (`src/app/input/mouse.rs:72-106`, `src/app/input/mod.rs:329-425`).

The exact declared TUI crates are `crossterm = 0.29` and
`ratatui = 0.30` with `unstable-rendered-line-info`
(`Cargo.toml:29-36`). The root dependency list has no OpenTUI dependency
(`Cargo.toml:23-48`). The one source identifier containing `opentui` is a
terminal palette-query compatibility test, not a TUI runtime
(`src/pane/terminal.rs:6083-6083`). The OpenTUI hypothesis is refuted.

Mouse capture is enabled or disabled with Crossterm when the host UI starts
(`src/main.rs:883-890`). Herdr handles clicks on UI controls, pane focus,
divider double clicks, and forwarding to mouse-aware pane applications
(`src/app/input/mod.rs:329-425`, `src/app/input/mouse.rs:72-174`). The private
protocol also has structured mouse
events for clients that do not provide Unix raw input bytes
(`src/protocol/wire.rs:330-337`, `src/protocol/wire.rs:417-418`).

## Component 10: Persistence

- Name: Versioned session snapshot store (`src/persist/snapshot.rs:11-29`).
- Responsibility: Save and restore workspace structure, tab layouts, pane
  launch state, agent resume identity, optional terminal history, and selected UI
  state (`src/persist/snapshot.rs:11-29`,
  `src/persist/snapshot.rs:49-136`).
- Key files: `src/persist.rs`, `src/persist/snapshot.rs`,
  `src/persist/io.rs`, `src/persist/restore.rs`, `src/app/session.rs`
  (`src/persist.rs:1-18`, `src/persist/io.rs:10-75`,
  `src/app/session.rs:39-83`).
- Key types: `SessionSnapshot`, `WorkspaceSnapshot`, `TabSnapshot`,
  `PaneSnapshot`, `LayoutSnapshot`, and `SessionHistorySnapshot`
  (`src/persist/snapshot.rs:14-37`, `src/persist/snapshot.rs:49-136`).
- How it talks to other components: `App` captures pure state plus terminal
  history, saves it in a background thread, and restores workspaces and terminal
  runtimes during startup (`src/app/session.rs:39-83`,
  `src/app/mod.rs:431-451`).

Snapshot format version 3 stores workspaces, active and selected positions,
sidebar state, tabs, pane launch data, agent session references, and the BSP
tree (`src/persist/snapshot.rs:11-29`,
`src/persist/snapshot.rs:49-136`). Optional pane history is ANSI text in a
separate snapshot (`src/persist/snapshot.rs:31-47`,
`src/persist/snapshot.rs:120-124`).

The main files are `session.json` and `session-history.json` in the active
session data directory (`src/persist/io.rs:10-16`). Writes serialize pretty JSON
to a temporary file and rename it over the target (`src/persist/io.rs:44-60`).
Missing, unreadable, malformed, or newer unsupported snapshots do not stop
startup. They are ignored (`src/persist/io.rs:113-171`).

State changes use a five-second save debounce
(`src/app/mod.rs:39-46`, `src/app/session.rs:13-24`). Saving runs on the named
`herdr-session-save` thread. If a save is already active, the next attempt is
deferred by 250 ms (`src/app/session.rs:60-83`). Final shutdown can join the
thread and save synchronously (`src/app/session.rs:86-98`).

## Component 11: Scheduling, buffering, and redraw control

- Name: Event-driven runtime and retained render path
  (`src/events.rs:1-4`, `src/server/headless.rs:708-800`).
- Responsibility: Bound high-volume work, coalesce redraw requests, skip hidden
  work, cap presentation rate, and prevent slow clients from adding latency.
  Its controls are bounded channels, retained rendering, and a droppable client
  render queue (`src/app/mod.rs:39-40`, `src/app/mod.rs:170-170`,
  `src/server/headless.rs:708-800`, `src/server/client_transport.rs:47-54`).
- Key files: `src/app/mod.rs`, `src/app/runtime.rs`, `src/render_signal.rs`,
  `src/server/headless.rs`, `src/server/client_transport.rs`,
  `src/protocol/render_ansi.rs` (`src/app/mod.rs:39-46`,
  `src/app/runtime.rs:530-550`, `src/render_signal.rs:6-18`,
  `src/server/headless.rs:708-800`, `src/server/client_transport.rs:47-54`,
  `src/protocol/render_ansi.rs:55-132`).
- Key types: Tokio runtime, bounded `AppEvent` channel, `RenderSignal`,
  `RenderRequest`, `RetainedRenderPlan`, and `ClientWriterQueue`
  (`src/server/headless.rs:5089-5092`, `src/app/mod.rs:170-170`,
  `src/render_signal.rs:6-18`, `src/server/headless.rs:764-786`,
  `src/server/client_transport.rs:175-188`).
- How it talks to other components: PTY tasks and background workers send app
  events. Render producers mark generic, PTY, or title damage. The server main
  loop classifies that damage and streams only the required output
  (`src/render_signal.rs:6-18`, `src/server/headless.rs:708-800`).

The app event channel has capacity 256 (`src/app/mod.rs:170-170`,
`src/app/mod.rs:396-408`). `RenderSignal` coalesces repeated work and keeps the
pane IDs that caused PTY or title damage (`src/render_signal.rs:6-18`,
`src/render_signal.rs:32-98`). Hidden-only PTY changes can be skipped. Visible
PTY changes can take a retained update path. Other changes cause a full render
(`src/server/headless.rs:743-796`).

The minimum render interval is 16 ms. `App` tracks render and presentation times
separately (`src/app/mod.rs:39-40`, `src/app/runtime.rs:530-550`). The local path
waits on event sources with Tokio selection instead of redrawing continuously
(`src/app/mod.rs:1164-1215`). The semantic
client then diffs frames and writes only visually changed cells
(`src/protocol/render_ansi.rs:68-132`,
`src/protocol/render_ansi.rs:757-799`).

## Takeaways for our tool

### What to copy

- Copy the separation between pane view state, terminal state, and live PTY
  runtime (`src/workspace/tab.rs:21-25`, `src/workspace/tab.rs:38-52`,
  `src/workspace/tab.rs:171-179`).
- Copy stable workspace, tab, pane, and terminal identities. Keep layout as pure
  data and runtime ownership in a registry (`src/workspace.rs:177-209`,
  `src/workspace/tab.rs:38-52`).
- Copy the small detector state model. Derive `Done` from
  idle plus unseen completion (`src/detect/mod.rs:9-20`,
  `src/app/api_helpers.rs:96-106`).
- Copy manifest-driven screen detection and explicit hook authority. Keep source
  arbitration in one place (`src/detect/manifest.rs:138-181`,
  `src/terminal/state.rs:5-8`).
- Copy bounded PTY control queues, redraw coalescing, a frame-rate cap, and a
  one-frame client render queue (`src/pty/actor/unix.rs:351-406`,
  `src/render_signal.rs:6-18`, `src/app/mod.rs:39-40`,
  `src/server/client_transport.rs:47-54`).
- Copy versioned, atomic JSON snapshots. Keep optional history separate from
  structural state (`src/persist/snapshot.rs:11-37`,
  `src/persist/io.rs:44-75`).

### What to avoid

- Avoid starting with both a public JSON protocol and a private render protocol
  unless persistent multi-client rendering is a first release requirement.
  Herdr maintains two sockets and two wire contracts
  (`src/session.rs:169-185`, `src/api/schema.rs:33-45`,
  `src/protocol/wire.rs:341-362`).
- Avoid binding terminal lifetime, child-process shutdown, detection scheduling,
  and PTY IO into one large runtime type. `PaneRuntime` currently owns all of
  these concerns (`src/pane.rs:1040-1063`, `src/pane.rs:1236-1250`).
- Avoid screen scraping as the only agent-state source. Herdr needs process
  probes, manifests, OSC evidence, transition stabilization, and hook authority
  to make it reliable (`src/detect/mod.rs:237-309`,
  `src/pane/agent_detection.rs:39-77`, `src/terminal/state.rs:1788-1857`).
- Avoid active alternate-screen traversal in an initial implementation. Herdr's
  read path must inject wheel input, merge snapshots, restore the viewport, and
  retain a fallback (`src/server/alt_screen_read.rs:207-290`,
  `src/server/alt_screen_read.rs:367-434`).
- Avoid a second implementation language in the build until terminal fidelity
  proves that it is needed. Herdr's Ghostty core adds a Zig build and static
  linking step (`build.rs:52-95`).

### Open questions

- UNKNOWN: Which agent integrations are required for our first release. Resolve
  this with a target-agent list and recorded terminal fixtures.
- UNKNOWN: Whether our tool needs persistent detached sessions or only a local
  monolithic process. Resolve this with product requirements for detach,
  reconnect, and multiple clients.
- UNKNOWN: Whether a vendored terminal emulator is justified. Resolve this with
  a compatibility test set for alternate screen, Unicode, mouse modes, OSC,
  synchronized output, and scrollback.
- UNKNOWN: Whether our public automation API must be stable before the UI model
  is stable. Resolve this by naming the first external API consumers.
- UNKNOWN: Whether restored agent processes must resume or whether restoring
  layout and shell history is enough. Resolve this with explicit crash and
  restart scenarios.
