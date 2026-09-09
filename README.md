# Seer

Seer is a window onto everyone's terminals on one server. One person hosts.
Friends join with one pasted line. Each person opens their own terminals on
the server and can watch anyone else's terminals without a grant.

The only grant is "can type here". It lets another person send input to your
terminals. The broker checks the grant and adds [seer: NAME] to each line of
guest input. Your own input has no marker. Grants survive a server restart.

An agent inbox is planned for a later wave. This build has no chat or human
messaging. Herdr is the multiplexer; herdr and luvus remain the reference
code bases for the terminal core.

The current host is Linux. Clients run on Linux and macOS. A macOS host is
planned. Shells and agents run on the server, never on the client.

## Quickstart

Install Seer on the owner's Linux machine:

    curl -fsSL https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh | sh

Restart the terminal if the installer asks you to. Start the server:

    seer start

Send the printed install and join line to a friend. The friend pastes it into
Terminal and selects a name. They do not need Rust, Git, or a GitHub account.

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

The screen uses Catppuccin Mocha when COLORTERM is truecolor. Otherwise it uses
a 16-color palette. Focused borders use the accent color. Other borders use
overlay0. Terminal output colors map to the selected palette.

## Server commands

- seer attach opens the people and terminals screen.
- seer invite --hours 24 creates a new invitation.
- seer list lists saved servers and people.
- seer detach disconnects a selected client without stopping its terminals.
- seer stop stops the local server.

Remote connections are encrypted end to end. See docs/adr/0002-builtin-connect.md.
Research notes are in docs/research/. Read CONTRIBUTING.md before a change.

## Build from source

    cargo build --workspace
