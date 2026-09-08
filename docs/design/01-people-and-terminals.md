# People and terminals: the seer TUI after the pivot

Status: decided by the owner on 2026-09-04. Wave one shipped as 0.4.1
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

Layout, left to right, top to bottom:

- Top bar, one row. Left: the word seer, then the owner name and server
  address, dim. Right: the count of people online. No key hints here.
- People column, 26 columns. Title "people". One row per person, name
  only. The owner's own row is first and reads "you". The selected row
  has a background. No presence text, no agent text in the list.
- Terminal area, the rest of the width. Title: the selected person's
  name. Header row inside, two groups with a 4 space gap: "online  5
  terminals" then "input: allowed" or "input: read only".
- Tab strip, one row under the header. One tab per terminal of the
  selected person, in tree order: the terminal name and its state. The
  selected tab has a surface0 background and accent text. Each tab has
  an x at its right edge. The strip ends with a + tab. The strip is
  always shown, also with one terminal.
- Live boxes: one bordered box per terminal of the selected person,
  laid out in a grid, two columns when the area is 80 columns or
  wider, one column below that. Each box title is the terminal name
  and its state (section 3.2). Each box shows the last rows of that
  terminal, scaled to fit, read only, updating live. The owner's own
  terminals show as boxes the same way. The grid is the default view
  whenever the selected person has one terminal or more.
- Footer, one row: key hints on the right. A notice shows on the left
  and clears after 3 seconds. Hints stay visible while a notice shows.

Spacing: 1 column margin at the left and right edge of the body, 1
column gap between the people column and the terminal area, padding 1
inside both blocks, one blank row after the header, 1 row and 1
column gap between boxes, minimum box height 8. When the boxes do not
fit, the grid scrolls and the last visible row reads "+N more" in
overlay0. The same cue applies to the people list.

Narrow widths: below 90 columns the people column narrows to 14.
Below 50 it hides and the footer shows "p people" to toggle it.

Focus: the accent border follows focus. When focus is on the people
list, the people column border is accent. When focus is in the grid,
the focused box border is accent and the people column is overlay0.

Keys: j and k select a person. h and l move focus between the boxes.
Enter opens the focused box in the viewer. n creates a new terminal in
the owner's own list, selects its tab, and opens it in the viewer with
input. Number keys 1 to 9 select a tab. x closes the selected terminal
at once and sends ClosePane. Slash opens a search over
names. Esc goes back, and on the main screen asks to quit. q quits.
p toggles the people column at narrow widths.

Empty state: when the selected person has no terminals, the area shows
"No terminals." centered, with the hint "n new terminal" for the
owner's own row.

Mouse: see section 2.5.

### 2.2 Viewer

The viewer draws inside the terminal area. The people column stays
visible on the left. The header and the tab strip stay above it. The
viewer is one bordered box that fills the rest of the area. Title: the
person name, the terminal name, then "read only" or "input". The
footer shows esc back first, tab next terminal, then the other keys,
q quit last. The same key and label style as the main footer.

A terminal in view is always live. There is no follow key and no
frozen state. The client sends Watch when a terminal comes into view
(a box in the grid or the viewer) and Unwatch when it leaves view
(another person selected, another tab, esc). Terminals out of view
keep running in the runtime, like herdr background panes.

Esc returns to the box grid of the same person.

Read only is the default for every terminal that is not the owner's.
The owner types into their own terminals with no marker.

If the terminal's owner gave the viewer the input grant, keys go to
that terminal. The sender is named to the receiving person and to the
agents in that terminal by a marker, see section 3.4. The client never
adds a marker.

Size: the viewer draws the frame at the remote size. When the remote
size is smaller than the viewer, the frame is placed at the top left
and the rest of the box is empty. The size rule for the pane itself is
in section 8, mission 14.

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
terminal. When the owner has terminals and is still alone, the join
line shows as one dim row under the header instead, with the hint c
copy. The people column shows only "you".

### 2.5 Mouse

Everything the keyboard can do, the mouse can do too, like herdr and
luvus. Left click is a plain click. Right click opens a context menu.

- Left click on a person row selects it. Double click opens the
  viewer on its first terminal.
- Left click on a box focuses it. Double click opens it in the viewer.
- Left click on a tab selects it. Click on the x of a tab closes that
  terminal at once. Click on the + tab creates a new
  terminal.
- Left click on a footer key hint presses that key.
- Left click outside an open menu closes it.
- Right click on a person row opens the person menu (2.3).
- Right click on a box or a tab opens a small menu: Open, Close. No
  split items. Seer has no client splits.
- Scroll wheel scrolls the people list, the box grid, and the viewer
  scrollback.
- Drag with the left button inside a box or the viewer selects text
  in the client. On release the selection is copied to the clipboard
  with OSC 52 and the footer notice says "Copied". Shift plus drag is
  left to the host terminal, so the terminal's own selection still
  works.

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
- Every widget background is Color::Reset, the terminal default. The
  owner's terminal is transparent and seer respects it. No solid fill
  anywhere. The top bar and the footer have no background. Only the
  selected people row, the selected tab, the person menu, the context
  menu, and the confirm dialog use surface0.
- Key letters in the footer are text with bold; their labels are
  subtext0. The same style in every footer.
- The header grant text is green for "input: allowed" and subtext0
  for "input: read only".
- Dialogs have padding 1, keys bold like the footer, and one blank
  row between the title and the keys.
- Borders are plain box drawing, the ratatui default. The focused box
  and the selected column use accent for the border. Every other
  border uses overlay0. The same rule as luvus border and
  border_focus.
- Box titles: the terminal name in blue for an agent, subtext0 for
  shell; the state after it, green for idle, yellow for busy. Titles
  sit in the top border with one space of padding each side.
- The people list: the selected row has surface0 background across
  the full column width. The owner's row "you" is in yellow. Names
  are text. The person the viewer shows keeps a small marker on the
  left of its row.
- The person menu has surface0 background, a plain border in
  overlay0, and the same row highlight as the list.
- The empty state and every dialog use the same palette. No color
  outside the palette anywhere in the client.
- Never draw a border with the default white. Never leave a widget
  without an explicit style.

## 5. What goes

- The client tree drawing in crates/seer/src/tui.rs draw and
  crates/seer/src/state.rs pane_rects. The client's own split and tab
  keys.
- The people drawer and its button. The people column replaces it.
- The PEEK banner. seer peek NAME opens the main screen on NAME.
- Peek and StopPeek in the protocol.
- The client keys that create tabs and splits. The runtime keeps the
  code for now.
- The multiplayer MVP tests T1 to T9 in crates/seer/tests that pin
  the tree layout, the drawer, or the peek banner. Replace them with
  the tests in section 6.

## 6. Contract tests

One test per part, at most. Through the CLI or a ratatui TestBackend.
Write each before its code.

- Main screen: with two people and three terminals for the selected
  one, the rendered buffer contains the two names, the three box
  titles, and the header "you may type: no".
- Watch: a client that sends Watch for another user's pane receives
  Cells for it with no grant.
- TypeInto: without the grant the broker answers Refused; with the
  grant the runtime receives the bytes with the marker prefix.
- Grant: SetGrant persists across a broker restart.
- First run: the owner alone with zero terminals sees the join line
  on the main screen. With one terminal the box grid shows.
- Tab strip: with three terminals the rendered buffer has three tabs
  and the + tab. Key 2 selects the second.
- Viewer inside the area: with the viewer open the buffer still has
  the people column title.
- Join: seer join with the whole pasted line joins with the capsule.
- Unknown command: seer bogus prints one line and exits 2.
- Backgrounds: no cell in the rendered buffer has a background other
  than Reset outside the selected row, the selected tab, a menu, or a
  dialog.

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
- Pane size rule for nitpick 13: a pane has the size of the smallest
  client that shows it, the owner's client or any watcher of that
  pane, like tmux. When nobody watches, the owner's size. Watch
  carries cols and rows. The size lease in the runtime applies it.
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
