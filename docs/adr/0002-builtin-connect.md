# ADR 0002: Built in connection without Tailscale

Date: 2026-09-02
Status: Accepted

## Context

ADR 0001 made the friend flow one pasted line, but it needs
Tailscale. Tailscale gives the friend network access to the whole
owner machine: ssh, every open port, every service. The owner
wants the friend to reach the Seer service and nothing else. The
owner also wants no account and no VPN install on the friend side.

Six research documents were written first. Read them for the
reasons behind each decision:

- docs/research/12-tailscale-anatomy.md
- docs/research/13-open-source-mesh-comparison.md
- docs/research/14-rust-nat-traversal-libraries.md
- docs/research/15-relays-and-rendezvous.md
- docs/research/16-service-only-security.md
- docs/research/17-terminal-sharing-prior-art.md

The order of goals is fixed: seamless for the friend, then secure,
then simple for the owner.

## Decision

### Transport: iroh

Seer uses the iroh crate for every remote connection. Reasons:

- The friend does nothing after the pasted line. No account, no
  VPN, no port forward, no second tool.
- The owner runs nothing but seer start. iroh finds a path through
  home NAT by hole punching, and falls back to a relay when the
  direct path fails. The public relays run by the iroh project are
  the default. No VPS is needed.
- The owner endpoint is a 32 byte public key. iroh authenticates it
  inside QUIC on every connection. A relay carries only encrypted
  bytes and cannot read terminal data.
- iroh is Rust, MIT or Apache-2.0, released often, and gives one
  API for identity, encryption, NAT traversal, relay, and lookup.
  The other candidates need a Go sidecar, a VPS relay, or a custom
  STUN and relay stack. See docs/research/14.

Rejected: mesh VPN products (whole machine access, Go sidecar),
Noise or TLS over a public TCP port (the owner would need a port
forward or a VPS relay), custom QUIC with own relay (duplicates
iroh), WireGuard libraries (wrong boundary, no traversal).

### Service only boundary

- iroh runs inside the seer binaries only. It never creates a
  network interface, a route, or a DNS change.
- The broker accepts one ALPN, seer/1. Every other protocol is
  refused before any Seer code runs.
- The friend reaches the Seer protocol only. Seer offers no port
  forwarding, no file transfer, no generic command channel.
- The friend still gets a shell on the owner machine by design.
  The OS identity rules below control that shell. See
  docs/research/08 and docs/research/09.

### OS identity deployment

- broker.toml has an os_users table. Each key is an exact Seer
  person name. Each value is an existing OS account name. Seer
  does not create OS accounts.
- A Linux broker can launch its own account without extra rights.
  A multi-user broker runs as a system service with rights to set
  groups, GID, and UID. It sets HOME, USER, LOGNAME, and SHELL from
  the target account. Each runtime uses that account shell and owns
  a mode 0700 state directory.
- A missing, unsafe, unknown, or unauthorized mapping refuses that
  runtime connection. The broker continues to serve other users.
- The multiplayer MVP server runs on Linux. macOS is a supported
  client platform and uses the identities on the Linux server.
  A macOS broker service is not supported in this release.

### Identity and invite

- seer start creates one stable iroh secret key for the broker and
  stores it in the state directory with mode 0600. Its public key
  is the server identity. The public key never changes across
  restarts unless the owner deletes the file.
- The capsule format becomes SEER2. It carries the server public
  key and the single use seat token. It carries no names and no
  network address. Example shape: SEER2-<key>-<token>, about 115
  characters.
- The client pins the server key from the capsule. Later attaches
  use the saved key from servers.toml. A different key is a hard
  failure.
- The seat token stays the join authorization. The existing person
  credential stays the attach authorization for this release.
  Binding a client device key is a later decision.

### Connection path

- The broker keeps a TCP listener on 127.0.0.1 for local clients
  and tests. It no longer listens on other interfaces.
- Remote clients dial the server public key with iroh. iroh starts
  through a relay and moves to a direct path when one works. The
  friend never sees this.
- Local blocking code does not change. One thread runs the iroh
  endpoint on tokio. Each QUIC stream is bridged to a Unix socket
  pair, and the blocking side of the pair is given to the existing
  broker and client code.

### Owner flow

- seer start prints a message with one friend step: paste the
  install line with the capsule. The Tailscale invite step is gone.
- seer invite prints the same message with a new capsule.
- All Tailscale code, checks, and messages are removed from seer,
  install.sh, and the README.

### Release

- Linux x86_64 builds on the owner PC with cargo-zigbuild as
  before.
- iroh links Apple frameworks (SystemConfiguration, Security,
  Foundation) through its macOS network monitoring code. A cross
  build from Linux is not possible without an Apple SDK. Decision
  (2026-09-02, issue 172): a GitHub Actions macOS runner in
  seer-releases builds the darwin assets. scripts/release.sh
  triggers the workflow after it publishes the Linux asset.

### Version

- The workspace version becomes 0.2.0. The major.minor handshake
  check refuses old clients and prints the seer update hint.

## Done line

Owner side: one install line, seer start, one message sent.
Friend side: paste one line, press Enter for the name. The friend
runs uname and sees Linux. The owner peeks the friend's shell.
No Tailscale on either machine.

## Consequences

- New crate seer-net wraps iroh behind a blocking API.
- New dependencies: iroh, tokio (one runtime thread).
- Binary size grows by about 12 MiB.
- The Tailscale section of ADR 0001 is superseded by this ADR.
  The rest of ADR 0001 stays in force.
- Not decided here: client device keys, revocation of a person,
  a self hosted relay setting, TLS for the loopback listener.
