# ADR 0001: One line install and join

Date: 2026-09-01
Status: Accepted

## Context

The two-person test failed at step one. The owner ran cargo install
from the wrong directory. The friend would need Rust, git, and a
GitHub login before the first command. Seer must work for people
who are not developers. The user set the target: the friend types
one line and gets a shell.

Decisions below come from a review with the owner on 2026-09-01.
They are settled. Do not reopen them without the owner.

## Decisions

### Floor

- The flow must work when the friend has only a terminal and a
  Tailscale invite. No Rust, no git, no GitHub account.
- The same flow must also work on a developer machine.
- Targets: macOS arm64, macOS x86_64, Linux x86_64.

### Distribution

- Prebuilt binaries live on a public GitHub repository:
  nethum529/seer-releases. It holds install.sh and releases only.
- This code repository stays private.
- The owner builds releases on the Linux PC with cargo-zigbuild.
  No CI. One script, seer-release, cross-builds the three targets,
  tags the version from Cargo.toml, and uploads with gh release.
- The install line never carries a version. It always installs the
  latest release.

### Friend flow

- The friend types one pasted line. Example:
  curl -fsSL https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh | sh -s -- <capsule>
- The line installs seer, checks Tailscale, joins, and opens the
  session. There are no other steps.
- The installer puts the binary in ~/.local/bin. No sudo. It adds
  the directory to the shell rc file when it is missing. It runs the
  join in the same script, so the first run works before a new
  shell. It prints one line that says to restart the terminal for
  later use.
- Name prompt: "Name [<system username>]: ". Enter accepts the
  default. Invite lines still contain no names.
- After the join, the friend sees a live shell on the owner's Linux
  machine. The runtime creates one tab with one shell when a person
  attaches to an empty tree. No new TUI key. No protocol change.
- Later, the friend types seer with no arguments. Bare seer attaches
  to the current server. If no server is joined, it says to paste the
  line from the owner. seer attach stays as an alias.

### Tailscale

- The installer checks Tailscale and guides. It never configures.
- Checks: Tailscale installed, running, and the server endpoint is a
  Tailscale address the friend can see.
- If a check fails, the line stops and prints exactly what to do:
  the install link, then accept the invite from the owner. The friend
  pastes the same line again after that.
- Seer never runs tailscale up, never creates keys, never changes
  Tailscale settings.

### Owner flow

- The owner uses the same install line, then seer start. The owner
  never needs the repository checkout.
- seer start prints a ready-to-paste message with two steps for the
  friend: 1. accept the Tailscale invite, 2. paste the curl line in
  Terminal. It also prints one line for the owner: send the
  Tailscale invite from the admin page.
- seer invite prints the same message with a new capsule.
- The broker survives the owner closing the terminal. It does not
  survive a reboot. After a reboot the owner runs seer start again.
  Pane persistence across restart is wave 2 and is not approved.

### Updates

- seer update downloads the latest release over itself.
- A version mismatch between client and server prints one clear
  message that names the fix (seer update).

### Verification

- The Linux binary and install.sh are verified on this PC with a
  fresh user directory.
- The friend's first run is the acceptance test for macOS. If it
  fails, the friend sends a screenshot and the owner re-releases.

## Done line

Owner side: one install line, seer start, one Tailscale invite, one
message sent. Friend side: accept the Tailscale invite, paste one
line, press Enter for the name. The friend runs uname and sees
Linux. The owner peeks the friend's shell. Nothing else.

## Consequences

- New public repository nethum529/seer-releases.
- New files in this repository: install.sh source, a release script,
  a first-shell-on-attach change in seer-runtime, bare seer as attach,
  seer update, name default, the two-step message in seer start.
- README Quickstart is replaced by the one-line flow.
- Cross-build tools (zig, cargo-zigbuild) become a build requirement
  for the owner's release PC only.
- Not decided here: TLS, public port without Tailscale, systemd
  service, persistence across restart.
