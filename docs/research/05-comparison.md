# Herdr and Luvus comparison

## Decision

Build the first release as one Rust binary with a headless server and a thin
terminal client. The server owns all durable state, PTYs, terminal grids, and
agent state. The client owns terminal setup, layout projection, hit testing, and
painting.

Use Ratatui 0.30 and Crossterm 0.29 for the first frontend. Keep both crates out
of core types. Use an Alacritty-based terminal engine as the baseline behind a
neutral engine trait. Do not lock that engine choice until it passes a shared
terminal compliance corpus.

Keep version one narrow. It must provide persistent sessions, workspaces, tabs,
split panes, correct terminal behavior, mouse and keyboard input, agent status,
agent control, and a small versioned local API. Add remote use, extensions,
orchestration, broad dashboards, and GPUI later.

Do not promise Herdr binary, skill, socket, ID, or plugin source compatibility.
Herdr is a design reference. Compatibility would make its full current CLI and
plugin callback surface part of our permanent contract.

These choices combine Herdr's cleaner frontend boundary and status arbitration
with Luvus's terminal trait, detection-only manifests, single-writer state, and
versioned public protocol.

## Evidence scope and correction

The claims below were checked against these read-only clones:

- `H:` is `/home/nethum/Projects/_research/herdr` at
  `2290257acb2085ce6842ba5c7e3ca50c3ba64f02`.
- `L:` is `/home/nethum/Projects/_research/luvus` at
  `d1013d16f48cdd724b8df40c7c4c83dc306dc5d6`.

The prior reports are cited by file and section. Clone paths then show the
source used to verify the claim. Cost is relative:

- Medium: bounded work with known libraries and local interfaces.
- High: cross-platform runtime work or a long correctness tail.
- Very high: several hard subsystems with public compatibility or recovery
  requirements.

One prior claim needs correction. [02-herdr-extensibility.md, "Current Luvus
implementation"](02-herdr-extensibility.md#component-current-luvus-implementation)
says that a new Luvus agent kind must be inserted into `BUILTINS`. That is true
only for native capabilities. A user manifest can add a new detection-only agent
without a binary release. Native session discovery, resume, fork, usage, and
integrations still require reviewed Rust code. The source says this directly
and tests an unknown `nimbus` agent. (`L:src/detect.rs:700-856`,
`L:src/detect.rs:2394-2435`,
`L:website/src/content/docs/docs/explanation/architecture.mdx:30-47`,
`L:website/src/content/docs/docs/extend/adding-agent-support.mdx:6-38`)

## Component matrix

| Component | Herdr approach | Luvus approach | Cost | Our pick with a reason |
| --- | --- | --- | --- | --- |
| Process model | One binary. The default starts or attaches to a detached server, then runs a thin client. `--no-session` is a monolithic escape hatch. It uses a multi-thread Tokio runtime. See [01, Components 1 and 2](01-herdr-architecture.md#component-1-package-and-process-entry-point). Verified at `H:src/main.rs:568-596`, `H:src/main.rs:815-910`, and `H:Cargo.toml:23-49`. | One binary. A detached server owns one mutable `App`. Thin clients attach. A local monolith is an escape hatch. Standard threads and channels do slow work around one single-writer loop. See [03, process router and server](03-luvus-architecture.md#component-process-router-and-named-sessions). Verified at `L:src/main.rs:64-124`, `L:src/main.rs:474-506`, `L:src/ipc/server.rs:248-458`, and `L:website/src/content/docs/docs/explanation/architecture.mdx:8-28`. | High | Use a detached server and disposable clients. Use one state owner with bounded standard channels first. This gives detach and future frontend support without making async part of every core interface. |
| IPC | A public newline-delimited JSON API and a separate versioned bincode client protocol use two local sockets. Client input is converted to Herdr-owned types. The client can receive semantic cells or ANSI. See [01, Components 7 and 8](01-herdr-architecture.md#component-7-public-cli-and-json-api) and [04, Herdr terminal client](04-rendering-and-gpui.md#component-herdr-terminal-client-and-wire-protocol). Verified at `H:src/session.rs:157-185`, `H:src/api/schema.rs:33-45`, and `H:src/protocol/wire.rs:15-44`. | A local JSON API, versioned UHP, and a private binary client stream all reach the same app loop. The private protocol exposes Crossterm and Ratatui types. See [03, server and UHP](03-luvus-architecture.md#component-server-client-ipc-and-event-loop) and [04, Luvus terminal client](04-rendering-and-gpui.md#component-luvus-terminal-client-and-wire-protocol). Verified at `L:src/ipc/api.rs:1-28`, `L:src/ipc/protocol.rs:1-60`, and `L:protocol/README.md:1-21`. | Very high | Use a small versioned JSON control API plus a separate bounded frontend stream. Both must serialize core-owned types. Publish one schema and fixture set. Do not expose frontend library types. |
| Layout | A BSP tree lives with pane view state in each tab. Live terminal runtimes live in a registry outside the tab. See [01, Component 3](01-herdr-architecture.md#component-3-workspace-tab-pane-and-terminal-state). Verified at `H:src/layout.rs:72-172` and `H:src/workspace/tab.rs:38-52`. | A BSP tree lives in each tab. Live panes and native views are flat maps in `App`. Persistence remaps saved leaves to new runtime pane IDs. See [03, pane layout](03-luvus-architecture.md#component-pane-layout). Verified at `L:src/layout.rs:1-203` and `L:src/layout.rs:590-655`. | Medium | Use a pure BSP tree with stable leaf and split IDs. Keep pane runtime ownership in a flat registry. Keep cell and pixel rectangles in each frontend projection. |
| PTY | `portable-pty` is pinned and patched. A pane runtime owns a PTY actor, terminal, process state, and detection task. Unix uses one actor thread per pane. See [01, Component 4](01-herdr-architecture.md#component-4-pty-and-child-process-runtime). Verified at `H:Cargo.toml:32-51`, `H:src/pane.rs:1040-1066`, and `H:src/pty/actor/unix.rs:351-495`. | `portable-pty` creates the PTY. Reader, writer, and reaper work uses standard threads. A `Pane` owns the engine and process lifecycle. See [03, PTY ownership](03-luvus-architecture.md#component-pty-ownership-and-terminal-emulation). Verified at `L:src/terminal/pty.rs:150-235`, `L:src/terminal/pty.rs:480-535`, and `L:src/terminal/pty.rs:1192-1223`. | High | Put `portable-pty` behind a narrow PTY service. Give each pane a bounded input path and at most one pending output wake. Isolate all platform code. Do not put detection or frontend state in the PTY type. |
| Terminal engine | A concrete vendored Ghostty engine is built with Zig and linked through FFI. It exposes strong dirty-region, mouse, hyperlink, and image behavior. See [01, Component 5](01-herdr-architecture.md#component-5-terminal-emulation-scrollback-and-rendering) and [04, Herdr terminal emulator](04-rendering-and-gpui.md#component-herdr-terminal-emulator). Verified at `H:build.rs:32-95` and `H:src/pane/terminal.rs:165-215`. | A `VtEngine` trait wraps a vendored, pure Rust Alacritty terminal. The trait is useful, but its render cell still contains Ratatui style types. See [03, PTY ownership](03-luvus-architecture.md#component-pty-ownership-and-terminal-emulation) and [04, Luvus terminal boundary](04-rendering-and-gpui.md#component-luvus-terminal-emulator-boundary). Verified at `L:src/terminal/vt/mod.rs:108-116`, `L:src/terminal/vt/mod.rs:187-260`, and `L:src/terminal/vt/alacritty.rs:1-23`. | Very high | Start with Alacritty behind our own neutral trait. It avoids Zig and FFI. Keep Ghostty as a measured fallback. The compliance corpus, not preference, makes the final choice. |
| TUI | Ratatui 0.30 and Crossterm 0.29. A view pass computes geometry. A render pass reads state. The thin client converts host events at the edge. See [01, Component 9](01-herdr-architecture.md#component-9-tui-and-mouse-input) and [04, Herdr view](04-rendering-and-gpui.md#component-herdr-view-renderer-and-pointer-routing). Verified at `H:Cargo.toml:28-37`, `H:src/ui.rs:108-156`, and `H:src/ui.rs:389-462`. | Ratatui 0.30 and Crossterm 0.29. The server renders a Ratatui buffer per client. Input and protocol types retain frontend library values. See [03, TUI](03-luvus-architecture.md#component-tui-rendering-and-pointer-input). Verified at `L:Cargo.toml:55-60`, `L:src/ui/mod.rs:114-190`, and `L:src/ipc/protocol.rs:1-60`. | High | Use Ratatui and Crossterm only in `frontend-tui`. The TUI receives neutral snapshots and sends semantic commands. This keeps a later GPUI frontend possible. |
| Mouse input | Computed `ViewState` geometry is shared by rendering and hit testing. Mouse handling returns small semantic actions. See [04, Herdr view](04-rendering-and-gpui.md#component-herdr-view-renderer-and-pointer-routing). Verified at `H:src/app/state.rs:862-885` and `H:src/app/input/mouse.rs:28-64`. | Rendering writes many hit rectangles into shared `App` state. The server must render the interacting client before it handles input. See [04, Luvus renderer](04-rendering-and-gpui.md#component-luvus-renderer-layout-and-pointer-routing). Verified at `L:src/ui/mod.rs:407-475`, `L:src/ui/mod.rs:674-706`, and `L:src/ipc/server.rs:690-747`. | High | Follow Herdr. Each frontend owns one size-specific geometry projection. Hit testing produces semantic focus, resize, selection, scroll, and child-pointer commands. |
| Agent detection | A closed `Agent` enum identifies known processes. Screen manifests can update rules only for those kinds. Process, screen, OSC, and hook evidence meet in one status arbiter with transition holds. See [01, Component 6](01-herdr-architecture.md#component-6-agent-recognition-and-state-detection) and [02, agent registry](02-herdr-extensibility.md#component-agent-kind-registry-and-detection). Verified at `H:src/detect/mod.rs:41-221`, `H:src/detect/manifest.rs:138-261`, and `H:src/terminal/state.rs:1788-1857`. | Built-in descriptors define trusted native capabilities. Managed or user TOML can refine identity or add a detection-only kind. Detection combines process, screen, and hook evidence. See [03, agent registry](03-luvus-architecture.md#component-agent-registry-detection-and-native-sessions). Verified at `L:src/agent/types.rs:7-52`, `L:src/agent/registry.rs:1-61`, and `L:src/detect.rs:700-856`. | Very high | Use Luvus's two-tier registry. Data may add detection-only kinds. Compiled adapters grant native operations. Use Herdr's explicit evidence authority and stabilized state transitions. |
| Persistence | Versioned atomic JSON stores structure and launch data. Optional terminal history is separate and off by default. A cold restart does not preserve live processes. See [01, Component 10](01-herdr-architecture.md#component-10-persistence). Verified at `H:src/persist/snapshot.rs:11-136`, `H:src/persist/io.rs:44-75`, and `H:docs/next/website/src/content/docs/session-state.mdx:8-50`. | Versioned atomic JSON stores structure, launch data, native agent session data, and a bounded visible ANSI screen. Orchestration has a separate durable file. A cold restart respawns panes. See [03, persistence](03-luvus-architecture.md#component-persistence-and-configuration). Verified at `L:src/persist.rs:17-104`, `L:src/persist.rs:930-1054`, and `L:src/orch/mod.rs:1-9`. | Very high | Store versioned structure, stable IDs, launch context, and bounded optional history. Keep secrets in mind and default history off. Put future coordination data in a separate file. Never claim that cold restore preserves a live child. |
| Extensions | Executable plugins declare build steps, actions, events, panes, and link handlers. Install supports preview, pinning, and exact commit records. Plugins can call the full CLI and socket API. See [02, plugin declaration and runtime](02-herdr-extensibility.md#component-plugin-declaration-and-registry). Verified at `H:src/api/schema/plugins.rs:229-289` and `H:src/app/api/plugins/runtime.rs:39-180`. | Executable modules declare actions, events, panes, docks, bars, settings, startup, and build steps. They call back through the same public API. See [03, runtime modules](03-luvus-architecture.md#component-runtime-modules). Verified at `L:src/module/manifest.rs:11-169` and `L:src/module/runtime.rs:52-194`. | Very high | Add extensions later. Start with a versioned subprocess ABI for actions, events, and panes. Keep frontend-specific docks and bars out of the first contract. Bound output and concurrency. |
| Orchestration | Agents can create panes, prompt peers, wait for state, and use worktrees. There is no durable task or lease namespace in the public method enum. See [02, skill control surface](02-herdr-extensibility.md#component-herdr-skill-control-surface) and [03, confirmed differences](03-luvus-architecture.md#confirmed-differences-from-herdr). Verified at `H:src/api/schema.rs:45-243`. | A separate durable state machine owns tasks, dependencies, workers, path leases, quality gates, and merge recovery. See [03, orchestration](03-luvus-architecture.md#component-orchestration-and-task-ledger). Verified at `L:src/orch/mod.rs:1-150` and `L:website/src/content/docs/docs/guides/orchestration.mdx:18-43`. | Very high | Add a Luvus-style ledger later as a separate core service and persistence file. Do not put worktree and merge policy into the v1 terminal runtime. |

## Conflicts and picks

| Conflict | Tradeoff | Pick |
| --- | --- | --- |
| Tokio versus standard threads | Tokio gives useful async coordination but makes runtime types and shutdown more complex. Standard threads fit PTY and filesystem work but need strict bounds. | Start with standard threads, bounded channels, and one state owner. Add an async transport only if measured load needs it. |
| Ghostty versus Alacritty | Ghostty has strong terminal features but adds Zig, FFI, and concrete coupling. Alacritty is pure Rust but Luvus carries local patches and some gaps may appear in the corpus. | Start with Alacritty behind a neutral trait. Keep the trait small enough to test Ghostty with the same corpus. |
| Core-owned input versus frontend types on the wire | Herdr converts input at the client edge. Luvus sends Crossterm and Ratatui values through IPC. Frontend types are easy now but block another renderer later. | Use core-owned key, pointer, cell, style, and cursor values on every seam. |
| Frontend-local geometry versus render-time global geometry | Herdr computes a reusable view projection. Luvus writes hit rectangles into shared state and must render before input. | Store geometry per frontend and viewport. The core stores only split orientation and ratio. |
| Closed agent kinds versus data-driven detection | Herdr can update rules but cannot add a process identity without a binary. Luvus can add detection-only identity in TOML while native powers remain compiled. | Allow data-driven detection-only agents. Require compiled adapters for filesystem access, command construction, resume, fork, usage, and integrations. |
| Small public API versus broad UHP | Herdr's plugin surface can reach a broad CLI and socket API without a bounded compatibility subset. Luvus has a clear UHP version but a large registry, gateway, schema, and fixture cost. | Version a small v1 API for topology, terminal reads, agent control, snapshots, and events. Grow it only with fixtures and capability discovery. |
| Narrow plugins versus UI-rich modules | Herdr has link handlers. Luvus has docks, bars, and typed settings. Neither surface is a strict superset. Every added surface increases compatibility and trust cost. | Delay extensions. First define a UI-neutral subprocess ABI. Add optional frontend surfaces only after two real modules need them. |
| No task ledger versus built-in orchestration | Herdr stays focused on terminal and agent coordination. Luvus adds large task, Git, worktree, lease, gate, and merge surfaces. | Keep orchestration out of v1. Later, add a separate ledger that uses the same core commands and events. |
| Optional separate history versus embedded visible screen | Herdr separates optional history because terminal output can hold secrets. Luvus stores a capped visible ANSI screen in the main pane snapshot. | Keep structural state and optional history separate. Default history off. Cap and redact any saved terminal data. |
| Exact Herdr compatibility versus a new contract | Compatibility would require Herdr command shapes, environment, IDs, response objects, lifecycle errors, and an unbounded plugin callback surface. | Do not claim compatibility. Reuse good semantics under a new versioned contract. |

## Feature matrix

The matrix is the union of the product features advertised in both READMEs and
their main English docs. `Must have v1` means it is part of the first shippable
product. `Later` means the architecture must leave room for it. `Skip` means it
is outside the planned product or should be supplied by another tool.

| Feature | Reference evidence | Decision | Reason |
| --- | --- | --- | --- |
| One Rust binary | Herdr promises one Rust binary. Luvus ships one binary. (`H:README.md:31-37`, `H:Cargo.toml:1-5`, `L:Cargo.toml:1-47`) | Must have v1 | One artifact simplifies install, version checks, and client/server matching. |
| macOS, Linux, and Windows | Both projects carry cross-platform PTY, IPC, and input paths. (`H:README.md:41-47`, `L:README.md:53-55`) | Must have v1 | Platform behavior affects every low-level interface. It must be designed in, not added after the API freezes. |
| Detached server and reattach | Both default to a server that owns PTYs while clients detach. (`H:README.md:31`, `H:docs/next/website/src/content/docs/concepts.mdx:65-77`, `L:README.md:25-26`, `L:website/src/content/docs/docs/explanation/architecture.mdx:8-20`) | Must have v1 | This is the core user value and the reason to separate core from frontend. |
| Workspaces, tabs, and BSP panes | Both expose workspaces, tabs, split panes, focus, resize, move, and close. (`H:docs/next/website/src/content/docs/concepts.mdx:8-24`, `L:README.md:25-28`) | Must have v1 | This is the minimum useful agent workspace. |
| Stable opaque object IDs | Herdr uses stable non-reused public handles. Luvus uses stable public workspace and tab IDs plus runtime pane IDs. (`H:src/workspace.rs:106-204`, `L:src/ids.rs:1-45`) | Must have v1 | APIs, restore, and events need identity that does not depend on list position. |
| Full-screen terminal apps | Both run real PTYs through an in-process terminal engine. (`H:README.md:34`, `H:src/pane/terminal.rs:165-215`, `L:README.md:43-44`, `L:src/terminal/vt/alacritty.rs:1-23`) | Must have v1 | Agents use alternate screen, Unicode, mouse reporting, OSC, and complex input. |
| Scrollback, selection, and copy | Herdr supports selection and host scrollback. Luvus advertises scrollback memory and copy mode. (`H:docs/next/website/src/content/docs/concepts.mdx:26-35`, `L:README.md:43-44`) | Must have v1 | A terminal manager without safe output review is not usable. |
| Keyboard and mouse control | Herdr advertises both as first-class. Luvus exposes pane control through mouse, TUI, and CLI. (`H:README.md:35`, `L:README.md:27-28`) | Must have v1 | Mouse behavior also forces the correct frontend geometry seam. |
| Agent identity and live status | Both detect agents and show blocked, working, done, idle, or unknown state. (`H:README.md:32-34`, `H:docs/next/website/src/content/docs/concepts.mdx:37-49`, `L:README.md:29-31`) | Must have v1 | Status is the main advantage over a generic terminal multiplexer. |
| Detection-only custom agents | Herdr manifests only update known kinds. Luvus manifests can add a new detection-only kind. (`H:docs/next/website/src/content/docs/agents.mdx:64-78`, `L:src/detect.rs:700-856`) | Must have v1 | Fast-moving agent CLIs must not require a release for safe identity and screen rules. |
| Agent list, start, prompt, wait, read, and send keys | Herdr's CLI and socket API expose these operations. Luvus advertises the same core workflow. (`H:README.md:33`, `H:docs/next/website/src/content/docs/socket-api.mdx:36-47`, `L:README.md:32-34`) | Must have v1 | This is the minimum automation surface for agent-to-agent work. |
| Small versioned local API | Herdr exposes CLI and socket control. Luvus exposes versioned UHP with schemas, snapshots, events, and waits. (`H:docs/next/website/src/content/docs/socket-api.mdx:6-34`, `L:README.md:47-49`, `L:website/src/content/docs/docs/guides/uhp.mdx:6-30`) | Must have v1 | The TUI, CLI, and agents need one ordered control model. Keep the v1 method list small. |
| Atomic cold restore | Both save layout and launch context. Neither preserves arbitrary live children after a full server death. (`H:docs/next/website/src/content/docs/session-state.mdx:8-37`, `L:src/persist.rs:17-104`) | Must have v1 | Restore is part of persistent sessions. The contract must clearly distinguish detach from cold restart. |
| Minimal config and key remapping | Herdr documents config and keyboard modes. Luvus advertises remapped keys and prefixes. (`H:README.md:57-59`, `H:docs/next/website/src/content/docs/concepts.mdx:79-85`, `L:README.md:50-52`) | Must have v1 | Users need shell, prefix, key, mouse, theme, and history policy without recompiling. |
| Named sessions | Both provide independent named server namespaces. (`H:docs/next/website/src/content/docs/concepts.mdx:51-63`, `L:website/src/content/docs/docs/explanation/architecture.mdx:16-20`) | Later | One default session proves the ownership model. Names add routing, locks, discovery, and lifecycle cases. |
| Multiple clients with independent viewports | Both servers support several clients. Luvus advertises independent sizes. (`H:docs/next/website/src/content/docs/concepts.mdx:65-71`, `L:README.md:41-42`) | Later | It adds per-client focus, geometry, frame state, and PTY-size lease rules. Keep the protocol ready for it. |
| SSH remote attach | Both docs describe a thin client over SSH. (`H:docs/next/website/src/content/docs/persistence-remote.mdx:38-68`, `L:README.md:41-42`, `L:website/src/content/docs/docs/guides/remote.mdx:1-27`) | Later | Local server/client correctness must be stable first. The same neutral frontend stream should support SSH. |
| Direct terminal observe and control | Herdr has direct attach, observers, controllers, and takeover. Luvus UHP has terminal observe and control leases. (`H:docs/next/website/src/content/docs/persistence-remote.mdx:103-153`, `L:website/src/content/docs/docs/guides/uhp.mdx:72-89`) | Later | It is useful for harnesses, but it expands leases, stream recovery, and security. |
| Native agent session resume and fork | Herdr can restore supported native sessions. Luvus advertises resume and selected forks. (`H:docs/next/website/src/content/docs/session-state.mdx:48-89`, `L:README.md:32-34`, `L:README.md:86-104`) | Later | Each adapter reads private native stores and builds commands. It needs agent-specific fixtures and review. |
| Precise agent hooks and integrations | Both improve heuristic status with optional hooks or plugins. (`H:docs/next/website/src/content/docs/agents.mdx:99-108`, `L:README.md:86-104`) | Later | Process and screen detection must work first. Hooks add third-party lifecycle and installer risk. |
| Notifications, sound, usage, cost, and context | Herdr has notifications. Luvus advertises sound and Mission Control data. (`H:src/protocol/wire.rs:659-755`, `L:README.md:29-31`) | Later | These are useful projections over stable agent events. They are not core runtime requirements. |
| Search, link detection, and terminal images | Both have advanced terminal reads. Luvus advertises history search and links. Herdr carries hyperlink and Kitty image data. (`L:README.md:43-44`, `H:src/pane/terminal.rs:390-506`) | Later | They have separate correctness, memory, security, and platform costs. |
| Runtime extensions | Herdr advertises plugins. Luvus advertises modules with actions, events, settings, panes, docks, and bars. (`H:README.md:36`, `L:README.md:45-46`) | Later | First stabilize the API and event model that extensions would consume. |
| Extension marketplace | Herdr and Luvus both index GitHub topics. Neither index is a trust boundary. (`H:docs/next/website/src/content/docs/marketplace.mdx:6-35`, `L:website/src/content/docs/docs/extend/using-modules.mdx:14-29`) | Skip | A marketplace has little value before a stable extension ABI and a real ecosystem. Direct pinned installs are enough later. |
| Basic Git worktree creation | Herdr exposes worktree commands. Luvus connects worktrees to workers. (`H:src/cli/spec.rs:235-277`, `L:README.md:39-40`) | Later | Worktrees help isolation, but they are not needed to prove the terminal and agent runtime. |
| Task ledger, leases, quality gates, and merge gate | Luvus implements the full workflow. Herdr has no matching task or lease namespace. (`L:README.md:39-40`, `L:website/src/content/docs/docs/guides/orchestration.mdx:18-43`, `H:src/api/schema.rs:45-243`) | Later | This is a product inside the product. Build it only after core events, identity, and persistence are stable. |
| Built-in file browser and preview | Luvus advertises file and code views. Herdr leaves these to terminals and plugins. (`L:README.md:35-36`, `H:README.md:29-37`) | Skip | Editors and later extensions already solve this. It would expand the core model and UI far beyond v1. |
| Built-in Git and GitHub dashboards | Luvus advertises status, branches, commits, pull requests, issues, and activity. Herdr has worktree and workspace Git awareness but no matching dashboard API. (`L:README.md:37-38`, `H:src/api/schema.rs:45-243`) | Skip | Keep Git as a CLI or extension concern. Do not put network and hosting state in core. |
| Built-in semantic diff review | Luvus has native diff views and notes. Herdr has no matching public domain. (`L:src/app/diff.rs:1-2629`, `H:src/api/schema.rs:45-243`) | Skip | This competes with editors and review tools and is not needed for agent terminal coordination. |
| Rich sidebars, docks, bars, and community themes | Luvus advertises a large customizable interface. Herdr has a smaller sidebar and plugin panes. (`L:README.md:45-52`, `H:README.md:36`) | Later | A simple status sidebar is enough for v1. Rich surfaces should consume neutral snapshots. |
| Localization and mobile layout | Luvus advertises eight languages and a compact narrow-screen switcher. (`L:README.md:41-42`, `L:README.md:50-52`) | Later | Both require a stable information architecture and dedicated testing. |
| Self-update, migration, and doctor | Luvus advertises update and environment diagnosis. Herdr also ships install and update paths. (`L:README.md:53-55`, `H:README.md:41-47`, `H:src/main.rs:568-610`) | Later | Packaging must exist first. These are release engineering features, not core semantics. |
| Broad UHP-compatible method registry and delegated access | Luvus UHP covers most product domains and adds tokens, access scopes, gateways, fixtures, and capability discovery. (`L:website/src/content/docs/docs/guides/uhp.mdx:6-30`, `L:src/uhp/mod.rs:17-174`) | Later | Keep the good versioning model. Do not take the full method and security surface into v1. |
| GPUI desktop frontend | Neither reference project uses GPUI. The prior rendering report recommends it only as a later adapter. See [04, Decision](04-rendering-and-gpui.md#decision). | Later | The neutral core seam comes first. GPUI then has a clear prototype and performance gate. |
| OpenTUI | Neither clone depends on it. See [04, OpenTUI hypothesis](04-rendering-and-gpui.md#opentui-hypothesis). Verified at `H:Cargo.toml:23-48` and `L:Cargo.toml:55-119`. | Skip | It does not explain either reference design. Ratatui and Crossterm already cover the required v1 TUI surface. |
| Herdr source compatibility | [02, compatibility requirements](02-herdr-extensibility.md#compatibility-requirements) shows the required CLI, JSON, environment, ID, lifecycle, and plugin contracts. | Skip | The surface is too large and partly unversioned. A new tool should publish its own bounded contract. |
| Live child survival across machine or server death | Herdr docs distinguish detach from restart. Luvus snapshots also respawn panes. (`H:docs/next/website/src/content/docs/session-state.mdx:8-37`, `L:src/persist.rs:1-3`) | Skip | A snapshot cannot preserve a dead process. Promise cold restore and native agent resume instead. |

## Hardest parts

1. Terminal correctness is the hardest base subsystem. It must handle arbitrary
   byte streams, alternate screen, Unicode graphemes, wide cells, mouse modes,
   OSC, scrollback, selection, resize, and damage. The engine choice also changes
   build and patch cost. (`H:src/pane/terminal.rs:390-506`,
   `L:src/terminal/vt/mod.rs:187-260`)

2. The headless server and frontend stream are hard because ownership and
   backpressure must remain correct during detach, reconnect, resize, slow
   clients, and shutdown. A client must never grow an unbounded frame queue.
   (`H:src/server/client_transport.rs:175-279`,
   `L:src/ipc/server.rs:810-950`)

3. Agent status is hard because no single signal is reliable. Process identity,
   bottom-screen text, OSC evidence, native hooks, stale reports, and transition
   holds need one authority model. False blocked status can stop orchestration.
   False idle status can send input at the wrong time. (`H:src/detect/mod.rs:237-325`,
   `H:src/terminal/state.rs:1788-1857`, `L:src/detect.rs:700-893`)

4. Restore is hard because stable identity, snapshot migration, atomic writes,
   secret-bearing terminal history, native agent resume, and partial failure all
   meet at startup. The product must distinguish live detach, cold restore, and
   native conversation resume. (`H:docs/next/website/src/content/docs/session-state.mdx:8-50`,
   `L:src/persist.rs:986-1054`)

5. A public automation contract is hard because every command, event, error,
   revision, and ID can become permanent. Extensions make this harder because
   arbitrary subprocesses can call back through the same API. (`H:docs/next/website/src/content/docs/socket-api.mdx:20-47`,
   `L:protocol/README.md:1-21`)

6. Orchestration is the hardest later feature. It combines durable task state,
   dependencies, agent lifecycle, worktrees, path overlap, quality commands,
   merge serialization, and crash recovery. (`L:src/orch/mod.rs:1-150`,
   `L:website/src/content/docs/docs/guides/orchestration.mdx:18-43`)

7. GPUI is the hardest later frontend. It adds text shaping, IME,
   accessibility, pixel hit testing, GPU rendering, packaging, and three desktop
   platform backends. It must not force those concerns back into core. See
   [04, Risks and unknowns](04-rendering-and-gpui.md#risks-and-unknowns).

## Open questions that still block design work

This list de-duplicates all still-relevant questions from the open-question and
risk sections of reports 01 through 04. Rank 1 blocks the first implementation.
Lower ranks block later components or final protocol details.

| Rank | Open question | Decision blocked | Evidence needed |
| ---: | --- | --- | --- |
| 1 | Which terminal engine passes our required behavior with acceptable build and patch cost? | Final terminal engine and packaging | Run one corpus against Alacritty and Ghostty. Cover alternate screen, Unicode, mouse modes, OSC, hyperlinks, synchronized output, scrollback, and damage. This combines [01, open questions](01-herdr-architecture.md#open-questions) and [04, open questions](04-rendering-and-gpui.md#open-questions). |
| 2 | Which agents and which status states are required in v1? | Built-in fixtures, manifest feed, and any native adapter work | Name the target agents. Record process and bottom-screen fixtures for idle, working, blocked, startup, and exit. From [01, open questions](01-herdr-architecture.md#open-questions). |
| 3 | What exact state is saved, and what terminal data is safe to save by default? | Snapshot schema, history policy, and restore UX | Write crash and restart cases. Decide whether bounded visible output is opt-in, redacted, encrypted, or omitted. This combines [01, open questions](01-herdr-architecture.md#open-questions) and [03, open questions](03-luvus-architecture.md#open-questions). |
| 4 | What is the exact v1 public method and event set? | Protocol schema, CLI parity, and compatibility promise | Name the first external consumers. Write schemas and golden fixtures for topology, terminal reads, agent control, snapshots, and events. This combines [01, open questions](01-herdr-architecture.md#open-questions), [02, open questions](02-herdr-extensibility.md#open-questions), and [03, open questions](03-luvus-architecture.md#open-questions). |
| 5 | Which client holds the PTY-size lease, and how is takeover handled? | Multi-client resize and direct control protocol | Define active, idle, disconnect, timeout, and explicit takeover rules. Test different viewport sizes. From [04, open questions](04-rendering-and-gpui.md#open-questions). |
| 6 | What terminal damage unit gives smooth rendering without excess copying? | Engine trait, frontend stream, and frame backpressure | Benchmark full frames, dirty rows, styled cell runs, and engine-native patches at 1 and 15 active panes. From [04, open questions](04-rendering-and-gpui.md#open-questions). |
| 7 | What pane and client counts define the v1 performance budget? | Channel sizes, thread policy, history caps, and render cadence | Set expected and stress counts. Measure idle and active CPU, memory, wake rate, and shutdown time. This is the unresolved measurement behind [03, open questions](03-luvus-architecture.md#open-questions). |
| 8 | Do v1 terminal pointer messages need SGR pixel coordinates? | Neutral pointer command and terminal mode contract | Test real applications that request pixel mouse mode on all supported platforms. From [04, open questions](04-rendering-and-gpui.md#open-questions). |
| 9 | Which native agent sessions must resume after cold restart? | Later adapter registry and restore command model | Select agents with stable stores and official resume commands. Add bounded fixtures and duplicate-session rules. This combines [01, open questions](01-herdr-architecture.md#open-questions) and [02, open questions](02-herdr-extensibility.md#open-questions). |
| 10 | What is the smallest stable extension ABI? | Later manifest, event, settings, and trust model | Build two real extensions. Include one event-driven action and one pane. Add docks, bars, settings, or link handlers only when the examples need them. This combines [02, open questions](02-herdr-extensibility.md#open-questions) and [03, open questions](03-luvus-architecture.md#open-questions). |
| 11 | Can GPUI meet terminal frame time, glyph, IME, accessibility, snapshot-test, build, and packaging targets on all desktop platforms? | Future GPUI frontend | Build one pinned prototype. Test 15 active panes on macOS, Linux, and Windows. Measure cold build, binary size, frame time, and memory. This de-duplicates the GPUI questions in [04, risks](04-rendering-and-gpui.md#risks-and-unknowns) and [04, open questions](04-rendering-and-gpui.md#open-questions). |
| 12 | How should a future Codex plugin bundle be discovered, installed, and versioned? | Optional host integration packaging | Read the host's official loader contract and test one clean install. The Luvus repository alone cannot answer this. From [03, open questions](03-luvus-architecture.md#open-questions). |

The following prior questions no longer block this design:

- Detached server or monolith: choose the detached server. Keep a local escape
  hatch only for development.
- Tokio or standard threads: start with standard threads and bounded channels.
  Revisit only with profiling evidence.
- Stable API before stable UI: yes. Stabilize neutral core commands and events,
  not presentation fields.
- Broad UHP in v1: no. Start with a small versioned subset.
- Herdr release target, socket coverage, ID spelling, and plugin compatibility:
  no Herdr compatibility promise.
- Compiled or data-driven agent kinds: detection-only kinds are data-driven.
  Native powers are compiled.
- Zed terminal crates: do not depend on or copy them in this plan. They remain
  design references only, so their license and dependency questions do not
  block this design.
- Termwiz dependency cost: it is not a selected backend. Measure and disable
  unused Ratatui features during implementation. It does not block the design.
- Live process persistence after full server death: do not promise it. Restore
  structure and later resume supported native agent sessions.

## Proposed component skeleton

The package prefix is not selected yet. The directory and dependency boundaries
are the important part.

```text
Cargo.toml                       workspace only
crates/
  model/                        no IO and no frontend dependencies
    src/ids.rs                  stable workspace, tab, pane, terminal, split IDs
    src/layout.rs               BSP tree, ratios, and logical focus
    src/command.rs              core-owned commands and typed errors
    src/event.rs                ordered events, revisions, and generations
    src/snapshot.rs             immutable session and terminal query results
    src/terminal.rs             neutral cells, styles, cursor, modes, and damage
  terminal/                     PTY and emulator implementation
    src/pty.rs                  bounded pane actor and child lifecycle
    src/engine.rs               TerminalEngine trait
    src/alacritty.rs            first adapter
    src/platform/               Unix and Windows PTY details
  core/                         headless single-writer runtime
    src/session.rs              workspaces, tabs, panes, focus, and runtime registry
    src/dispatch.rs             validate and apply model commands
    src/detect/                 identity, manifests, evidence, and state arbiter
    src/persist/                versioned atomic snapshots and migrations
    src/extensions/             reserved for the later subprocess host
    src/orchestration/          reserved for the later separate ledger
  protocol/                     serialization and transport, no UI types
    src/control.rs              bounded JSON request and response frames
    src/frontend.rs             handshake, input, snapshots, damage, and effects
    src/version.rs              protocol and capability versions
  frontend-tui/                 the v1 frontend
    src/adapter.rs              Crossterm to model input conversion
    src/projection.rs           cell geometry and typed hit targets per client
    src/render.rs               neutral snapshots to Ratatui buffers
    src/paint.rs                terminal diff output and local effects
  frontend-gpui/                future crate, absent from the v1 build
    src/adapter.rs              GPUI input to model commands
    src/projection.rs           pixel geometry, fonts, and accessibility
    src/render.rs               neutral snapshots and damage to GPUI elements
  app/                          the one binary
    src/main.rs                 route server, client, CLI, and local debug roles
```

The dependency direction is strict:

```text
model <- terminal <- core <- app
model <- protocol <- frontend-tui <- app
model <- protocol <- frontend-gpui <- app     future
```

The seams are:

1. `CorePort::command` accepts a model command and returns a typed result or
   error. All mutation runs on the core owner thread.

2. `CorePort::query` returns immutable snapshots with a state revision or
   terminal generation. A frontend never borrows mutable core state.

3. `CorePort::subscribe` emits ordered state events and coalescible terminal
   damage. State changes are reliable. Superseded damage may be dropped. A gap
   forces a snapshot refresh.

4. Core owns sessions, IDs, split ratios, PTYs, terminal grids, scrollback,
   persistence, agent state, and the active PTY-size lease. It does not own
   rectangles, fonts, widgets, pointer capture, or clipboard objects.

5. The TUI owns terminal setup, cell layout, hit targets, focus presentation,
   Ratatui conversion, Crossterm conversion, and ANSI paint. It sends final
   terminal columns and rows to core after layout.

6. GPUI will own windows, pixels, glyph shaping, IME, pointer capture,
   accessibility nodes, native menus, and GPU paint. It will use the same model
   commands and snapshots. No GPUI type crosses the protocol.

7. Clipboard, URL open, notifications, and credentials are frontend services.
   Core requests them through typed effects. A headless client may reject an
   unsupported effect without changing core state.

8. Remote use serializes the same neutral commands, snapshots, events, and
   effects. It does not create a second application model or a second API.

This skeleton preserves the useful boundary proposed in [04, Proposed headless
core boundary](04-rendering-and-gpui.md#component-proposed-headless-core-boundary).
It also fixes the Ratatui and Crossterm leakage observed in Luvus and avoids
Herdr's concrete Ghostty coupling.
