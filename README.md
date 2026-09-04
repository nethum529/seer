# Seer

A Rust terminal runtime for coding agents, in the style of herdr and luvus,
with multiplayer as the long term goal.

Goals:
- Rust core, high performance.
- Mouse driven TUI.
- Compatible with herdr skills and the herdr plugin marketplace.
- Later: a desktop application built on the Rust GPUI framework.

Research notes live in docs/research/.

## Quickstart

This example uses one owner on Linux and one friend on macOS or Linux. The
friend needs only a terminal. The friend does not need Rust, Git, or a GitHub
account.

On the owner's Linux machine, install Seer:

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
