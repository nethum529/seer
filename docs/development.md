# Seer development

## Prerequisites

Install these tools:

- Git.
- GitHub CLI `gh`, authenticated for this repository.
- A current Rust toolchain with Cargo and Rust 2024 edition support.
- `cargo-udeps` for the unused dependency gate.

The client and local runtime target Linux and macOS. Each participant runs
their own PTYs and shells. The room host command, seer start, currently runs
on Linux only. A Linux test run does not verify the macOS build or execution.

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
command finds seer-broker next to the seer binary. The client finds
seer-runtime in the same directory, then on PATH.

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

This check needs a Linux room host. Use three terminal windows. Give each
person separate config and state paths. Set both paths on every command so
the check cannot attach to or stop your normal runtime. Use a machine with
port 7321 free, or an isolated test environment.

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
XDG_CONFIG_HOME=/tmp/seer-owner-config XDG_STATE_HOME=/tmp/seer-owner-state target/debug/seer invite
```

In terminal 2, join as the second person. Replace `<capsule>` with the copied
value:

```sh
XDG_CONFIG_HOME=/tmp/seer-friend-config \
XDG_STATE_HOME=/tmp/seer-friend-state \
target/debug/seer join '<capsule>'
```

Enter a different name, such as `bob`. Join saves the new identity and opens
Bob's tree. The runtime creates the first shell for the empty tree.

In terminal 3, attach as the owner:

```sh
XDG_CONFIG_HOME=/tmp/seer-owner-config XDG_STATE_HOME=/tmp/seer-owner-state target/debug/seer
```

Open another owner terminal to list people or view Bob's tree:

```sh
XDG_CONFIG_HOME=/tmp/seer-owner-config XDG_STATE_HOME=/tmp/seer-owner-state target/debug/seer list
XDG_CONFIG_HOME=/tmp/seer-owner-config XDG_STATE_HOME=/tmp/seer-owner-state target/debug/seer peek bob
```

Watch Bob through the top right picker. Bob can grant you typing permission
from your person menu. Revoke it and check that further input is blocked.
Right click the top right control and select Quit to close a window. Run bare
seer with the same config and state paths to return to the same local shells.
Stop each test runtime with seer stop and that person's config and state paths.

The normal installed flow is in the [README quickstart](../README.md#quickstart).
The installer source is `scripts/install.sh`. It installs `seer`, `seer-broker`,
and `seer-runtime` in `~/.local/bin`.

## Common failures

### Port 7321 is busy

seer start and its start tests use 127.0.0.1:7321. Keep live sessions running.
On Linux, run the tests in a private network namespace instead:

```sh
cargo test -p seer --test start --test restore --no-run
unshare -Urn sh -c 'ip link set lo up && cargo test --offline -p seer --test start --test restore -- --test-threads=1'
```

This requires unprivileged user namespaces and the ip command. Run the other
crate tests normally. Do not skip the start and restore tests when the live
port is busy.

On macOS, port 7321 must also be free when the workspace runs the start tests.

### A Unix socket path is too long

Local runtime sockets live under XDG_STATE_HOME/seer/runtimes/PERSON/socket,
or ~/.local/state/seer/runtimes/PERSON/socket. Unix socket paths have an OS
length limit. Use a short private XDG_STATE_HOME for test fixtures.

### A source binary is missing

If `seer start` cannot find `seer-broker`, or the client cannot find
`seer-runtime`, run this command again:

```sh
cargo build --workspace
```

Run `target/debug/seer`. Do not move only one binary to another directory.

### The remote server is not reachable

Shared views need the room broker and a network path to it. Your own local
terminals remain usable without the room. With a saved identity, Seer retries
the connection automatically. A new invitation is needed only for joining,
not for reconnecting an existing member.

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
