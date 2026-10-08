# ADR 0010: Seer becomes a GPUI desktop app

Date: 2026-10-07
Status: Accepted

Supersedes these earlier decisions:

- AGENTS.md, settled decision "TUI stack: ratatui plus crossterm."
- AGENTS.md, settled decision "GPUI desktop app is far future. Keep
  core types frontend-neutral, nothing more."
- docs/research/04-rendering-and-gpui.md, decision "Treat GPUI as a
  later frontend. Do not make it the first renderer.", and the row
  "GPUI desktop frontend: Later" in docs/research/05-comparison.md.
- docs/design/01-people-and-terminals.md, section 1, decision
  "Everything is ratatui plus crossterm."

ADR 0008 (the top right picker) stays in force for the TUI. It does
not apply to the window.

## Context

Seer has one interface today: a ratatui TUI that runs inside the
terminal app of each person. The roadmap items R-449 to R-459 record
what that costs. A grant needs a right click menu with j, k, and
Space. A new terminal needs the session panel. A friend joins with a
long line that they paste into a terminal. The host copies a join
line only when the terminal app permits clipboard access. A version
mismatch can only print a message.

The owner approved a desktop window design on 2026-10-06 and
2026-10-07 in a variate session. Research from the same session:
Zed is built on GPUI. Warp uses its own Rust UI framework, not GPUI.

Research 04 advised against GPUI as the first renderer, because GPUI
is before version 1.0 and its README warns about frequent breaking
changes. That risk stays. The TUI came first, as research 04 advised.
The window is now the second frontend.

## Decision

### Seer becomes a native desktop app

- Seer becomes a native GPUI desktop app for macOS and Linux. Windows
  stays out of scope.
- The window is a new client of the same local runtime and the same
  broker as the TUI. Own terminal work goes over the private local
  socket. Room work goes over the broker. ADR 0009 does not change.
  The move does not change the protocol rules, the grants, or the
  transport.
- Core types stay frontend-neutral. GPUI types do not go into
  seer-core or into the protocol.
- The pin rule of research 04 stays: pin every GPUI crate to exact
  versions or to one Git revision. No floating branch.

### The approved window design

A copy of the approved design is in
[docs/design/03-gpui-window.html](../design/03-gpui-window.html).
Open it in a browser. The room, people, and terminal output in it are
sample data.

- A left sidebar. At the top, a person dropdown picks whose terminals
  you see. Under it, the terminals of that person are vertical tabs.
- A filled "New terminal" button under the dropdown. A settings gear
  at the bottom of the sidebar, next to the room name and address.
- No room people list in the sidebar. The owner removed it, because
  the dropdown already switches people.
- Terminal output shows as rounded cards with hover actions. Your own
  terminal has a floating input bar. The terminal of another person
  has a read only bar.
- The palette is gpui-component Default Dark: neutral grays and a
  white primary. No transparency or blur.

The design does not cover the invite, join, room, and settings
screens. The settings sheet in the copy is a placeholder.

### The TUI during and after the move

- During the move, the TUI stays. Bare seer, seer attach, and seer
  peek open it, as today. Each release ships it.
- The TUI and the window can show the same room and the same
  terminals at the same time. Neither one breaks the other.
- New user facing features go to the window. The TUI gets bug fixes.
  A ticket can still ask for a TUI change.
- The move ends when the window holds every row of the feature
  checklist that has a place in the window, on macOS and on Linux, in
  a release. Then a separate ticket removes the TUI, and ratatui and
  crossterm with it.
- After the TUI is removed, seer, seer attach, and seer peek open the
  window. R-457 decides how the command finds and starts the app.

### The CLI commands during and after the move

- During the move, every command in seer help stays and works as
  today.
- After the move, every command stays. Only the commands that open
  the TUI change: they open the window. Commands are the path for
  scripts, for agents in a terminal, and for a computer with no
  display.
- The help lines about the TUI keys and the top right control go away
  with the TUI.

### The feature checklist

[docs/design/04-gpui-feature-checklist.md](../design/04-gpui-feature-checklist.md)
has one row for each user facing feature in the README and in seer
help. Each row gives the place in the window, or the reason that the
feature stays a command only. A row with no place yet names the
roadmap item that decides it.

The checklist is a separate file because it changes each time a
roadmap item decides a row. This ADR does not change.

- A PR that decides or builds a row updates that row.
- A PR that adds a user facing feature adds its row.

## Consequences

- AGENTS.md points to this ADR. Agents build GPUI work and do not
  send new features to the TUI.
- GPUI upgrades can break the build. The pin rule limits this. Each
  GPUI upgrade is its own change.
- The window draws the terminal itself, so the font, colors, and keys
  of the terminal app of the person do not apply. R-459 tracks
  import.
- An app that starts from the macOS Dock or a Linux desktop launcher
  gets a smaller environment than a terminal. For this reason T-448
  requires the login shell of the person.
- Two clients exist until the TUI is removed. A change to the runtime
  or the broker must work for both.
- The installer puts only command line binaries in ~/.local/bin.
  Desktop packaging and signing are new work in R-457.
- Not decided here: the invite, join, and room screens, the contents
  of settings, packaging, and the keys that the window keeps for
  itself.
