# People and terminals: the seer TUI after the pivot

Status: terminal-first option 3 approved on 2026-09-08. The earlier
people-and-terminals design was decided on 2026-09-04. Wave one shipped as 0.4.1
the same day. The owner then used it and gave 15 nitpicks. Sections 2,
4, 6, 7 and 8 carry those decisions. The broker and runtime process
model does not change.

## 1. What seer is now

Seer is not a terminal multiplexer. Herdr is the multiplexer on each
machine. Seer is a window onto everyone's terminals on one server, and
an inbox that agents use to talk to each other across users.

Decisions. Do not reopen them.

- No human to human messaging. No threads, no chat, no composer.
- Viewing is never gated. Everyone on the server can watch every
  terminal of every person.
- The only grant is "can type here": a person lets another person send
  input to their terminals. No block feature. No other grant.
- Shells and agents run on the seer server, in the runtime per user,
  the same as today. A person opens their own terminals from the TUI.
- The agent inbox is wave two. This spec covers wave one only: the TUI
  with people, live boxes, the viewer, and the input grant.
- Everything is ratatui plus crossterm. The TUI must look as good as
  herdr and luvus. See section 4.

## 2. Screens

All screens are one ratatui app, started by bare seer, seer attach,
and seer peek NAME (peek opens with NAME selected).

### 2.1 Main screen

The owner approved terminal-first option 3 on 2026-09-08.

- The terminal uses the full window. No persistent sidebar, tab row,
  context row, or footer takes space from it.
- One terminal has no frame. Multiple terminals retain the live grid,
  two columns at widths of 80 or more, one column below that. Grid
  focus, scrolling, and the more cue remain available.
- A small handle at the middle of the left edge opens the people
  panel. The p key opens it in overview. The panel draws over the
  terminal and does not change the terminal size. Its heading closes
  it. Escape or a click outside also closes it.
- The people panel heading has a pin control. When pinned, the people
  panel becomes a column at the left edge, and the terminal uses the
  area beside it. A visible pinned column alone does not take terminal
  input. Clicking search or a menu gives that control the keys until
  terminal content is clicked again. A pinned column stays through panel
  changes, terminal changes, and outside clicks. Below 60 columns the
  pinned column collapses to the handle, and the terminal uses the
  whole window. A wider window shows the column again. The pin stays
  in the current window only. A new window starts unpinned.
- The people panel shows names, presence, input permission, typing
  status, and owner identity. The / key searches. The m key or a right
  click opens the person menu. Enter opens the selected terminal.
- A small chip at the top right shows seer and Your terminal, Read
  only, or Can type. The input grant determines the remote access
  text. On narrow screens, the access text takes priority over seer.
- Click the chip or press s in overview for the session panel. It
  shows the selected person, access, host, terminal list, and actions.
  Use j/k or arrows to select, Enter or a click to act. The list
  scrolls with selection or the mouse wheel. Short screens reduce
  the header to keep actions reachable.
- The session panel supports 1-9 terminal selection, n new terminal,
  x close terminal, c copy invite when available, a back row from the
  viewer, and q quit. Escape or the x at its top closes it. Back is a
  mouse action: no key leaves a terminal, because every key belongs to
  that terminal.
  New, close, and copy actions apply to the owner's own terminals.
- People and session panels are mutually exclusive. Opening or
  closing a panel clears text selection. Panels consume key, paste,
  and mouse input so it cannot reach the terminal behind them.
  Person menus, context menus, and the quit dialog take precedence.
- Notices draw over a small part of the last row and clear after
  three seconds. They do not reserve a row.

The overview keeps j/k people, h/l boxes, Enter view, n new terminal,
x close, 1-9 terminal selection, / find, q quit, and Escape to ask
before quitting. The existing person menu keeps its grant actions.
A click on terminal content in the grid opens that terminal and gives
it the keys. A click on the search row in the people panel starts a
search, the same as the / key.

### 2.2 Viewer

The viewer uses the full window, with the same handle and access chip.
Every key goes to the terminal when no panel or dialog is open,
including p, s, q, Escape, and Ctrl+B. Seer keeps no prefix key, so a
tmux or screen session in the terminal gets its own prefix. The mouse
opens a panel without leaving the viewer, and the back row in the
session panel returns to the overview. Escape closes an open panel and
keeps the viewer.
The terminal cursor is hidden while a panel or dialog owns input.

Watch, Resize, cursor, selection, and mouse targets use the actual
content rectangle. Opening or closing either overlay does not send a
changed Watch or Resize. A real window resize updates those dimensions.
Hidden terminals are unwatched and keep running. Remote input still
requires the owner's grant; the client does not add typing markers.

The owner's active window controls the size of each visible terminal.
If the owner opens several windows, a changed size, input, or gained
focus selects that window. Passive viewers and lost focus do not take
control. Read-only viewers cannot shrink a terminal the owner shows.
When only remote viewers show a terminal, their smallest size applies.
When nobody shows it, the last active owner size applies.

### 2.3 Person menu

Opened by right click on a person row, or by m on the selected row.
Drawn as a dropdown under that row, over the terminal area. Width is
the longest row plus 4, at least 22, capped to the area. Rows:

- presence and the time since last activity
- one row per terminal: name and state
- a separator
- one "watch NAME" row per terminal; enter or click opens the viewer
- a separator
- one toggle: "[x] can type here". It means: this person may type into
  my terminals. Space or click toggles it. Only the owner's side is
  shown. What the other person allows you shows in the header of the
  terminal area.

Esc or a click outside closes the menu. The menu is not shown for the
owner's own row.

### 2.4 First run

The first run block shows only when the owner has zero terminals. It
is one centered block, at most 80 columns wide: "Nobody else is here
yet.", the sentence "Send this line to a friend. Your friend pastes it
in a terminal. It expires in 24 hours.", the join line with the
capsule token on its own line, and the hints c copy and n new
terminal. When the owner has terminals, the session panel offers c copy invite.
The people panel shows the owner as "you".

### 2.5 Mouse

- The edge handle opens people. The top-right chip opens session.
- A click outside a panel closes it. A click on terminal content also
  gives the keys to that terminal, so one click is enough. A click
  elsewhere, and a click on the panel heading or its x, is consumed.
- A click on a menu, a dialog, or a panel action is consumed. Its
  release cannot reach the terminal behind it.
- Click a person to select them. Double click opens their first
  terminal. Right click opens the person menu.
- Click a terminal row in session to open it. Click a session action
  to create or close a terminal, copy an invite, go back, or quit.
- Click a grid tile to focus it. Double click opens the viewer.
  Right click opens the terminal context menu.
- The wheel scrolls people, session actions, the grid, or scrollback.
- Drag inside terminal content selects text. Release requests an
  OSC 52 clipboard copy. Shift plus drag stays with the host terminal.
  Open panels and dialogs take input before text selection.

### 2.6 CLI

- seer join accepts the whole pasted invite line. It takes the last
  token that starts with SEER as the capsule and ignores the rest.
- seer with an unknown command prints one line: "unknown command: X.
  Run seer help." and exits 2. seer help prints the help.
- Bare seer on the host machine, with an empty server store and an
  existing broker.toml, attaches as the owner from broker.toml. When
  the store is empty and there is no broker.toml, it prints "Paste the
  line the owner sent you." and the recovery hint "seer start creates
  the owner entry".

## 3. Model and protocol

### 3.1 Terminals

A terminal is a pane in the person's runtime. Wave one reuses the
existing Tree, Tab and Pane types and the Cells stream. The client
shows the panes of the person's current workspace as the terminal
list, in tree order. The runtime keeps its tree and pane grid. The
client no longer draws a tree; the grid of live boxes is the client's
own layout. CreateTab and SplitPane stay in the protocol for the
runtime, but the TUI exposes only "n new terminal", which sends
CreateTab.

### 3.2 Terminal name and state

The runtime already reads the foreground process name of a pane on
Linux. The terminal name is that foreground name when it is not a
shell (claude, codex, vim), else "shell". The state is "idle" when
the foreground process is a shell, else "busy". Wave one has no
working, blocked or done detection. That is a later ticket.

### 3.3 Protocol changes in seer-core

Add to ClientMsg:

- Watch { user, pane }: start the Cells stream for one pane of any
  user. Many watches may be open at once. Viewing needs no grant.
- Unwatch { user, pane }.
- Terminals { user }: ask for the list.
- TypeInto { user, pane, bytes }: input into another person's pane.
  The broker refuses it without the grant.
- SetGrant { user, can_type }: the sender lets user type into the
  sender's terminals.

Add to ServerMsg:

- Terminals { user, terminals: Vec<TerminalInfo> } with TerminalInfo
  { pane, name, state, cols, rows }. Sent on request and on change.
- Presence { user, online, idle_secs }. Sent on change.
- Grants { can_type_here: Vec<user>, you_may_type_into: Vec<user> }.
  Sent after Welcome and on change.

Cells { pane, rows } gains a user field so a client can watch panes of
several people. Peek and StopPeek are removed. Person gains
online: bool and idle_secs: u64.

### 3.4 Broker

- Grants live in the broker state dir, one JSON file per user, the
  list of user ids that may type into that user's terminals.
- The broker enforces TypeInto against that file. On refusal it sends
  Refused { reason: "NAME has not let you type" }.
- The marker "[seer: NAME] " must not enter the PTY input bytes. It
  breaks the typed command. Wave one did this and it is a bug. The
  broker forwards the bytes unchanged and passes the sender name with
  them. The runtime shows the sender name in the terminal state (name
  of the last external typist, cleared after 5 seconds) so the client
  can show "NAME is typing" in the box title and the viewer title, and
  so agents can read it from the activity feed in wave two. Mission
  14 in section 8 owns this change.
- Watch is forwarded to the target runtime the same way Peek was.

## 4. Look

The owner's rule: it must look as good as herdr and luvus, not lazy
white lines. Concrete rules:

- One palette module in the client, crates/seer/src/theme.rs, with a
  Palette struct and two constructors: mocha (default) and terminal
  (16 colors, used when the terminal has no truecolor, detected with
  the COLORTERM variable).
- Mocha values, the same as herdr's Catppuccin Mocha: accent and blue
  137,180,250; panel_bg 24,24,37; surface0 49,50,68; surface1
  69,71,90; overlay0 108,112,134; text 205,214,244; subtext0
  166,173,200; mauve 203,166,247; green 166,227,161; yellow
  249,226,175; red 243,139,168; teal 148,226,213; peach 250,179,135.
- Terminal content keeps Color::Reset, the terminal default. The
  owner's terminal is transparent and seer respects it. The people overlay uses
  panel_bg, with surface0 for its selected row. The handle, chip,
  session panel, notices, menus, and dialogs use surface0.
- The session panel uses accent for its selected action. The chip
  uses accent for Your terminal and Can type, subtext0 for Read only.
- The people panel uses green for allowed and subtext0 for read only.
- Dialogs have padding 1, bold keys, and one blank
  row between the title and the keys.
- Borders are plain box drawing, the ratatui default. The focused box
  and the selected column use accent for the border. Every other
  border uses overlay0. The same rule as luvus border and
  border_focus.
- Box titles: the terminal name in blue for an agent, subtext0 for
  shell; the state after it, green for idle, yellow for busy. Titles
  sit in the top border with one space of padding each side.
- The people list: the selected row has surface0 background across
  the full column width, with no break in it. Every name is text,
  including the owner's row "you"; an offline name is subtext0. The
  person the viewer shows is bold. The heading, the "<" and the ">"
  toggle marks, the online count, and the owner identity at the
  bottom of the column are overlay0.
- The person menu has surface0 background, a plain border in
  overlay0, and the same row highlight as the list.
- The empty state and every dialog use the same palette. No color
  outside the palette anywhere in the client.
- Never draw a border with the default white. Never leave a widget
  without an explicit style.

## 5. What wave one removed

- The client tree drawing in crates/seer/src/tui.rs draw and
  crates/seer/src/state.rs pane_rects. The client's own split and tab
  keys.
- The original people drawer was replaced by a column in wave one.
  The approved terminal-first layout now uses an overlay panel.
- The PEEK banner. seer peek NAME opens the main screen on NAME.
- Peek and StopPeek in the protocol.
- The client keys that create tabs and splits. The runtime keeps the
  code for now.
- The multiplayer MVP tests T1 to T9 in crates/seer/tests that pin
  the tree layout, the drawer, or the peek banner. Replace them with
  the tests in section 6.

## 6. Contract tests

Write tests for user-facing contracts before their implementation.

- The full window is available to one terminal or a viewer. Grid
  terminals still receive their actual tile content sizes.
- People and session panels open and close by key and click. Escape
  closes a panel before the quit dialog. Outside clicks are consumed.
- A viewer forwards normal keys through the real pane location when
  panels are closed. Open panels consume keys and paste. Watching
  from a person menu dismisses the people panel and returns input.
- Session actions select, create, and close real terminals and quit.
  The selected action remains visible on a short screen.
- Opening or closing panels sends no changed Watch or Resize. A real
  window resize sends the correct new dimensions.
- Remote access stays visible on narrow screens and follows grants.
- First run retains the invite. Terminal backgrounds retain Reset
  outside the small controls, panels, menus, notices, and dialogs.
- Existing CLI, broker viewing, grant, join, and input contracts remain
  in effect. Granted input reaches the runtime without a text prefix.

## 7. Wave one bundle, done

Merged as PR 294 on 2026-09-04. The parts were: theme and chrome,
protocol and broker, main screen, viewer, person menu and grants,
cleanup.

## 8. Nitpick bundle and missions

### 8.1 Nitpick bundle, in order

One Astra agent, one branch, one PR. Client crate only, plus Cargo.toml
and the release script. Each part is one commit.

1. CLI: section 2.6. Files: crates/seer/src/cli.rs, commands.rs,
   commands/selection.rs, start.rs.
2. Look: section 4 and the spacing, header, focus, narrow width,
   scroll cue, empty state, and dialog rules in 2.1. Files: render.rs,
   theme.rs, viewer.rs, person_menu.rs.
3. Terminals: own terminals as boxes, first run rule in 2.4, the tab
   strip, number keys, x closes at once with ClosePane. Files: render.rs,
   state.rs, input.rs, tui.rs, tui_navigation.rs.
4. Viewer: inside the terminal area, follow key removed, Watch on
   show and Unwatch on hide, footer order. Files: viewer.rs, state.rs,
   input.rs, tui.rs.
5. Mouse: section 2.5 without text selection. Files: input.rs,
   person_menu.rs, render.rs, viewer.rs.
6. Text selection and copy: the drag rule in 2.5. Files: input.rs,
   viewer.rs, render.rs, state.rs.
7. Weight: a [profile.release] in Cargo.toml with strip = true,
   lto = "fat", codegen-units = 1, panic = "abort". The release script
   prints the size of each binary and the crate count from cargo tree
   into the release notes.

Hard limits: no new dependencies, no changes under crates/seer-broker
or crates/seer-runtime or crates/seer-core, no inbox work. Mechanical
compile fixes forced by a rename are in scope. New behavior that is
not in this document is out.

### 8.2 Mission 14: input and output path

One Astra agent alone. Target: under 50 ms from a key press by user A
to the screen update on both A's viewer and B's own screen, on a
real network. Owns crates/seer-broker forwarding and grants,
crates/seer-runtime, and crates/seer-core protocol changes.

- Measure first: a timestamp per hop, a latency test over the real
  network, numbers in the PR.
- Known cause: send_granted opens a new runtime socket per key, takes
  two locks, does a handshake, and blocks up to 5 seconds. Keep one
  runtime stream per target user in the forwarding session.
- The marker rule in section 3.4.
- Pane size rule: use the active owner window described in section 2.2.
  When only remote viewers show the pane, use their smallest size.
  When nobody watches, keep the last active owner size. Watch carries
  cols and rows.
- Check the Cells path: poll interval, full frame versus dirty rows,
  socket buffering, and the client render tick.

### 8.3 Mission 15: feel and behavior, closer to herdr

One Claude Fable agent alone, after the bundle is dispatched. A QA
pass on the real app with two users, a list of every rough edge with
the herdr behavior for the same action, then fixes in scope of this
document. Herdr source at /home/nethum/Projects/_research/herdr is a
read only reference. Nitpicks 1 to 13 belong to the bundle. Mission
15 takes what the bundle and mission 14 do not name and must not
touch files those two change while they are open.
