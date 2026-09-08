# Seer

Seer lets people share live terminals in a room. One person hosts the room.
Friends join with one pasted line. Each person's terminals run on their own
computer, with their own files, tools, shell, and provider logins.

Everyone in the room can watch. The only grant is "can type here": it lets
another person type into your terminals. The broker checks grants, which
survive a room restart. The room host is trusted and can read shared output
and input.

Closing a Seer window or losing the room connection leaves your local shells
running. Open Seer again to return to them. A computer or runtime restart
restores the saved layout with new shells, not the old running processes.

The room host command runs on Linux or macOS. Participants can also use Linux
or macOS. An agent inbox is planned; this build has no chat or human messaging.
Herdr and luvus remain the reference code bases for the terminal core.

## Quickstart

Install Seer on the owner's Linux or macOS machine:

    curl -fsSL https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh | sh

Restart the terminal if the installer asks you to. Start the server:

    seer start

Send the printed seer join line to a friend who has Seer. The friend pastes it
into Terminal and selects a name. A friend without Seer pastes the install line
printed under it. They do not need Rust, Git, or a GitHub account.

Open the people and terminals screen:

    seer

The terminal keeps the whole window. The control at the top right shows
"Seer" and the name of the person you look at. Left click it to open the
picker, then select a person to see their live terminals. A terminal takes
its name from its foreground process, or uses "shell" for a shell. A shell
is idle; another foreground process is busy.

When you are alone, the screen shows a join line that expires in 24 hours.
Right click the control at the top right to copy the invite or open a
terminal. Clipboard access must be enabled in your terminal.

## Controls

Main screen:

- Type or paste to open the selected terminal and send the first input.
- All terminal keys, including q, Escape, and Ctrl+B, go to the terminal.
- Left click the top right control to open the picker. It shows your typing
  permission for the person you look at, every person with the host marked,
  and the server address. Five names fill one column, then a new column is
  added to the left. Select a person to see their terminals.
- Right click the same control for the session panel. Use it to open, close,
  or select a terminal, copy the invite, go back, and quit Seer.
- Click a terminal to open it for typing. Drag to select and copy text.
- Scroll over the picker to see more columns, or over the terminal area.

Viewer:

- Use the mouse to open the session panel and select Back for the overview.
- Input is enabled for your terminals and for people who gave you a grant.
- Seer has no prefix. Keys belong to the terminal until you open a panel
  or a menu.

Right click another person's row in the picker to open their menu. It shows
their presence, idle time, terminals, watch actions, and your "can type here"
grant for them. Use j and k to select an action, Enter to watch, and Space to
toggle the grant.
The checkbox changes when the broker confirms it. Esc or a click outside
closes the menu.

Seer controls use Catppuccin Mocha when COLORTERM is truecolor. Otherwise they
use a 16-color palette. Focused borders use the accent color. Other borders use
overlay0. Terminal output keeps your own terminal colors: the default text and
background colors and the 16 ANSI colors of your terminal profile, also when you
watch another person. Explicit RGB colors and color indexes 16 to 255 stay
unchanged.

## Server commands

- seer attach opens the people and terminals screen.
- seer invite --hours 24 creates a new invitation.
- seer list lists saved servers and people.
- seer detach disconnects a selected client without stopping its terminals.
- seer stop ends your local terminals for the selected room. If this computer
  hosts that room, it also stops the room server. Other people's shells keep
  running.
- seer start opens a new room. The people of the old room are removed. An
  invitation made before a stop stays valid. seer start --restore reopens the
  old room with its members.

Remote connections to the broker are encrypted. See docs/adr/0002-builtin-connect.md.
Research notes are in docs/research/. Read CONTRIBUTING.md before a change.

## Build from source

    cargo build --workspace

## Upgrade from 0.4.7 to 0.5.0

Upgrade the broker and every participant's three Seer binaries together.
Major and minor versions must match. Room identity, membership, seats, and
grants stay in the existing room store.

Before the host stops the old 0.4.7 broker, everyone must save their work and
finish any jobs in the old hosted terminals. The old broker controls those
processes; this upgrade cannot move a running shell to another computer.

Start the upgraded room and open Seer on each participant's computer. Their
first local terminals start fresh. Seer does not copy host files, provider
credentials, or host terminal snapshots. Keep any host files you still need
and transfer them through your normal file workflow.

Only one computer can publish terminals for a person in a room at a time.
Use a separate room identity for a second person. To move your own identity
to another computer, stop its local runtime on the first computer first.
See [ADR 0009](docs/adr/0009-participant-owned-terminals.md).
