# Seer development

## Prerequisites

Install these tools:

- Git.
- GitHub CLI `gh`, authenticated for this repository.
- A current Rust toolchain with Cargo and Rust 2024 edition support.
- `cargo-udeps` for the unused dependency gate.

The workspace builds on Linux and macOS. The client runs on both systems. The
broker, runtimes, PTYs, and shells run on Linux. `seer start` reports an error
on macOS.

## Build

Build every package and binary:

```sh
cargo build --workspace
```

The three development binaries are:

```text
target/debug/seer
target/debug/seer-broker
target/debug/seer-runtime
```

Build the complete workspace before you run from source. The `seer start`
command finds `seer-broker` next to the `seer` binary. The broker finds
`seer-runtime` in the same directory.

## Required checks

Run the formatter check:

```sh
cargo fmt --all --check
```

Run Clippy with warnings denied:

```sh
cargo clippy --workspace --all-targets -- -D warnings
```

Run the workspace tests:

```sh
cargo test --workspace --no-fail-fast
```

Check for unused dependencies:

```sh
cargo udeps --workspace --all-targets
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the code, test, comment, and
complexity rules.

## Run a local two-person session

This check needs Linux because the server starts only on Linux. Use three
terminal windows. Use short and separate config paths so the two clients have
different saved identities.

First, build the workspace:

```sh
cargo build --workspace
```

In terminal 1, start the owner server:

```sh
XDG_CONFIG_HOME=/tmp/seer-owner-config \
XDG_STATE_HOME=/tmp/seer-owner-state \
target/debug/seer start
```

Enter the owner name. Copy the capsule from the printed install command. A seat
works once. If this is not the first start, create a new seat:

```sh
XDG_CONFIG_HOME=/tmp/seer-owner-config target/debug/seer invite
```

In terminal 2, join as the second person. Replace `<capsule>` with the copied
value:

```sh
XDG_CONFIG_HOME=/tmp/seer-friend-config \
target/debug/seer join '<capsule>'
```

Enter a different name, such as `bob`. Join saves the new identity and opens
Bob's tree. The runtime creates the first shell for the empty tree.

In terminal 3, attach as the owner:

```sh
XDG_CONFIG_HOME=/tmp/seer-owner-config target/debug/seer
```

Open another owner terminal to list people or view Bob's tree:

```sh
XDG_CONFIG_HOME=/tmp/seer-owner-config target/debug/seer list
XDG_CONFIG_HOME=/tmp/seer-owner-config target/debug/seer peek bob
```

Peek is read-only. Press Control-Q to leave a TUI. Run bare `seer` with the same
XDG_CONFIG_HOME value to attach again.

The normal installed flow is in the [README quickstart](../README.md#quickstart).
The installer source is `scripts/install.sh`. It installs `seer`, `seer-broker`,
and `seer-runtime` in `~/.local/bin`.

## Common failures

### Port 7321 is busy

`seer start` and its start tests use `127.0.0.1:7321`. Stop the process that
owns this port before you start the broker or run the test suite. Do not stop an
unknown process.

On macOS, port 7321 must also be free when the workspace runs the start tests.

### A Unix socket path is too long

The broker limits runtime socket paths to 99 bytes. A long XDG_RUNTIME_DIR can
make the path invalid. Use a short private runtime directory, such as a path
under `/tmp`, and run the command again.

### A source binary is missing

If `seer start` cannot find `seer-broker`, or the broker cannot find
`seer-runtime`, run this command again:

```sh
cargo build --workspace
```

Run `target/debug/seer`. Do not move only one binary to another directory.

### The remote server is not reachable

The built-in remote path needs the owner broker to be running and both machines
to have internet access. Ask the owner to run `seer start`. Then use a new
one-use invitation if the old seat was already used or expired.

### The client and server versions do not match

The broker requires matching major and minor versions. Rebuild all workspace
binaries from the same commit. Installed users can run `seer update`.

## Work with a coding agent

AGENTS.md and CLAUDE.md contain the draft policy for coding agents. Use this
flow:

1. Give the agent the issue number, branch name, base branch, and exact file
   scope.
2. Tell the agent to read AGENTS.md and CLAUDE.md before it starts.
3. Tell the agent which research files and source files contain the facts.
4. Keep one issue and one branch for the change.
5. Review the agent's diff and staged file list. Exclude unrelated files and
   scratch artifacts.
6. Run every required gate yourself before a push.
7. Use `git commit -s`. Do not add a co-author line.
8. Give each numbered review blocker back to the agent. Commit and push fixes
   to the same branch and pull request.

The contributor remains responsible for scope, facts, verification, and the
pull request.
