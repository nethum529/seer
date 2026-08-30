# Luvus architecture

## Scope and source notation

This report uses the checked-out Luvus and Herdr reference clones named in the ticket.

Source references use these prefixes:

- `L:` means `/home/nethum/Projects/_research/luvus/`.
- `H:` means `/home/nethum/Projects/_research/herdr/`.

Line counts below are physical file lines. Large files serve more than one feature. The counts show the source surface that a maintainer must understand. They are not additive estimates of marginal implementation cost.

## Component: package and source layout

**Name:** Package and source layout.

**Responsibility:** It builds and packages one Rust binary named `luvus`. The root manifest has a `[package]` and one `[[bin]]`. It has no `[workspace]` section. This is one Cargo package, not a multi-crate Cargo workspace. [L:Cargo.toml:1-47]

**Key files:** `Cargo.toml` defines the binary, features, dependencies, release profile, and local crate patches. `src/main.rs` is the binary entry point. [L:Cargo.toml:1-47] [L:Cargo.toml:55-129]

**Key types:** There is no package-level library facade. The major types live in source modules under `src/`, including `App`, `Workspace`, `Tab`, `Pane`, and `OrchState`. [L:src/main.rs:1-54] [L:src/app/mod.rs:324-375] [L:src/app/mod.rs:1027-1122] [L:src/app/mod.rs:1469-1601] [L:src/terminal/pty.rs:150-196] [L:src/orch/mod.rs:96-136]

**How it talks to other components:** `main` selects a process role and calls the server, client, local TUI, remote bridge, integration, or CLI component. [L:src/main.rs:64-124] [L:src/main.rs:474-506] [L:src/main.rs:609-747]

Two vendored libraries are separate packages but are patched into this package. `luvus-vte` version 0.15.0 exposes the library name `vte`. `luvus-alacritty-terminal` version 0.26.1 exposes the library name `alacritty_terminal` and depends on the local `luvus-vte`. [L:vendor/vte/Cargo.toml:12-56] [L:vendor/alacritty_terminal/Cargo.toml:12-45] [L:vendor/alacritty_terminal/Cargo.toml:90-98] [L:Cargo.toml:120-129]

## Component: process router and named sessions

**Name:** Process router and named sessions.

**Responsibility:** One executable supports several roles. It can run a headless server, a thin client, a local monolith, a remote byte bridge, an integration handler, or a CLI command. The default path starts or finds a server and attaches a client. [L:src/main.rs:64-124] [L:src/main.rs:474-506] [L:src/main.rs:609-747]

**Key files:** `src/main.rs` routes roles and starts a detached server. `src/session.rs` validates names and derives per-session paths. [L:src/main.rs:680-747] [L:src/session.rs:40-117] [L:src/session.rs:139-205]

**Key types:** `SessionMeta` records named-session metadata. Session helpers derive separate API and client socket paths under a session directory. [L:src/session.rs:185-205] [L:src/session.rs:245-289]

**How it talks to other components:** The default route launches the current executable with the internal `server` role, then connects the client. Remote attach launches an SSH bridge that carries the same framed client protocol. [L:src/main.rs:474-506] [L:src/main.rs:609-709] [L:src/main.rs:680-747] [L:src/ipc/client.rs:46-49] [L:src/ipc/client.rs:364-418]

The durable session is the server-owned model and its saved snapshot. A PTY child is not a separately durable service. Restore recreates panes from saved commands, working directories, agent session data, and screen snapshots. [L:src/ipc/server.rs:1-3] [L:src/persist.rs:20-104] [L:src/app/mod.rs:2332-2344] [L:src/app/mod.rs:2516-2620]

## Component: server, client IPC, and event loop

**Name:** Server, client IPC, and event loop.

**Responsibility:** The server is the single owner of mutable application state and PTYs. It accepts API clients and interactive TUI clients on separate listeners. [L:src/ipc/server.rs:1-3] [L:src/ipc/server.rs:248-346]

**Key files:** `src/ipc/server.rs` owns startup, listeners, the event loop, rendering, and shutdown. `src/ipc/client.rs` owns terminal input and direct frame painting. `src/ipc/protocol.rs` defines the private interactive protocol. `src/event.rs` defines events sent into the application loop. [L:src/ipc/server.rs:248-500] [L:src/ipc/client.rs:106-230] [L:src/ipc/protocol.rs:20-165] [L:src/event.rs:1-126]

**Key types:** `AppEvent` carries PTY, client, module, file, search, and session results. `ClientMessage` and `ServerMessage` carry input, resize, full frames, and frame diffs. Each client has its own dimensions, current buffer, previous frame, and resync state. [L:src/event.rs:1-126] [L:src/ipc/protocol.rs:20-120] [L:src/ipc/server.rs:207-238]

**How it talks to other components:** Worker threads and PTY readers send `AppEvent` values through channels. The event loop applies them to `App`, asks the UI to render into a per-client buffer, computes a diff, and sends it to the client. Mutating API requests also enter this same loop and return through a reply channel. [L:src/ipc/server.rs:406-458] [L:src/ipc/server.rs:586-620] [L:src/ipc/server.rs:824-950] [L:src/ipc/api.rs:1-28]

## Component: core application model

**Name:** Core application model.

**Responsibility:** It owns workspaces, tabs, panes, focus, configuration, overlays, agent status, modules, and UI state. The server keeps one mutable `App`. [L:src/app/mod.rs:1469-1601] [L:src/app/mod.rs:1992-2059]

**Key files:** `src/app/mod.rs` defines the main model. `src/ids.rs` defines stable pane IDs. `src/layout.rs` defines tab layout trees. [L:src/app/mod.rs:324-375] [L:src/app/mod.rs:1027-1122] [L:src/app/mod.rs:1469-1601] [L:src/ids.rs:1-8] [L:src/layout.rs:1-75]

**Key types:** `Workspace` has a stable ID, name, working directory, branch and worktree data, tabs, active-tab index, and pin state. `Tab` has a stable ID, `TileLayout`, Git state, orchestration and mission dashboard state, and a name. `App` owns the pane map and workspace list. [L:src/app/mod.rs:324-375] [L:src/app/mod.rs:1027-1043] [L:src/app/mod.rs:1469-1516]

**How it talks to other components:** `App::new` loads configuration, theme, and the module registry, then creates the initial pane. Input and API dispatch mutate this model. The persistence layer serializes selected model fields. The UI reads the model to produce a Ratatui buffer. [L:src/app/mod.rs:2027-2059] [L:src/app/input.rs:1012-1052] [L:src/ipc/api.rs:1-28] [L:src/persist.rs:20-104] [L:src/ui/mod.rs:114-151]

## Component: pane layout

**Name:** Pane layout.

**Responsibility:** It models split panes as a binary space partition tree. It computes rectangles, focus movement, swaps, splits, and removals. [L:src/layout.rs:1-75] [L:src/layout.rs:77-203]

**Key files:** `src/layout.rs` contains both the mutable layout and its serializable tree form. [L:src/layout.rs:1-75] [L:src/layout.rs:590-655]

**Key types:** `Node` is either a pane leaf or a horizontal or vertical split. `TileLayout` owns the root and focused pane. `LayoutTree` is the serializable form. [L:src/layout.rs:1-75]

**How it talks to other components:** Tabs own a `TileLayout`. The UI gives each leaf a terminal rectangle. Persistence saves and restores a remappable tree so new runtime pane IDs can replace saved IDs. [L:src/app/mod.rs:324-375] [L:src/layout.rs:77-203] [L:src/layout.rs:590-655] [L:src/persist.rs:20-104]

## Component: PTY ownership and terminal emulation

**Name:** PTY ownership and terminal emulation.

**Responsibility:** A `Pane` owns a portable PTY, a terminal engine, process metadata, input channels, size, revisions, and repaint coalescing state. Dropping a pane hangs up its child. [L:src/terminal/pty.rs:150-235]

**Key files:** `src/terminal/pty.rs` creates PTYs and reader and writer threads. `src/terminal/vt/mod.rs` defines the emulator boundary. `src/terminal/vt/alacritty.rs` adapts the vendored Alacritty terminal. [L:src/terminal/pty.rs:480-535] [L:src/terminal/vt/mod.rs:1-6] [L:src/terminal/vt/alacritty.rs:1-23]

**Key types:** `Pane`, `MouseModes`, `VtEngine`, `AlacrittyEngine`, `RenderCell`, and `Cursor` form the terminal layer. The current engine factory exposes only the Alacritty implementation. [L:src/terminal/pty.rs:150-196] [L:src/terminal/vt/mod.rs:71-153] [L:src/terminal/vt/mod.rs:187-260] [L:src/terminal/vt/alacritty.rs:92-155]

**How it talks to other components:** The PTY reader feeds byte chunks into the engine under a lock and sends at most one outstanding `PtyData` wake event. The renderer reads visible cells, display offset, wide-cell state, cursor state, and bounded history from `VtEngine`. Input flows in the other direction through the pane writer channel. [L:src/terminal/pty.rs:1192-1223] [L:src/terminal/vt/mod.rs:187-260] [L:src/terminal/vt/alacritty.rs:527-690]

Terminal emulation is in-process and pure Rust. It is not a screen scrape of another multiplexer. Alacritty's parser and terminal grid consume PTY bytes, and Luvus converts the visible grid into its own render cells. [L:src/terminal/vt/alacritty.rs:1-23] [L:src/terminal/vt/alacritty.rs:527-578]

## Component: TUI, rendering, and pointer input

**Name:** TUI, rendering, and pointer input.

**Responsibility:** It renders the full application UI, records clickable geometry, maps terminal input into client messages, and paints server-produced frames. [L:src/ui/mod.rs:8-27] [L:src/ui/mod.rs:114-151] [L:src/ipc/client.rs:191-230] [L:src/app/input.rs:1012-1052]

**Key files:** The declared stack is Ratatui 0.30, Crossterm 0.29, and portable-pty 0.9. `src/ui/` draws the model. `src/ipc/client.rs` handles terminal events and paints cells. `src/app/input.rs` performs hit testing and actions. [L:Cargo.toml:55-60] [L:src/ui/mod.rs:8-27] [L:src/ipc/client.rs:319-360] [L:src/app/input.rs:1012-1052]

**Key types:** Ratatui `Buffer`, `Frame`, and `Rect` are the drawing and geometry types. Crossterm `Event`, `MouseEvent`, and terminal commands are the input and output types. [L:src/ui/mod.rs:8-27] [L:src/main.rs:48-54] [L:src/ipc/client.rs:319-360]

**How it talks to other components:** In server mode, the UI renders into an owned Ratatui buffer. The server sends either a full frame or changed runs. The client uses DEC synchronized updates and directly emits only changed cells. [L:src/ui/mod.rs:114-151] [L:src/ipc/protocol.rs:92-165] [L:src/ipc/client.rs:421-431] [L:src/ipc/client.rs:454-536]

The OpenTUI hypothesis is refuted. The complete dependency block declares Ratatui and Crossterm and does not declare OpenTUI. The source imports and uses Ratatui and Crossterm directly. [L:Cargo.toml:55-118] [L:src/main.rs:48-54] [L:src/ui/mod.rs:8-27]

Crossterm drives mouse capture and mouse events. Click behavior is Luvus code. The renderer stores per-frame `Rect` values for pane titles and controls. Input code checks coordinates against those rectangles for sidebar rows, tabs, panes, files, settings, docks, and bar widgets. Mouse-aware terminal applications can receive unhandled pointer input. [L:src/ipc/client.rs:171-189] [L:src/ipc/client.rs:319-360] [L:src/ui/panes.rs:112-165] [L:src/app/input.rs:1494-1533] [L:src/app/input.rs:1571-1715] [L:src/app/input.rs:2102-2249]

## Component: agent registry, detection, and native sessions

**Name:** Agent registry, detection, and native sessions.

**Responsibility:** It identifies agent processes, derives working, blocked, done, or idle state, discovers native sessions, and constructs resume or fork commands. [L:src/detect.rs:1-26] [L:src/agent.rs:1-13] [L:src/agent.rs:51-116]

**Key files:** `src/agent/types.rs` defines adapter capabilities. `src/agent/registry.rs` registers 18 built-in agents. `src/agent.rs` performs native session discovery. `src/detect.rs` combines process identity, screen rules, hooks, and user manifests. [L:src/agent/types.rs:7-52] [L:src/agent/registry.rs:3-61] [L:src/agent.rs:1-49] [L:src/detect.rs:1-81]

**Key types:** `AgentDescriptor` composes identity, discovery, session, and integration operations. `SessionInfo` describes a discoverable native session. `AgentSession`, `AgentReport`, and `PaneStatus` attach identity and status to panes. [L:src/agent/types.rs:7-52] [L:src/agent.rs:40-49] [L:src/app/mod.rs:1045-1122]

**How it talks to other components:** Detection reads process and terminal evidence from panes. Optional integration reports can override heuristic state. The UI consumes the resulting pane status. Persistence records enough native-session data to resume a supported agent after restore. [L:src/detect.rs:1-26] [L:src/app/mod.rs:1045-1122] [L:src/persist.rs:72-104]

User agent manifests are TOML. They can add identity and screen rules. Loading is deterministic and validates merged definitions. [L:src/detect.rs:700-753] [L:src/detect.rs:768-893] [L:src/persist.rs:584-658]

## Component: orchestration and task ledger

**Name:** Orchestration and task ledger.

**Responsibility:** It tracks dependent tasks, readiness, assignments, heartbeats, path leases, quality state, and merge reservations. It is a separate single-writer state machine. [L:src/orch/mod.rs:1-9] [L:src/orch/mod.rs:16-136] [L:src/orch/mod.rs:210-267] [L:src/orch/mod.rs:581-666]

**Key files:** `src/orch/mod.rs` contains the durable state machine. `src/app/board.rs` contains the board UI and workflow glue. [L:src/orch/mod.rs:1-136] [L:src/app/board.rs:1-2083]

**Key types:** `TaskId`, `TaskStatus`, `Task`, `Lease`, and `OrchState` are the durable model. `Task` includes dependencies and an 85 percent context compaction threshold. [L:src/orch/mod.rs:16-136]

**How it talks to other components:** API and UI actions mutate `OrchState` through the application loop. The board reads the state. Persistence writes it to a separate atomic `orch.json` file and repairs an interrupted merge reservation on load. [L:src/ipc/api.rs:1-28] [L:src/app/board.rs:1-2083] [L:src/orch/mod.rs:669-734]

## Component: Universal Harness Protocol

**Name:** Universal Harness Protocol, or UHP.

**Responsibility:** It is the public, versioned automation contract. UHP 1.0 covers snapshots, events, workspaces, tabs, panes, agents, files, Git, tasks, leases, modules, UI surfaces, and terminal backend tests. The private binary TUI protocol is explicitly outside this public contract. [L:protocol/README.md:1-21] [L:src/api/capabilities.rs:3-184]

**Key files:** `protocol/README.md` defines the boundary. `protocol/uhp/v1/README.md` points to schemas, fixtures, terminal cases, and access rules. `src/api/capabilities.rs` is the canonical method registry. `src/uhp/` implements authenticated transport access. [L:protocol/README.md:1-21] [L:protocol/uhp/v1/README.md:1-19] [L:src/api/capabilities.rs:3-184] [L:src/uhp/mod.rs:17-174]

**Key types:** `AccessMode` maps access modes to scopes. `AccessSession` carries gateway, token, and pairing state. `Gateway` enforces connection, timeout, and request-rate limits. [L:src/uhp/mod.rs:17-92] [L:src/uhp/gateway.rs:15-41]

**How it talks to other components:** A loopback TCP NDJSON endpoint authenticates a caller, then forwards requests to the same application API. The older local API also uses newline-delimited JSON and marshals mutations onto the single application loop. [L:src/uhp/mod.rs:100-174] [L:src/ipc/api.rs:1-28]

The protocol package is intended for consumers, not only for Luvus internals. Its examples include a dependency-free UHP package validator and a separate terminal contract validator. [L:examples/uhp/consumer.py:1-64] [L:examples/uhp/terminal/consumer.py:1-39]

## Component: runtime modules

**Name:** Runtime modules.

**Responsibility:** Modules add subprocess-backed actions, event hooks, startup hooks, panes, docks, bar widgets, and settings without linking Rust code into Luvus. [L:MODULE-GUIDE.md:20-58] [L:src/module/manifest.rs:1-169]

**Key files:** A module directory contains `luvus-module.toml` and scripts or binaries. `src/module/manifest.rs` parses and validates it. `src/module/registry.rs` persists installed modules in `modules.json`. `src/module/runtime.rs` launches commands and captures output. `src/app/modules.rs` connects module results to the UI. [L:MODULE-GUIDE.md:20-55] [L:src/module/manifest.rs:190-218] [L:src/module/registry.rs:1-128] [L:src/module/runtime.rs:1-23] [L:src/app/modules.rs:1-1971]

**Key types:** `ModuleManifest` contains identity, build, startup, actions, events, panes, docks, bars, and settings. `InstalledModule` stores registry state. Runtime invocations have a 64 KiB output cap and bounded log and in-flight collections. [L:src/module/manifest.rs:1-169] [L:src/module/registry.rs:1-42] [L:src/module/runtime.rs:1-23]

**How it talks to other components:** Luvus injects `LUVUS_*` environment values, starts an argv command on a detached thread, captures stdout and stderr concurrently, and accepts callbacks through the running Luvus binary. Startup hooks run after API listening begins. [L:src/module/runtime.rs:52-96] [L:src/module/runtime.rs:118-194] [L:MODULE-GUIDE.md:45-58] [L:src/ipc/server.rs:370-377]

Modules are written in any language and require no SDK. The stable boundary is a TOML manifest, environment variables, argv, command output, and callbacks through `LUVUS_BIN_PATH`. [L:MODULE-GUIDE.md:20-55] [L:examples/modules/README.md:37-54]

## Component: Codex plugin bundle

**Name:** Codex plugin bundle.

**Responsibility:** `plugins/luvus` packages instructions and metadata that teach a Codex host how to use an installed Luvus binary. It is different from a Luvus runtime module. [L:plugins/luvus/.codex-plugin/plugin.json:1-43] [L:plugins/luvus/skills/luvus/SKILL.md:1-10] [L:src/module/manifest.rs:1-50]

**Key files:** `.codex-plugin/plugin.json` declares the plugin name, version, description, interface metadata, and `skills` directory. `skills/luvus/SKILL.md` defines the Luvus operating instructions. Cargo excludes `plugins/` from the published Rust crate. [L:plugins/luvus/.codex-plugin/plugin.json:1-43] [L:plugins/luvus/skills/luvus/SKILL.md:1-10] [L:Cargo.toml:22-31]

**Key types:** There is no Rust type for this bundle in Luvus. Its declared interface is JSON metadata plus a skill directory. [L:plugins/luvus/.codex-plugin/plugin.json:1-43]

**How it talks to other components:** The skill tells the host to select a Luvus session and invoke the installed binary. It says not to add another service, event loop, or polling loop. [L:plugins/luvus/skills/luvus/SKILL.md:1-10] [L:plugins/luvus/skills/luvus/SKILL.md:43-92]

The exact Codex discovery, installation, and loading algorithm is UNKNOWN from the Luvus repository. The manifest declares a Codex-facing plugin and a skill path, while Cargo excludes the bundle from the Rust package. Reading the matching Codex plugin loader or official plugin documentation would resolve the host-side behavior. [L:plugins/luvus/.codex-plugin/plugin.json:1-43] [L:Cargo.toml:22-31]

## Component: persistence and configuration

**Name:** Persistence and configuration.

**Responsibility:** It saves structural session state, selected terminal screen content, application configuration, module registrations, task state, and named-session metadata. [L:src/persist.rs:20-104] [L:src/config.rs:16-96] [L:src/module/registry.rs:1-42] [L:src/orch/mod.rs:669-734] [L:src/session.rs:245-289]

**Key files:** `src/persist.rs` owns session snapshots. `src/config.rs` owns user configuration. `src/module/registry.rs` owns `modules.json`. `src/orch/mod.rs` owns `orch.json`. `src/session.rs` owns named-session metadata and paths. [L:src/persist.rs:20-104] [L:src/config.rs:542-608] [L:src/module/registry.rs:1-128] [L:src/orch/mod.rs:669-734] [L:src/session.rs:185-205] [L:src/session.rs:245-289]

**Key types:** `SessionSnapshot`, `WsSnap`, `TabSnap`, and `PaneSnap` are the restore schema. `PaneSnap` includes working directory, command, name, native agent session, launch flags, visible ANSI screen, and special pane view state. [L:src/persist.rs:20-104]

**How it talks to other components:** Snapshot code walks `App`, caps a saved screen at 256 KiB, writes a temporary file, flushes it, and renames it atomically. Load rejects data from a newer snapshot version. Configuration and module registry saves also use normalized or atomic file updates. [L:src/persist.rs:790-975] [L:src/persist.rs:986-1055] [L:src/config.rs:542-608] [L:src/module/registry.rs:86-128]

This design restores terminal appearance and launch context. It does not preserve a live child process across server death. The snapshot stores commands and screen data, while restore spawns a new pane and `Pane` owns and hangs up the live PTY child. [L:src/persist.rs:72-104] [L:src/app/mod.rs:2516-2620] [L:src/terminal/pty.rs:198-235]

## Component: feature subsystems

**Name:** Feature subsystems.

**Responsibility:** These components implement the user-facing file browser, Git and GitHub views, semantic diff, mission and task boards, settings, themes, updates, and platform integration listed in the README. [L:README.md:23-55]

**Key files:** The principal files and their source cost are listed in the next section. [L:src/app/files.rs:1-3660] [L:src/app/diff.rs:1-2629] [L:src/app/board.rs:1-2083] [L:src/platform.rs:1-1277] [L:src/update.rs:1-756]

**Key types:** Most feature state is embedded in `App`, `Workspace`, and `Tab`, rather than isolated behind independent service objects. Tabs directly carry Git, orchestration, and mission dashboard state. [L:src/app/mod.rs:324-375] [L:src/app/mod.rs:1469-1601]

**How it talks to other components:** Feature actions enter through input, CLI, or API dispatch. Background work returns through `AppEvent`. Results update `App`, and the UI renders them. [L:src/app/input.rs:1-5820] [L:src/app/dispatch.rs:1-7683] [L:src/event.rs:84-126] [L:src/ui/mod.rs:114-151]

## Feature list and source cost

The README groups the product into 13 feature families. The table maps every family to its principal implementation surface. The line counts are shared maintenance surface, not independent totals. [L:README.md:23-55]

| README feature | Principal source cost | Notes |
| --- | --- | --- |
| Persistent workspaces | `app/mod.rs`, 11,974 lines; `persist.rs`, 1,315; `session.rs`, 541; `ipc/server.rs`, 1,500. [L:src/app/mod.rs:1-11974] [L:src/persist.rs:1-1315] [L:src/session.rs:1-541] [L:src/ipc/server.rs:1-1500] | The cost spans model ownership, named sessions, restore, and the background server. [L:README.md:25-26] |
| Pane and tab control | `app/mod.rs`, 11,974 lines; `app/input.rs`, 5,820; `app/dispatch.rs`, 7,683; `layout.rs`, 1,008. [L:src/app/mod.rs:1-11974] [L:src/app/input.rs:1-5820] [L:src/app/dispatch.rs:1-7683] [L:src/layout.rs:1-1008] | This is the largest shared UI and command surface. [L:README.md:27-28] |
| Agent awareness | `detect.rs`, 2,496 lines; `agent.rs`, 1,006; `agent/registry.rs`, 229, plus adapter modules selected by the registry. [L:src/detect.rs:1-2496] [L:src/agent.rs:1-1006] [L:src/agent/registry.rs:1-229] | Detection combines process, screen, hook, and native session evidence. [L:src/detect.rs:1-26] |
| Agent workflows | The same agent surface plus shared `app/mod.rs` and `app/dispatch.rs`. [L:src/agent.rs:1-1006] [L:src/app/mod.rs:1-11974] [L:src/app/dispatch.rs:1-7683] | Resume and fork support is adapter-specific. [L:src/agent/types.rs:7-52] [L:src/agent/registry.rs:175-227] |
| Files and code | `app/files.rs`, 3,660 lines; `app/diff.rs`, 2,629; plus shared dispatch and UI. [L:src/app/files.rs:1-3660] [L:src/app/diff.rs:1-2629] [L:src/app/dispatch.rs:1-7683] | File browsing and diff review are native application modes. [L:README.md:35-36] |
| Git and GitHub | `git/local.rs`, 1,263 lines; `git/github.rs`, 475; plus shared app and dispatch code. [L:src/git/local.rs:1-1263] [L:src/git/github.rs:1-475] [L:src/app/dispatch.rs:1-7683] | Local Git and GitHub data use separate source modules. [L:README.md:37-38] |
| Worktrees and orchestration | `orch/mod.rs`, 1,333 lines; `app/board.rs`, 2,083; plus shared model and dispatch. [L:src/orch/mod.rs:1-1333] [L:src/app/board.rs:1-2083] [L:src/app/mod.rs:1-11974] | The task and lease ledger is a durable component, not only a view. [L:src/orch/mod.rs:1-9] [L:src/orch/mod.rs:96-136] |
| Remote and multi-client | `ipc/server.rs`, 1,500 lines; `ipc/client.rs`, 1,170; `ipc/protocol.rs`, 779; remote routing in `main.rs`. [L:src/ipc/server.rs:1-1500] [L:src/ipc/client.rs:1-1170] [L:src/ipc/protocol.rs:1-779] [L:src/main.rs:609-709] | Each client has an independent viewport and render state. [L:src/ipc/server.rs:207-238] |
| Terminal tools | `terminal/pty.rs`, 1,558 lines; `terminal/vt/mod.rs`, 379; `terminal/vt/alacritty.rs`, 1,857; plus terminal UI code. [L:src/terminal/pty.rs:1-1558] [L:src/terminal/vt/mod.rs:1-379] [L:src/terminal/vt/alacritty.rs:1-1857] [L:src/ui/panes.rs:1-638] | Search, copy, links, scrollback, and full-screen programs share one emulator boundary. [L:README.md:43-44] [L:src/terminal/vt/mod.rs:187-260] |
| Extensible surfaces | `module/manifest.rs`, 790 lines; `module/registry.rs`, 128; `module/runtime.rs`, 223; `app/modules.rs`, 1,971. [L:src/module/manifest.rs:1-790] [L:src/module/registry.rs:1-128] [L:src/module/runtime.rs:1-223] [L:src/app/modules.rs:1-1971] | Much of the cost is UI integration, not subprocess launch. [L:src/module/runtime.rs:118-194] [L:src/app/modules.rs:1-1971] |
| Universal Harness Protocol | `ipc/api.rs`, 3,201 lines; `api/capabilities.rs`, 417; `uhp/mod.rs`, 367; `uhp/gateway.rs`, 1,171, plus schemas and fixtures under `protocol/uhp/v1`. [L:src/ipc/api.rs:1-3201] [L:src/api/capabilities.rs:1-417] [L:src/uhp/mod.rs:1-367] [L:src/uhp/gateway.rs:1-1171] [L:protocol/uhp/v1/README.md:1-19] | The public contract adds auth, limits, compatibility data, fixtures, and a large method registry. [L:src/uhp/gateway.rs:15-41] [L:src/api/capabilities.rs:3-184] |
| Custom interface | `ui/mod.rs`, 1,263 lines; `app/input.rs`, 5,820; `config.rs`, 780; plus shared model and feature views. [L:src/ui/mod.rs:1-1263] [L:src/app/input.rs:1-5820] [L:src/config.rs:1-780] [L:src/app/mod.rs:1-11974] | Layout, key maps, themes, language, and sidebars cross component boundaries. [L:README.md:50-52] |
| Cross-platform delivery | `platform.rs`, 1,277 lines; `platform/windows.rs`, 654; `update.rs`, 756; plus role routing in `main.rs`. [L:src/platform.rs:1-1277] [L:src/platform/windows.rs:1-654] [L:src/update.rs:1-756] [L:src/main.rs:1-3072] | Platform process, path, update, migration, and doctor behavior is a substantial separate surface. [L:README.md:53-55] |

The module examples show the cost of extending those surfaces without core edits. Counts include manifests, scripts, source, and example documentation. The Buzz `.gitignore` is excluded.

| Module example | Physical lines | Surface demonstrated |
| --- | ---: | --- |
| `agent-ping` | 129 | Event hook, secret setting, agent action, and toast. [L:examples/modules/agent-ping/luvus-module.toml:1-38] [L:examples/modules/agent-ping/ping.py:1-91] |
| `branch-dock` | 129 | Dock, startup hook, settings, click payload, and checkout action. [L:examples/modules/branch-dock/luvus-module.toml:1-57] [L:examples/modules/branch-dock/refresh.sh:1-46] [L:examples/modules/branch-dock/checkout.sh:1-26] |
| `ci-bar` | 49 | Bar widget, refresh, click action, and details. [L:examples/modules/ci-bar/luvus-module.toml:1-25] [L:examples/modules/ci-bar/refresh.sh:1-14] [L:examples/modules/ci-bar/details.sh:1-10] |
| `scratch-pane` | 135 | Pane entry point, selection, tab rename, and state. [L:examples/modules/scratch-pane/luvus-module.toml:1-34] [L:examples/modules/scratch-pane/notes.js:1-38] [L:examples/modules/scratch-pane/open.js:1-33] [L:examples/modules/scratch-pane/stash.js:1-30] |
| `file-tree` | 326 | A stateful clickable file tree built outside core. [L:examples/modules/file-tree/README.md:1-52] [L:examples/modules/file-tree/luvus-module.toml:1-76] [L:examples/modules/file-tree/lib.sh:1-41] [L:examples/modules/file-tree/open.sh:1-40] [L:examples/modules/file-tree/render.sh:1-102] [L:examples/modules/file-tree/toggle.sh:1-15] |
| `deck` | 816 | A Python presentation pane and sample deck. [L:examples/modules/deck/README.md:1-76] [L:examples/modules/deck/luvus-module.toml:1-50] [L:examples/modules/deck/deck.py:1-611] [L:examples/modules/deck/present.sh:1-4] [L:examples/modules/deck/sample.md:1-75] |
| `esp-idf` | 935 | A larger tool integration with docks, pane commands, device state, and tests. [L:examples/modules/esp-idf/README.md:1-161] [L:examples/modules/esp-idf/luvus-module.toml:1-166] [L:examples/modules/esp-idf/dock.sh:1-128] [L:examples/modules/esp-idf/flash-monitor.sh:1-48] [L:examples/modules/esp-idf/idf.sh:1-19] [L:examples/modules/esp-idf/lib.sh:1-66] [L:examples/modules/esp-idf/menuconfig-pane.sh:1-40] [L:examples/modules/esp-idf/monitor.sh:1-16] [L:examples/modules/esp-idf/partitions-pane.sh:1-46] [L:examples/modules/esp-idf/select-device.sh:1-14] [L:examples/modules/esp-idf/set-target.sh:1-13] [L:examples/modules/esp-idf/test/fake-idf/bin/idf.py:1-22] [L:examples/modules/esp-idf/test/fake-idf/export.sh:1-2] [L:examples/modules/esp-idf/test/run.sh:1-168] [L:examples/modules/esp-idf/toggle.sh:1-26] |
| `buzz` | 1,442 | A compiled Rust TUI, relay, build step, actions, and documentation. [L:examples/modules/buzz/Cargo.toml.example:1-31] [L:examples/modules/buzz/README.md:1-72] [L:examples/modules/buzz/luvus-module.toml:1-67] [L:examples/modules/buzz/src/main.rs:1-257] [L:examples/modules/buzz/src/relay.rs:1-350] [L:examples/modules/buzz/src/tui.rs:1-665] |

There is documentation drift. `MODULE-GUIDE.md` says there are three worked examples. `examples/modules/README.md` says five. The tree contains those five plus Buzz, Deck, and ESP-IDF. [L:MODULE-GUIDE.md:60-71] [L:examples/modules/README.md:1-16] [L:examples/modules/buzz/luvus-module.toml:1-67] [L:examples/modules/deck/luvus-module.toml:1-50] [L:examples/modules/esp-idf/luvus-module.toml:1-166]

The separate preview example documents a terminal-native, offline Markdown preview with Mermaid and safe raw HTML handling. Its documentation is 42 lines. Two included Mermaid samples add 15 lines. [L:examples/preview/README.md:1-42] [L:examples/preview/agent-session.mermaid:1-10] [L:examples/preview/workflow.mmd:1-5]

## Performance model

Luvus does not declare Tokio, async-std, or another async runtime in its complete dependency list. It uses standard threads, locks, and channels for concurrent work. [L:Cargo.toml:55-118] [L:src/ipc/server.rs:5-13] [L:src/terminal/pty.rs:480-535]

PTY reads use an 8 KiB buffer. A pane permits only one outstanding `PtyData` wake, so a hot PTY cannot flood the main event queue with one event per read. [L:src/terminal/pty.rs:1192-1223]

The server drains queued events after a blocking receive. It coalesces render causes and suppresses immediate redraw for hidden PTY output. [L:src/ipc/server.rs:406-458] [L:src/ipc/server.rs:137-170]

Active rendering targets about 60 frames per second with a 16 ms interval. Idle intervals increase to 100 or 250 ms. The server renders only when dirty and rearms PTY repaint state after a render. [L:src/ipc/server.rs:26-45] [L:src/ipc/server.rs:384-427] [L:src/ipc/server.rs:586-620]

Each client keeps one pending frame. If a slow client drops a pending frame, the server marks it for a full resync. Normal updates are runs of changed cells, and the client emits only those changes inside a synchronized terminal update. [L:src/ipc/server.rs:172-204] [L:src/ipc/protocol.rs:132-165] [L:src/ipc/client.rs:421-431] [L:src/ipc/client.rs:454-536]

The API bounds retained events, replay, active connections, worker stack sizes, and module output. This limits memory and thread costs at explicit boundaries. [L:src/ipc/api.rs:30-75] [L:src/module/runtime.rs:1-23]

Release builds use thin LTO and strip symbols. The manifest also patches the local VTE and terminal crates, with a note that the VTE patch avoids a lazy 2 MiB allocation. [L:Cargo.toml:120-129]

## Confirmed differences from Herdr

This section compares only the two checked-out reference clones. An absent public method means the exhaustive public method registry in that clone does not expose it. It does not prove that no private helper exists.

1. Luvus publishes a versioned UHP 1.0 package with schemas, fixtures, access rules, and a terminal backend namespace. Herdr exposes its own API method enum, but its exhaustive enum has no UHP package or terminal backend namespace. [L:protocol/README.md:1-21] [L:protocol/uhp/v1/README.md:1-19] [L:src/api/capabilities.rs:3-184] [H:src/api/schema.rs:45-243]

2. Luvus exposes a built-in task ledger with dependencies, assignments, quality state, merge reservations, and path leases. Its public registry includes `task.*` and `lease.*`. Herdr's exhaustive method enum has no task or lease namespace. [L:src/orch/mod.rs:16-136] [L:src/orch/mod.rs:210-380] [L:src/orch/mod.rs:581-666] [L:src/api/capabilities.rs:119-133] [H:src/api/schema.rs:45-243]

3. Luvus has public methods and native views for files, Git history and hosting data, semantic diffs, and mission state. Herdr's public methods cover server, workspace, worktree, tab, agent, pane, events, integration, and plugin domains, but not those Luvus domains. Herdr still has worktree commands and workspace Git metadata, so this is a difference in breadth, not an absence of Git awareness. [L:src/api/capabilities.rs:89-118] [L:src/app/files.rs:1-3660] [L:src/app/diff.rs:1-2629] [H:src/api/schema.rs:3-27] [H:src/api/schema.rs:45-243] [H:src/cli/spec.rs:235-277] [H:src/workspace.rs:178-196]

4. A Luvus runtime module can declare docks, top or bottom bar widgets, and typed settings. A Herdr plugin can declare build, startup, actions, events, panes, and link handlers, but its plugin schema has no dock, bar, or settings field. Herdr has link handlers that the Luvus manifest does not list, so neither extension surface is a strict superset. [L:src/module/manifest.rs:1-169] [H:src/api/schema/plugins.rs:37-68] [H:src/api/schema/plugins.rs:229-289] [H:src/app/api/plugins/manifest.rs:11-34] [H:src/app/api/plugins/manifest.rs:118-227]

5. Luvus uses standard threads and channels and has no declared async runtime. Herdr declares Tokio with the multi-thread runtime and uses a different concurrency base. [L:Cargo.toml:55-118] [L:src/ipc/server.rs:5-13] [H:Cargo.toml:23-49]

Both projects already share Ratatui 0.30, Crossterm 0.29, portable-pty 0.9, a client/server model, panes and tabs, agent support, worktrees, and subprocess extensions. These are not Luvus-only features. [L:Cargo.toml:55-60] [H:Cargo.toml:23-49] [L:src/main.rs:64-124] [H:src/cli/spec.rs:5-46] [L:src/module/manifest.rs:1-169] [H:src/api/schema/plugins.rs:229-289]

## Takeaways for our tool

### Copy

- Keep one authoritative mutable model and marshal all mutations onto its event loop. This makes API, TUI, PTY, and persistence ordering explicit. [L:src/ipc/server.rs:1-3] [L:src/ipc/api.rs:1-28] [L:src/event.rs:1-126]
- Put terminal emulation behind a narrow engine interface. This isolates the Alacritty grid from the rest of the product. [L:src/terminal/vt/mod.rs:1-6] [L:src/terminal/vt/mod.rs:187-260] [L:src/terminal/vt/alacritty.rs:1-23]
- Coalesce PTY wakes and render requests. Keep one bounded pending frame per client and force a resync after a drop. [L:src/terminal/pty.rs:1192-1223] [L:src/ipc/server.rs:137-204]
- Separate the public automation contract from the private display protocol. Ship schemas and fixtures with the public contract. [L:protocol/README.md:1-21] [L:protocol/uhp/v1/README.md:1-19]
- Use argv and environment variables as the small extension ABI. Keep extension output, logs, and concurrency bounded. [L:MODULE-GUIDE.md:20-55] [L:src/module/runtime.rs:1-23] [L:src/module/runtime.rs:52-96]
- Save durable coordination state separately from UI snapshots. Use atomic replacement and recovery rules. [L:src/orch/mod.rs:669-734] [L:src/persist.rs:986-1055]

### Avoid

- Avoid concentrating unrelated feature state in one 11,974-line `App` file and routing in a 7,683-line dispatch file. The current shape makes feature ownership hard to isolate. [L:src/app/mod.rs:1-11974] [L:src/app/dispatch.rs:1-7683]
- Avoid making pointer behavior depend on many manually retained rectangles in a 5,820-line input module. Prefer typed hit targets and local event handlers. [L:src/app/input.rs:1-5820] [L:src/ui/panes.rs:112-165]
- Avoid claiming live process persistence when the saved object is launch context plus a screen snapshot. State the restore boundary precisely. [L:src/persist.rs:72-104] [L:src/terminal/pty.rs:198-235]
- Avoid documentation counts that are not generated from the example tree. The current guide says three examples, while the examples README says five and the tree shows eight documented modules. [L:MODULE-GUIDE.md:60-71] [L:examples/modules/README.md:1-16]
- Avoid letting a public method list, protocol schemas, CLI help, and implementation become independent sources of truth. Luvus names `src/api/capabilities.rs` as canonical, while the protocol package also carries schemas and fixtures that must remain aligned. [L:src/api/capabilities.rs:3-184] [L:protocol/uhp/v1/README.md:1-19]

### Open questions

- Should our first version use threads and channels like Luvus or an async runtime like Herdr? The answer needs expected client count, PTY count, API workload, and profiling data. [L:Cargo.toml:55-118] [H:Cargo.toml:23-49]
- Which state must survive a server crash: layout only, visible terminal history, agent sessions, or live child processes? Luvus preserves the first three forms of data but not the live child. [L:src/persist.rs:20-104] [L:src/terminal/pty.rs:198-235]
- Do we need a broad UHP-style contract in version one, or can we expose a smaller method set and add compatibility fixtures later? Luvus's current registry and gateway are already a large surface. [L:src/api/capabilities.rs:3-184] [L:src/uhp/gateway.rs:1-1171]
- Are docks, bars, settings, and link handlers all required extension points? Luvus and Herdr chose overlapping but different plugin surfaces. [L:src/module/manifest.rs:1-169] [H:src/api/schema/plugins.rs:229-289]
- How should the Codex plugin bundle be discovered and installed? This remains UNKNOWN from the Luvus repository. Reading the Codex host loader or official plugin documentation would resolve it. [L:plugins/luvus/.codex-plugin/plugin.json:1-43]
