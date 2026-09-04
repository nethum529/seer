# Seer

Seer is the multiplayer and collaboration layer for coding agent terminals.
One person hosts a session. Friends join with one pasted line. Each person
gets their own tree of shells on the host.

Seer is not a herdr replacement. Seer keeps its own small tabs and panes, so
nobody must install herdr. When herdr or another multiplexer runs inside a
Seer pane, Seer sends the prefix keys through to it (issue 246, and
docs/adr/0005 when it lands).

Goals:
- Stronger multiplayer: see the people in the session and their terminals
  live (people drawer and peek, done in 0.4.0).
- Cross user terminal messaging with access grants (issue 267).
- An agent inbox for each user (issue 268).
- An activity manager (issue 269).
- macOS users as hosts (issue 272).
- Linux and macOS, as host and as client. No Windows.
- Later: a desktop application built on the Rust GPUI framework.

Issues 267 to 269 have the label needs-discussion. They are ideas for later.
A human must flesh them out before build.

herdr and luvus stay the reference code bases for the terminal core.

Research notes live in docs/research/.

See [CONTRIBUTING.md](CONTRIBUTING.md) before you make a change.

## Quickstart

This example uses one owner on Linux and one friend on macOS or Linux. The
friend needs only a terminal. The friend does not need Rust, Git, or a GitHub
account.

Today the host is Linux. A macOS host is issue 272. On the owner's Linux
machine, install Seer:

```sh
curl -fsSL https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh | sh
```

Restart the terminal if the installer asks you to. Then start Seer:

```sh
seer start
```

Seer prints a message in this format:

```text
Send this to a friend:

Paste this in Terminal:
curl -fsSL https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh | sh -s -- <capsule>
```

Send the printed message to the friend. The friend pastes the install line in
Terminal. The installer joins the session and opens a shell. The friend presses
Enter to use the default name or types a different name.

Use these commands after both people join:

```sh
seer
seer invite --hours 24
seer peek <name>
seer detach
seer stop
```

Bare seer attaches again after a detach.

Remote connections are encrypted end to end (see docs/adr/0002-builtin-connect.md).

## Build from source

Developers with Rust can build all workspace packages from the repository:

```sh
cargo build --workspace
```
