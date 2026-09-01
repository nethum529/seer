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
private repository must give the friend collaborator access.

On the owner's Linux machine, run these commands from the repository:

```sh
cargo install --path crates/seer
cargo install --path crates/seer-broker
cargo install --path crates/seer-runtime
seer start
```

All three land in `~/.cargo/bin`, which must be on `PATH`.

Copy the printed `seer join <capsule>` line and send it to the friend through
a private channel.

On the friend's machine, install Rust and log in to GitHub for Git:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
gh auth login
cargo install --git https://github.com/nethum529/placeholder-herdrlike seer
```

The friend can use an SSH key instead of `gh auth login`. Paste the full
`seer join <capsule>` line, then type a name when Seer asks for it.

Use these commands after both people join:

```sh
seer peek <name>
seer detach
seer attach
```

Seer uses plain TCP and has no TLS yet. Use Tailscale or a LAN.
