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

The people column puts you first. Select a person to see their live terminals.
A terminal takes its name from its foreground process, or uses "shell" for a
shell. A shell is idle; another foreground process is busy.

When you are alone, the screen shows a join line that expires in 24 hours.
Press c to request a clipboard copy. Clipboard access must be enabled in your
terminal. Press n to refresh the invite and open a new terminal.

## Controls

Main screen:

- j and k select a person.
- h and l focus a terminal.
- Enter opens the viewer.
- n creates your own terminal and opens it for input.
- / searches names.
- Esc asks to quit. q quits.
- Click a terminal to focus it. Double click to open it.
- Scroll over the people column or terminal area to scroll that area.

Viewer:

- Esc returns to the main screen.
- Tab opens the next terminal of the same person.
- f turns following output on or off.
- Input is enabled for your terminals and for people who gave you a grant.
- Esc, Tab, and f are viewer controls. Paste text to send these literal bytes.

Click another person's row to open their menu. It shows their presence, idle
time, terminals, watch actions, and your "can type here" grant for them. Use
j and k to select an action, Enter to watch, and Space to toggle the grant.
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
