# People and terminals: the seer TUI after the pivot

Status: decided by the owner on 2026-09-04. This is the spec for the
first build wave. It replaces the multiplayer MVP test cases T1 to T9
for the client. The broker and runtime process model does not change.

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
  address, dim. Right: the count of people online, then key hints.
- People column, 26 columns. Title "people". One row per person, name
  only. The owner's own row is first and reads "you". The selected row
  has a background. No presence text, no agent text in the list.
- Terminal area, the rest of the width. Title: the selected person's
  name. Header row inside: presence (online or away with the time),
  the number of terminals, and "you may type: yes" or "no".
- Live boxes: one bordered box per terminal of the selected person,
  laid out in a grid, two columns when the area is 80 columns or
  wider, one column below that. Each box title is the terminal name
  and its state (section 3.2). Each box shows the last rows of that
  terminal, scaled to fit, read only, updating live.
- Footer, one row: key hints.

Keys: j and k select a person. h and l move focus between the boxes.
Enter opens the focused box in the viewer. n creates a new terminal in
the owner's own list and opens it in the viewer with input. Slash
opens a search over names. Esc goes back, and on the main screen asks
to quit. q quits.

Mouse: left click on a person row opens the person menu (2.3). Left
click on a box focuses it. Double click opens it. Scroll wheel scrolls
the people list or the box grid.

### 2.2 Viewer

One terminal full screen inside one border. Title: the person name,
the terminal name, then "read only" or "input". The footer shows esc
back, tab next terminal of the same person, f follow output on or
off.

Read only is the default for every terminal that is not the owner's.
The owner types into their own terminals with no marker.

If the terminal's owner gave the viewer the input grant, keys go to
that terminal. Every line the viewer sends is prefixed on the wire
with the marker "[seer: NAME] " where NAME is the sender's name. The
marker is plain ASCII so the person and any agent in that terminal
can read who typed. The broker adds the marker. The client never adds
it.

### 2.3 Person menu

Opened by left click on a person row. Drawn as a dropdown under that
row, 22 columns wide, over the terminal area. Rows:

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

When the owner is alone on the server, the terminal area shows one
centered block: "Nobody else is here yet.", the sentence "Send this
line to a friend. It expires in 24 hours.", the join line, and the
hints c copy and n new invite. The people column shows only "you".

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
- The broker prefixes the marker to every line of TypeInto bytes
  before it forwards them to the runtime. A line ends with CR or LF.
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
- Top bar and footer use panel_bg as background. Key letters in the
  footer are text with bold; their labels are subtext0.
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
- The person menu has panel_bg background, a plain border in
  overlay0, and the same row highlight as the list.
- The marker inside a terminal is written by the broker as plain
  text. The client does not color it.
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
- First run: the owner alone sees the join line on the main screen.

## 7. Bundle parts, in order

1. Theme and chrome: theme.rs, top bar, footer, people column widget,
   border rules. No behavior change yet.
2. Protocol and broker: section 3.3 and 3.4, plus the broker tests.
3. Main screen: people column, live boxes grid, keys, mouse focus,
   new terminal, first run state.
4. Viewer: full screen, tab, follow, input when granted.
5. Person menu and grants: the dropdown, the toggle, presence.
6. Cleanup: section 5, help text, README, old tests removed.

Each part is one commit or one PR on the same branch. The hard limits
for the implementer: no new dependencies, no changes under
crates/seer-runtime except the foreground name and state fields, no
inbox work, no presence beyond online and idle time.
