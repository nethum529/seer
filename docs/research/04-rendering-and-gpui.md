# Rendering and GPUI research

## Decision

- Use Ratatui 0.30 and Crossterm 0.29 for the first terminal UI. Herdr uses
  Ratatui 0.30 and Crossterm 0.29. Luvus uses the same versions.
  (`herdr:Cargo.toml:23-48`, `luvus:Cargo.toml:55-60`)
- Do not use OpenTUI. Neither reference project depends on it. The current
  OpenTUI packages target Zig and TypeScript, not a normal Rust application.
  (`herdr:Cargo.toml:23-48`, `luvus:Cargo.toml:55-119`,
  [OpenTUI README:12-38](https://github.com/anomalyco/opentui/blob/c1a52d2114036a8490606d8a0d297cea9f135e90/README.md#L12-L38))
- Keep the terminal engine, application core, and frontend adapters separate.
  Luvus has a useful terminal engine trait, but its IPC and render state still
  expose Ratatui and Crossterm types. (`luvus:src/terminal/vt/mod.rs:1-4`,
  `luvus:src/terminal/vt/mod.rs:187-260`, `luvus:src/ipc/protocol.rs:1-60`)
- Treat GPUI as a later frontend. Do not make it the first renderer. It is
  pre-1.0 and its own README warns about frequent breaking changes.
  ([GPUI README:8-15](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L8-L15))
- If GPUI is adopted, pin all GPUI crates to exact compatible versions or one
  Git revision. This is our recommendation based on the documented breaking
  changes. Do not use a floating wildcard or a floating branch.
  ([GPUI README:8-15](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L8-L15))

## Evidence scope

- `herdr:` means `/home/nethum/Projects/_research/herdr` at commit
  `2290257acb2085ce6842ba5c7e3ca50c3ba64f02`.
- `luvus:` means `/home/nethum/Projects/_research/luvus` at commit
  `d1013d16f48cdd724b8df40c7c4c83dc306dc5d6`.
- GPUI and Zed links are pinned to Zed commit
  `399258feeaf90ad8a3a208c99221ee87b6452f38`.
- OpenTUI links are pinned to OpenTUI commit
  `c1a52d2114036a8490606d8a0d297cea9f135e90`.
- Local line references were read from those exact commits. External facts in
  this report were checked on 2026-08-30.

## Stack inventory

### Herdr

- Ratatui is declared as 0.30 and locked to 0.30.0.
  (`herdr:Cargo.toml:34-37`, `herdr:Cargo.lock:1298-1309`)
- Crossterm is declared as 0.29 and locked to 0.29.0.
  (`herdr:Cargo.toml:28-30`, `herdr:Cargo.lock:245-260`)
- portable-pty is pinned to 0.9.0 and patched to a vendor tree.
  (`herdr:Cargo.toml:32-35`, `herdr:Cargo.toml:50-51`,
  `herdr:Cargo.lock:1218-1225`)
- The terminal engine is vendored libghostty-vt at `1.3.2-HEAD` and is linked
  through a Zig build. (`herdr:vendor/libghostty-vt.vendor.json:1-4`,
  `herdr:build.rs:52-95`)
- Ratatui brings `ratatui-termwiz` 0.1.0 and Termwiz 0.23.3 into the lock file.
  (`herdr:Cargo.lock:1354-1361`, `herdr:Cargo.lock:1759-1776`)

### Luvus

- Ratatui is declared as 0.30 and locked to 0.30.2.
  (`luvus:Cargo.toml:55-57`, `luvus:Cargo.lock:1262-1275`)
- Crossterm is declared as 0.29 and locked to 0.29.0.
  (`luvus:Cargo.toml:57-59`, `luvus:Cargo.lock:239-255`)
- portable-pty is declared as 0.9 and locked to 0.9.0.
  (`luvus:Cargo.toml:58-60`, `luvus:Cargo.lock:1179-1186`)
- The terminal engine is `luvus-alacritty-terminal`, declared as 0.26 and locked
  to 0.26.1. It is patched to a vendor tree.
  (`luvus:Cargo.toml:59-60`, `luvus:Cargo.toml:124-129`,
  `luvus:Cargo.lock:815-838`)
- Ratatui brings `ratatui-termwiz` 0.1.2 and Termwiz 0.23.3 into the lock file.
  (`luvus:Cargo.lock:1333-1340`, `luvus:Cargo.lock:1721-1738`)

## Component: Herdr terminal client and wire protocol

### Name

- Herdr terminal client and wire protocol.

### Responsibility

- The client owns terminal setup, raw input capture, mouse capture, resize
  reporting, frame receipt, diff encoding, and writes to stdout. It does not own
  the application model. (`herdr:src/client/mod.rs:1-13`,
  `herdr:src/client/mod.rs:574-623`, `herdr:src/client/mod.rs:1640-1718`)

### Key files

- `herdr:src/client/mod.rs:1382-1388` describes the client thread layout.
- `herdr:src/client/mod.rs:1640-1718` handles input, resize, and frames.
- `herdr:src/protocol/wire.rs:37-44` selects semantic frames or ANSI frames.
- `herdr:src/protocol/wire.rs:95-136` defines input types owned by Herdr.
- `herdr:src/protocol/wire.rs:525-600` defines and builds semantic frames.

### Key types

- `ClientInputEvent` contains Herdr key, mouse, text, paste, and focus events.
  It does not put Crossterm event types on the wire.
  (`herdr:src/protocol/wire.rs:95-136`)
- `FrameData` contains cells, dimensions, cursor data, hyperlinks, and graphics.
  (`herdr:src/protocol/wire.rs:525-540`)
- `TerminalFrame` carries ANSI bytes for a terminal-rendered attachment.
  (`herdr:src/protocol/wire.rs:633-646`)
- `ServerMessage` can carry either semantic frame data or terminal frame data.
  (`herdr:src/protocol/wire.rs:659-677`)

### How it talks to other components

- Crossterm events are converted at the client adapter before they enter the
  wire protocol. Resize is sent as a separate client message.
  (`herdr:src/protocol/wire.rs:270-292`)
- The server sends complete semantic frame data. The client computes a terminal
  diff and paints it. (`herdr:src/client/mod.rs:1691-1718`)
- A size poller checks the terminal every 100 ms and sends changes to the server.
  (`herdr:src/client/mod.rs:2522-2564`)

## Component: Herdr view, renderer, and pointer routing

### Name

- Herdr computed view and Ratatui renderer.

### Responsibility

- The view pass computes geometry and hit targets. The render pass reads the
  application state and draws the frame. Mouse input reads the same computed
  geometry. (`herdr:src/ui.rs:108-156`, `herdr:src/ui.rs:215-324`,
  `herdr:src/ui.rs:389-462`)

### Key files

- `herdr:src/ui.rs:108-156` starts view computation and controls whether a
  secondary client may resize panes.
- `herdr:src/ui.rs:215-324` computes desktop rectangles, pane information, and
  split borders.
- `herdr:src/app/state.rs:862-885` stores computed `ViewState` geometry.
- `herdr:src/app/input/mouse.rs:28-64` defines semantic mouse results.
- `herdr:src/app/input/mouse.rs:426-478` starts drags and handles scrollbars.
- `herdr:src/app/input/mouse.rs:938-1035` routes wheel and hover events.

### Key types

- `ViewState` stores rectangles, pane information, and split borders used by
  both rendering and input. (`herdr:src/app/state.rs:862-885`)
- `PaneInfo` and `SplitBorder` describe the pane and divider geometry.
  (`herdr:src/layout.rs:32-61`)
- `DragState` stores the current pointer drag.
  (`herdr:src/app/state.rs:1239-1242`)
- `MouseAction` is a small semantic result. It includes pane focus, pane move,
  and split ratio changes. (`herdr:src/app/input/mouse.rs:28-64`)

### How it talks to other components

- Clicks first pass through overlays and global UI hit targets. A pane click is
  then sent to the child when mouse reporting is active, or used for selection
  and focus. (`herdr:src/app/input/mod.rs:329-361`,
  `herdr:src/app/input/mouse.rs:629-668`)
- Drag motion updates selection, tab reorder, sidebar width, or a split ratio.
  A split ratio is clamped from 0.1 to 0.9.
  (`herdr:src/app/input/mouse.rs:671-824`)
- Mouse release finalizes the operation, may copy the selection, and clears the
  drag state. (`herdr:src/app/input/mouse.rs:832-925`)
- Hover updates menus and may forward motion to a child terminal.
  (`herdr:src/app/input/mouse.rs:1025-1035`)
- Wheel input can cycle tabs, scroll the terminal, or scroll the sidebar. Child
  terminal reporting gets the first chance before host scrollback.
  (`herdr:src/app/input/mouse.rs:938-1023`,
  `herdr:src/app/input/mouse.rs:1703-1750`)
- Pane resize hit testing uses the computed divider and gap geometry. The result
  becomes an application API call that changes a split ratio.
  (`herdr:src/app/input/mouse.rs:1410-1460`,
  `herdr:src/app/input/mod.rs:383-450`)
- The headless server renders a virtual client even when no display client is
  connected. It renders each client using that client's size.
  (`herdr:src/server/headless.rs:4432-4512`)

## Component: Herdr terminal emulator

### Name

- Herdr Ghostty terminal engine.

### Responsibility

- The engine parses terminal output, stores terminal state, encodes input, and
  produces terminal snapshots for rendering.
  (`herdr:src/pane/terminal.rs:165-215`,
  `herdr:src/pane/terminal.rs:390-506`)

### Key files

- `herdr:src/pane/terminal.rs:165-215` defines the concrete terminal objects and
  byte processing.
- `herdr:src/pane/terminal.rs:390-506` exposes mode, render, cursor, hyperlink,
  dirty-region, and image data.
- `herdr:src/pane/terminal.rs:1021-1059` creates the Ghostty terminal, renderer,
  and key encoder.
- `herdr:build.rs:6-17` lists supported target families.
- `herdr:build.rs:52-95` builds and links the Ghostty library through Zig.
- `herdr:vendor/libghostty-vt.vendor.json:1-4` records the vendored revision.

### Key types

- `GhosttyPaneTerminal`, `GhosttyPaneCore`, and `PaneTerminal` are concrete
  types. The shared core is protected by a mutex.
  (`herdr:src/pane/terminal.rs:165-195`)
- The public methods expose terminal input mode, mouse mode, alternate-screen
  state, wheel routing, cells, cursor state, and dirty patches.
  (`herdr:src/pane/terminal.rs:390-506`)

### How it talks to other components

- PTY bytes enter the concrete Ghostty core. Resize calls also go directly to
  that core. (`herdr:src/pane/terminal.rs:202-215`)
- The application reads snapshots and terminal modes. The renderer consumes
  cells and other terminal metadata. (`herdr:src/pane/terminal.rs:390-506`)
- There is no terminal engine trait at this boundary in the cited implementation.
  Replacing Ghostty would require changing this concrete integration.
  (`herdr:src/pane/terminal.rs:165-215`,
  `herdr:src/pane/terminal.rs:1021-1059`)

## Component: Luvus terminal client and wire protocol

### Name

- Luvus terminal client and render stream.

### Responsibility

- The client owns terminal setup, input capture, frame receipt, and painting. It
  does not own the application state. (`luvus:src/ipc/client.rs:1-20`,
  `luvus:src/ipc/client.rs:63-78`, `luvus:src/ipc/client.rs:171-210`)

### Key files

- `luvus:src/ipc/client.rs:106-120` sends initial client geometry.
- `luvus:src/ipc/client.rs:319-362` maps terminal events to client messages.
- `luvus:src/ipc/client.rs:504-545` paints cells with a Ratatui backend.
- `luvus:src/ipc/protocol.rs:1-60` defines input and frame messages.
- `luvus:src/ipc/protocol.rs:244-320` creates full and diff frames.
- `luvus:src/ipc/server.rs:810-950` renders per client and applies backpressure.

### Key types

- `ClientMessage` directly carries Crossterm `KeyEvent` and `MouseEvent` values.
  (`luvus:src/ipc/protocol.rs:1-36`)
- `FrameData` and frame diffs directly use Ratatui color and modifier types.
  (`luvus:src/ipc/protocol.rs:1-9`, `luvus:src/ipc/protocol.rs:38-60`)
- `RenderTarget` wraps a Ratatui `Buffer`.
  (`luvus:src/ui/mod.rs:19-31`)

### How it talks to other components

- The server promotes the interacting client, renders for its geometry before
  hit testing, and converts its input into an application event.
  (`luvus:src/ipc/server.rs:690-747`)
- The server renders the foreground client first, then other clients at their
  own sizes. (`luvus:src/ipc/server.rs:810-850`)
- Full or diff frames use a bounded output path so a slow client does not build
  an unlimited frame queue. (`luvus:src/ipc/server.rs:863-950`)
- This boundary is tied to Crossterm and Ratatui. A GPUI frontend would need a
  second protocol or conversions around these frontend types.
  (`luvus:src/ipc/protocol.rs:1-60`)

## Component: Luvus renderer, layout, and pointer routing

### Name

- Luvus Ratatui renderer and application-owned hit geometry.

### Responsibility

- The renderer computes layout, draws into a Ratatui buffer, and writes hit
  rectangles back into application state. Mouse input later uses those
  rectangles. (`luvus:src/ui/mod.rs:114-190`,
  `luvus:src/ui/mod.rs:407-475`, `luvus:src/ui/mod.rs:674-706`)

### Key files

- `luvus:src/app/mod.rs:1885-1930` stores hover, hit rectangles, and drag state.
- `luvus:src/app/input.rs:1012-1052` checks whether motion changed visual state.
- `luvus:src/app/input.rs:1550-1885` routes click, drag, release, and hover.
- `luvus:src/app/input.rs:1889-2099` routes wheel input.
- `luvus:src/layout.rs:258-360` finds dividers and updates ratios.
- `luvus:src/ui/sidebar.rs:128-159` draws divider hover and drag feedback.

### Key types

- Application state stores `hover`, pane rectangles, resize drag state, hovered
  divider state, sidebar resize state, and the last main area.
  (`luvus:src/app/mod.rs:1885-1930`)
- `ClientInput` contains Crossterm and Ratatui types and is carried inside
  `AppEvent`. (`luvus:src/event.rs:8-83`)
- Layout divider records support exact and nearest-divider hit testing.
  (`luvus:src/layout.rs:258-320`)

### How it talks to other components

- A click checks links, the sidebar divider, and pane dividers before it starts
  child mouse forwarding or text selection.
  (`luvus:src/app/input.rs:1637-1764`)
- Drag motion gives priority to an active link, divider drag, sidebar drag,
  captured child mouse input, and then selection.
  (`luvus:src/app/input.rs:1771-1806`)
- Mouse release ends resize or child capture and can copy a selection.
  (`luvus:src/app/input.rs:1808-1853`)
- Hover scans for links when the control modifier is held. Motion is also sent
  to a child that requested any-motion events.
  (`luvus:src/app/input.rs:1855-1885`)
- Wheel input is routed by UI area. In a terminal, child mouse reporting wins.
  Primary-screen fallback uses host scrollback. Alternate-screen fallback emits
  cursor-key behavior. (`luvus:src/app/input.rs:1889-1929`,
  `luvus:src/app/input.rs:2015-2099`)
- Pane resize is disabled when the affected child is reporting mouse input.
  Ratios enforce a minimum pane size. (`luvus:src/app/mod.rs:5325-5423`,
  `luvus:src/layout.rs:322-360`)
- Sidebar width changes live during drag and is persisted once on release.
  (`luvus:src/app/mod.rs:5442-5539`)

## Component: Luvus terminal emulator boundary

### Name

- Luvus `VtEngine` and Alacritty adapter.

### Responsibility

- The trait isolates terminal parsing, visible cells, cursor state, resize, and
  history from the concrete Alacritty implementation.
  (`luvus:src/terminal/vt/mod.rs:1-4`,
  `luvus:src/terminal/vt/mod.rs:187-260`)

### Key files

- `luvus:src/terminal/vt/mod.rs:71-105` selects and creates the current engine.
- `luvus:src/terminal/vt/mod.rs:108-116` defines render cells.
- `luvus:src/terminal/vt/mod.rs:187-260` defines the engine trait.
- `luvus:src/terminal/vt/alacritty.rs:1-18` imports the pure Rust Alacritty
  backend and the local trait.
- `luvus:src/terminal/vt/alacritty.rs:92-100` defines `AlacrittyEngine`.
- `luvus:Cargo.toml:55-60` selects the renamed Alacritty package.
- `luvus:Cargo.toml:124-129` patches Alacritty and VTE to local vendor trees.

### Key types

- `VtEngine` is the engine contract. It covers visible cells, mode detection,
  cursor state, resize, and history. (`luvus:src/terminal/vt/mod.rs:187-260`)
- `AlacrittyEngine` is the only engine selected by the factory.
  (`luvus:src/terminal/vt/mod.rs:71-105`,
  `luvus:src/terminal/vt/alacritty.rs:92-100`)
- `RenderCell` still uses Ratatui color and modifier types. The terminal trait is
  not fully renderer-neutral. (`luvus:src/terminal/vt/mod.rs:108-116`)

### How it talks to other components

- The backend constructs an engine and exposes terminal operations and snapshots
  through application methods. (`luvus:src/app/backend.rs:90-220`)
- The UI consumes cells from the trait, but Ratatui style types cross the
  boundary. A GPUI adapter would need to translate those types.
  (`luvus:src/terminal/vt/mod.rs:108-116`,
  `luvus:src/terminal/vt/mod.rs:187-260`)

## OpenTUI hypothesis

- Finding: the hypothesis is false for both reference projects.

- Herdr directly declares Ratatui 0.30, Crossterm 0.29, and portable-pty 0.9.
  (`herdr:Cargo.toml:23-48`)
- Luvus directly declares Ratatui 0.30, Crossterm 0.29, portable-pty 0.9, and a
  renamed Alacritty terminal package. (`luvus:Cargo.toml:55-60`)
- Ratatui itself pulls a Termwiz adapter in each lock file. This is transitive
  use. It is not evidence that either application chose Termwiz as its direct
  frontend backend. (`herdr:Cargo.lock:1298-1309`,
  `herdr:Cargo.lock:1354-1361`, `luvus:Cargo.lock:1262-1275`,
  `luvus:Cargo.lock:1333-1340`)
- The only `OpenTUI` text found in either source tree is a Herdr test name. The
  test feeds a Ghostty-style 256-color query burst. It is a compatibility test,
  not an OpenTUI dependency. (`herdr:src/pane/terminal.rs:6083-6105`)
- Current OpenTUI is written in Zig. Its public packages provide TypeScript,
  React, and Solid interfaces. Its native package is private to its workspace.
  ([OpenTUI README:12-38](https://github.com/anomalyco/opentui/blob/c1a52d2114036a8490606d8a0d297cea9f135e90/README.md#L12-L38))
- OpenTUI development requires Bun and Zig. This adds a second application
  runtime and build stack to a Rust tool.
  ([OpenTUI README:50-68](https://github.com/anomalyco/opentui/blob/c1a52d2114036a8490606d8a0d297cea9f135e90/README.md#L50-L68))

## Rust rendering and terminal options

### Ratatui 0.30

- Role: terminal UI layout, widgets, buffers, styles, and terminal backend
  integration.
- Evidence: both reference applications declare version 0.30.
  (`herdr:Cargo.toml:34-37`, `luvus:Cargo.toml:55-57`)
- Strength: it already supports the full click, drag, hover, scroll, and divider
  resize designs described above.
  (`herdr:src/app/input/mouse.rs:426-478`,
  `herdr:src/app/input/mouse.rs:629-824`,
  `luvus:src/app/input.rs:1637-1929`)
- Risk: Ratatui types can leak into application events, wire messages, and
  terminal engine traits if the boundary is not enforced.
  (`luvus:src/event.rs:8-83`, `luvus:src/ipc/protocol.rs:1-60`,
  `luvus:src/terminal/vt/mod.rs:108-116`)
- Verdict: use it for the first TUI adapter. Do not expose its types from the
  core API.

### Crossterm 0.29

- Role: raw terminal setup and terminal input/output events.
- Evidence: both projects declare version 0.29 and use mouse capture in their
  thin clients. (`herdr:Cargo.toml:28-30`,
  `herdr:src/client/mod.rs:28-37`, `luvus:Cargo.toml:57-59`,
  `luvus:src/ipc/client.rs:63-78`)
- Strength: it covers keys, mouse, focus, paste, resize, and terminal mode
  control used by both products. (`herdr:src/client/mod.rs:574-623`,
  `herdr:src/client/mod.rs:1640-1689`, `luvus:src/ipc/client.rs:171-189`,
  `luvus:src/ipc/client.rs:319-362`)
- Risk: its concrete event types can make a wire or core API terminal-specific.
  Luvus permits this leak. Herdr converts them at the adapter.
  (`luvus:src/ipc/protocol.rs:1-36`, `herdr:src/protocol/wire.rs:95-136`,
  `herdr:src/protocol/wire.rs:270-292`)
- Verdict: use it inside the TUI adapter only.

### Termwiz

- Role: another terminal backend supported by Ratatui.
- Evidence: Ratatui brings `ratatui-termwiz` and Termwiz into both lock files.
  (`herdr:Cargo.lock:1298-1309`, `herdr:Cargo.lock:1354-1361`,
  `herdr:Cargo.lock:1759-1776`, `luvus:Cargo.lock:1262-1275`,
  `luvus:Cargo.lock:1333-1340`, `luvus:Cargo.lock:1721-1738`)
- UNKNOWN: the compile-time and binary-size cost of this unused adapter in our
  dependency graph. Resolve this with `cargo tree -e features` and release build
  size measurements after the first Cargo workspace exists.
- Verdict: do not choose it directly now. Disable unused Ratatui backend features
  if a local dependency measurement shows a useful reduction.

### Ghostty terminal library

- Role: terminal emulation, keyboard encoding, mouse modes, dirty rendering,
  hyperlinks, and image data inside Herdr.
- Evidence: Herdr builds a vendored Ghostty terminal library with Zig and links
  it as a static library. (`herdr:build.rs:32-95`,
  `herdr:vendor/libghostty-vt.vendor.json:1-4`)
- Strength: the integration exposes dirty patches, SGR pixel mouse state,
  hyperlinks, and Kitty image data. (`herdr:src/pane/terminal.rs:390-506`)
- Risk: the integration adds Zig, FFI, vendored source, and concrete coupling.
  (`herdr:build.rs:32-95`, `herdr:src/pane/terminal.rs:165-215`)
- Verdict: keep it as a terminal engine candidate, not as a rendering framework.
  Put it behind our own terminal engine contract before adopting it.

### Alacritty terminal library

- Role: pure Rust terminal emulation inside Luvus.
- Evidence: Luvus implements its `VtEngine` with an Alacritty adapter and patches
  its Alacritty and VTE packages to vendored trees.
  (`luvus:src/terminal/vt/alacritty.rs:1-18`,
  `luvus:src/terminal/vt/alacritty.rs:92-100`, `luvus:Cargo.toml:124-129`)
- Strength: its pure Rust implementation fits behind a trait without a Zig build
  or FFI. (`luvus:src/terminal/vt/mod.rs:187-260`,
  `luvus:src/terminal/vt/alacritty.rs:1-18`)
- Risk: Luvus needs local forks, and its render cell type leaks Ratatui styles.
  (`luvus:Cargo.toml:124-129`, `luvus:src/terminal/vt/mod.rs:108-116`)
- Verdict: use it as the baseline terminal engine candidate. Define our own cell
  and style values instead of copying the Ratatui leak.

### Zed terminal crates

- Role: an Alacritty-backed terminal model and a GPUI terminal view.
- Evidence: Zed describes a backend-neutral terminal model and a separate GPUI
  view that renders `TerminalContent`.
  ([Zed terminal view README:5-23](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal_view/README.md#L5-L23))
- Risk: both crates use GPL-3.0-or-later. The model depends on GPUI and several
  Zed service crates. The view also depends on editor, project, menu, workspace,
  and UI crates.
  ([Zed terminal Cargo.toml:1-46](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal/Cargo.toml#L1-L46),
  [Zed terminal view Cargo.toml:1-49](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal_view/Cargo.toml#L1-L49))
- Verdict: study its boundary. Do not depend on or copy its code without an
  explicit license and dependency decision.

### portable-pty 0.9

- Role: PTY creation and process I/O. It is not a renderer or terminal emulator.
- Evidence: both projects declare portable-pty 0.9. Herdr pins and vendors it.
  Luvus uses the registry release. (`herdr:Cargo.toml:32-35`,
  `herdr:Cargo.toml:50-51`, `luvus:Cargo.toml:58-60`,
  `luvus:Cargo.lock:1179-1186`)
- Verdict: keep PTY ownership in the headless core. Do not let the selected UI
  framework own process lifetime or PTY I/O.

## Component: GPUI frontend candidate

### Name

- GPUI desktop frontend.

### Responsibility

- GPUI is a GPU-accelerated Rust UI framework with both immediate and retained
  patterns. It provides state entities, declarative views, and lower-level
  custom elements.
  ([GPUI README:3-4](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L3-L4),
  [GPUI README:74-84](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L74-L84))

### Key files

- [GPUI README:8-43](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L8-L43)
  states the support and setup contract.
- [GPUI Cargo.toml:1-20](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/Cargo.toml#L1-L20)
  gives the published version, license, and default features.
- [gpui_platform.rs:13-25](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui_platform/src/gpui_platform.rs#L13-L25)
  constructs normal and headless applications.
- [gpui_platform.rs:56-95](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui_platform/src/gpui_platform.rs#L56-L95)
  selects platform implementations and the headless renderer.

### Key types

- `Application` starts a standalone program and selects platform services through
  `gpui_platform::application()`. ([GPUI README:15-24](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L15-L24))
- `Entity` stores state owned by GPUI. A view is an entity that implements
  `Render`. Elements provide lower-level rendering control.
  ([GPUI README:74-84](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L74-L84))
- GPUI actions convert keystrokes into logical UI operations.
  ([GPUI README:86-94](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L86-L94))

### How it talks to other components

- Proposed: a GPUI adapter should subscribe to core events and query neutral
  terminal snapshots. It should send only core-owned commands.
- Proposed: GPUI should own window geometry, pixel hit testing, pointer capture,
  fonts, theme mapping, menus, and accessibility nodes.
- Proposed: the core should own sessions, stable IDs, PTYs, terminal grids,
  scrollback, split ratios, persistence, and agent state.
- Proposed: GPUI should translate its pointer and keyboard events at the adapter.
  No GPUI type should appear in a core trait or stored core event.

### Current state and maturity

- The current published GPUI manifest says version 0.2.2, `publish = true`, and
  Apache-2.0. ([GPUI Cargo.toml:1-12](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/Cargo.toml#L1-L12))
- The project is pre-1.0, under active development, and expects frequent breaking
  changes. It requires the latest stable Rust.
  ([GPUI README:8-12](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L8-L12))
- The documented native targets are macOS, Linux, FreeBSD, and Windows. macOS
  uses Metal. Linux and FreeBSD use Wayland or X11. Windows uses Win32 and
  DirectWrite. ([GPUI README:27-43](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L27-L43))
- The platform selector also contains a WebAssembly implementation.
  ([gpui_platform.rs:13-20](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui_platform/src/gpui_platform.rs#L13-L20),
  [gpui_platform.rs:76-80](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui_platform/src/gpui_platform.rs#L76-L80))
- A visual headless renderer is only returned on macOS in the current platform
  selector. Other targets return none.
  ([gpui_platform.rs:83-95](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui_platform/src/gpui_platform.rs#L83-L95))
- The GPUI crate has many Zed workspace dependencies. A release from crates.io
  is possible, but it is not a small dependency surface.
  ([GPUI Cargo.toml:45-110](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/Cargo.toml#L45-L110))

## Component: Proposed headless core boundary

### Name

- Frontend-neutral core service.

### Responsibility

- This proposed component owns durable application state and terminal semantics.
  It must run without Ratatui, Crossterm, GPUI, or a visible display.

- Herdr shows that a server can keep a virtual view alive without a display
  client. It also has stable session snapshots with workspace, tab, pane,
  layout, and agent identities. (`herdr:src/server/headless.rs:4432-4512`,
  `herdr:src/api/schema/session.rs:1-23`)

### Key files

- These paths are proposed. They do not exist yet.

- `crates/core/src/port.rs`: command, query, and subscription contracts.
- `crates/core/src/model.rs`: stable IDs and session snapshots.
- `crates/core/src/terminal.rs`: neutral cells, styles, modes, and damage.
- `crates/core/src/layout.rs`: split tree and ratios, without screen rectangles.
- `crates/frontend-tui/src/adapter.rs`: Crossterm and Ratatui conversion.
- `crates/frontend-gpui/src/adapter.rs`: GPUI conversion.

### Key types

- The names below are proposed domain messages. They are not Rust definitions.

#### Core commands

- `CreateWorkspace`, `CreateTab`, `SplitPane`, `ClosePane`, `FocusPane`, and
  `MovePane` change the session model.
- `SetSplitRatio { split_id, ratio }` changes logical layout. The core validates
  the ratio. Frontends do not send pixel or cell rectangles.
- `ResizeTerminal { pane_id, columns, rows, pixel_width, pixel_height }` reports
  the final terminal viewport after frontend layout.
- `TerminalKey`, `TerminalText`, and `TerminalPaste` carry core-owned input data.
- `TerminalPointer { pane_id, phase, button, modifiers, cell, pixel }` carries
  normalized child-terminal pointer input. `pixel` is optional.
- `ScrollTerminal { pane_id, delta, unit }` carries semantic host scrollback.
- `SetSelection`, `ClearSelection`, and `CopySelection` carry terminal selection
  actions.

#### Core events

- `SnapshotChanged { revision }` tells a frontend to refresh model data.
- `LayoutChanged { revision, split_tree }` carries IDs, orientation, and ratios.
- `TerminalDamage { pane_id, generation, runs }` carries changed cell runs.
- `CursorChanged`, `TitleChanged`, `Bell`, `ProcessExited`, and
  `AgentStatusChanged` carry focused changes.
- `ClipboardRequest` and `OpenUrlRequest` ask a frontend for platform services.
- `CoreError { command_id, code, message }` reports command failure.

#### Core query results

- `SessionSnapshot` returns workspaces, tabs, panes, split trees, and stable IDs.
- `TerminalSnapshot` returns dimensions, neutral cell runs, cursor, modes,
  hyperlinks, images, and a generation number.
- `SelectionText` returns extracted text without using a frontend clipboard type.

#### Neutral terminal values

- `CellRun` groups adjacent cells with the same `CellStyle`.
- `CellStyle` uses core-owned color, attributes, underline, and hyperlink values.
- `TerminalModes` tells the adapter whether the child accepts mouse events,
  alternate scroll, focus events, bracketed paste, or pixel coordinates.

### How it talks to other components

- Proposed `CorePort.command`: accepts one core command and returns success or a
  typed core error.
- Proposed `CorePort.query`: returns an immutable snapshot at one revision.
- Proposed `CorePort.subscribe`: emits ordered events with revision or generation
  numbers. A frontend may coalesce stale terminal damage but not state changes.
- Proposed `FrontendServices`: the core requests clipboard writes, URL opening,
  notifications, and credential prompts through a small service interface.
- Proposed TUI flow: Crossterm event, TUI hit test, core command, core event,
  neutral snapshot, Ratatui conversion, terminal paint.
- Proposed GPUI flow: GPUI event, pixel hit test, core command, core event,
  neutral snapshot, GPUI element draw.
- Proposed remote flow: serialize the same core commands and events. Do not
  serialize Crossterm, Ratatui, or GPUI types.

- The split between computed geometry and semantic state follows useful evidence
  in Herdr. Its renderer and mouse handler share `ViewState`, while application
  mutation occurs through semantic actions. (`herdr:src/app/state.rs:862-885`,
  `herdr:src/app/input/mouse.rs:28-64`,
  `herdr:src/app/input/mod.rs:383-450`)

- The neutral cell requirement follows a problem in Luvus. Its engine trait is
  useful, but `RenderCell` and IPC data still contain Ratatui values.
  (`luvus:src/terminal/vt/mod.rs:108-116`,
  `luvus:src/ipc/protocol.rs:1-60`)

## Risks and unknowns

- UNKNOWN: GPUI terminal-grid throughput and glyph correctness for our workload.
  Resolve it with a prototype that redraws damage for 15 active panes. Measure
  frame time, memory, shaping, ligatures, wide glyphs, combining marks, and IME
  input on macOS, Linux, and Windows. GPUI offers custom low-level elements, but
  that does not prove this workload.
  ([GPUI README:76-84](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L76-L84))
- UNKNOWN: GPUI platform quality for our exact input, clipboard, accessibility,
  and multi-window needs. Resolve it with one CI build and one manual smoke test
  on each supported target. The source states the backends, not our quality bar.
  ([GPUI README:27-43](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L27-L43))
- UNKNOWN: cross-platform visual snapshot testing for GPUI. The current helper
  supplies a headless renderer only on macOS. Resolve it by testing normal
  windows under Linux and Windows CI, or by keeping render-model tests below the
  GPUI layer.
  ([gpui_platform.rs:83-95](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui_platform/src/gpui_platform.rs#L83-L95))
- UNKNOWN: which terminal engine meets our protocol and packaging needs. Resolve
  it with one shared compliance corpus against Alacritty and Ghostty. Include
  mouse modes, alternate screen, Unicode, hyperlinks, sixel or Kitty images,
  shell integration, and dirty-region output. The reference engines have
  different build and abstraction costs. (`herdr:build.rs:32-95`,
  `herdr:src/pane/terminal.rs:390-506`,
  `luvus:src/terminal/vt/mod.rs:187-260`)
- UNKNOWN: whether Zed's GPL terminal crates are acceptable to the planned
  product license. Resolve it with legal review before any dependency or code
  reuse. The crate manifests declare GPL-3.0-or-later.
  ([Zed terminal Cargo.toml:1-6](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal/Cargo.toml#L1-L6),
  [Zed terminal view Cargo.toml:1-6](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal_view/Cargo.toml#L1-L6))
- UNKNOWN: which connected frontend controls PTY dimensions when clients have
  different sizes. Resolve it with an explicit active-client lease and takeover
  rule. Both projects already privilege a foreground or interacting client.
  (`herdr:src/server/headless.rs:286-343`, `luvus:src/ipc/server.rs:690-747`,
  `luvus:src/ipc/server.rs:810-850`)
- UNKNOWN: whether pixel mouse coordinates must be preserved through the core
  API. Resolve it by testing applications that request SGR pixel mouse mode.
  Herdr exposes pixel mode and local pixel calculations.
  (`herdr:src/pane/terminal.rs:390-426`,
  `herdr:src/app/input/mouse.rs:1752-1789`)
- UNKNOWN: the compile and binary cost of GPUI's published dependency graph on
  each target. Resolve it with a pinned empty application and cold build timing
  on macOS, Linux, and Windows. The current manifest has a broad dependency
  list. ([GPUI Cargo.toml:45-110](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/Cargo.toml#L45-L110))
- Risk: a rendered-frame-only core API would make native GPUI controls difficult.
  Herdr's `FrameData` is designed around terminal cells. Keep semantic state and
  commands as the main API, with terminal damage as one data stream.
  (`herdr:src/protocol/wire.rs:525-600`)
- Risk: render-time mutation makes input depend on which client rendered last.
  Luvus writes hit rectangles into application state during rendering and must
  render the interacting client before input handling. Keep geometry inside each
  frontend projection. (`luvus:src/ui/mod.rs:407-475`,
  `luvus:src/ui/mod.rs:674-706`, `luvus:src/ipc/server.rs:690-747`)
- Risk: unbounded frame delivery can let a slow frontend exhaust memory. Luvus
  uses a bounded path. Core subscriptions need coalescing and backpressure.
  (`luvus:src/ipc/server.rs:863-950`)

## Takeaways for our tool

### Copy

- Copy Herdr's adapter rule: convert Crossterm input into core-owned input types
  at the edge. (`herdr:src/protocol/wire.rs:95-136`,
  `herdr:src/protocol/wire.rs:270-292`)
- Copy the shared geometry rule: rendering and pointer hit testing must read one
  projection for the current frontend and size.
  (`herdr:src/ui.rs:215-324`, `herdr:src/app/state.rs:862-885`)
- Copy the semantic pointer flow: clicks, drags, hover, scroll, and resize become
  application actions after hit testing.
  (`herdr:src/app/input/mouse.rs:28-64`,
  `herdr:src/app/input/mod.rs:383-450`)
- Copy Luvus's terminal engine trait idea, but replace Ratatui values with
  core-owned cells and styles. (`luvus:src/terminal/vt/mod.rs:108-116`,
  `luvus:src/terminal/vt/mod.rs:187-260`)
- Copy bounded frame or damage delivery. Slow frontends must not grow memory
  without a limit. (`luvus:src/ipc/server.rs:863-950`)
- Copy the active-client concept for PTY sizing, then define the lease and
  takeover behavior as a public rule. (`luvus:src/ipc/server.rs:690-850`)
- Copy Zed's conceptual split between backend-neutral terminal data and a GPUI
  view. Do not copy GPL code without approval.
  ([Zed terminal view README:13-23](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal_view/README.md#L13-L23),
  [Zed terminal view Cargo.toml:1-6](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal_view/Cargo.toml#L1-L6))

### Avoid

- Avoid OpenTUI for this Rust application. It does not explain either reference
  stack and adds Zig plus a TypeScript and Bun interface.
  ([OpenTUI README:12-38](https://github.com/anomalyco/opentui/blob/c1a52d2114036a8490606d8a0d297cea9f135e90/README.md#L12-L38),
  [OpenTUI README:50-68](https://github.com/anomalyco/opentui/blob/c1a52d2114036a8490606d8a0d297cea9f135e90/README.md#L50-L68))
- Avoid Crossterm, Ratatui, GPUI, and terminal-engine concrete types in core
  messages. Luvus shows how those types spread across IPC, events, and render
  cells. (`luvus:src/ipc/protocol.rs:1-60`, `luvus:src/event.rs:8-83`,
  `luvus:src/terminal/vt/mod.rs:108-116`)
- Avoid a single global set of hit rectangles. Each frontend needs geometry for
  its own viewport. Luvus must render the active client before using its stored
  rectangles. (`luvus:src/ipc/server.rs:690-747`,
  `luvus:src/ui/mod.rs:674-706`)
- Avoid floating GPUI versions. GPUI documents frequent pre-1.0 breakage.
  ([GPUI README:8-12](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/gpui/README.md#L8-L12))
- Avoid adopting Zed's terminal crates as a shortcut before reviewing their GPL
  license and Zed-specific dependency surface.
  ([Zed terminal Cargo.toml:1-46](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal/Cargo.toml#L1-L46),
  [Zed terminal view Cargo.toml:1-49](https://github.com/zed-industries/zed/blob/399258feeaf90ad8a3a208c99221ee87b6452f38/crates/terminal_view/Cargo.toml#L1-L49))

### Open questions

- Which terminal engine passes the shared protocol corpus with the lowest build
  and maintenance cost?
- Which frontend holds the active PTY-size lease, and how does another frontend
  take it over?
- Do we need SGR pixel mouse coordinates in the first core protocol?
- What damage granularity gives smooth TUI and GPUI rendering without excess
  copying?
- Can GPUI meet our frame-time, text, IME, accessibility, and packaging targets
  on macOS, Linux, and Windows?
- Is a GPUI frontend worth its pre-1.0 churn after the TUI and core seams are
  stable?
- Are the Zed terminal crates legally and operationally acceptable, or should
  they remain design references only?
