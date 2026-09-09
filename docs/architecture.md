# Seer architecture

Seer shares live terminals between people in a trusted room. Each person runs
shells on their own computer. The room broker routes shared views and input;
it does not start those shells. ADR 0009 defines this model from version 0.5.0.

## Crate map

| Crate | Job |
| --- | --- |
| seer-core | Workspace trees, terminal cells, protocol messages, and the JSON codec. |
| seer-net | Iroh identity, encrypted connections, multiple streams, and blocking adapters. |
| seer-broker | Identity, invitations, grants, people, runtime registrations, and shared routing. |
| seer-runtime | One person's local workspace tree, PTYs, shells, snapshots, and terminal frames. |
| seer | CLI, local runtime startup, saved room credentials, and the Ratatui TUI. Builds all three binaries. |

## Process model

```text
Room host (currently Linux)
  seer-broker

Alice's computer (Linux or macOS)
  seer window <--- private local socket ---> seer-runtime ---> Alice's shells
       |                                        |
       +--- room connection ---> broker <--- outbound publication

Bob's computer (Linux or macOS)
  seer window <--- private local socket ---> seer-runtime ---> Bob's shells
       |                                        |
       +--- room connection ---> broker <--- outbound publication
```

The first window starts its local runtime under the local OS account. Windows
for the same room identity share that runtime. A startup lock prevents
concurrent windows from starting separate runtimes. The state directory and
socket are private and scoped by the person ID minted for that room.

The runtime uses the local shell, current directory, tools, and provider
logins. The broker does not receive credential files. It is trusted: it can
read shared terminal frames and the input it routes.

## Lifetime and persistence

Closing or detaching a window leaves its local runtime and shells running.
The runtime also survives room loss or broker restart. Own typing and terminal
management use the local socket and do not wait for a room handshake.

The runtime republishes after a lost connection. The window separately
reconnects to the room and requests current trees and frames. It discards
unsent remote input; it does not replay keystrokes. One active runtime can
publish for a person in a room. A second computer is refused without replacing
the first runtime.

The seer stop command ends the local runtime for the selected room. It also
stops the broker if this computer hosts that room. If the room selection is
missing, it can still stop the locally hosted broker. Other participants'
processes stay on their computers.

A runtime restart restores a saved layout with new shells. It cannot restore
process memory. See the [upgrade steps](../README.md#upgrade-from-047-to-050)
for the transition from hosted terminals.

## Transport and protocol

The broker supports local TCP, normally 127.0.0.1:7321, and remote iroh
connections with ALPN seer/1. Iroh uses encrypted direct or relayed connections.
Each network connection ends at the broker; this is not protection from a
hostile room host. No network interface or machine-wide route is installed.

A Session in seer-net carries multiple streams over one iroh connection.
Blocking Unix stream adapters connect the async transport to the runtime and
broker code. Own terminal traffic uses a private Unix socket directly.

ClientMsg and ServerMsg live in crates/seer-core/src/proto/mod.rs. The codec
uses a four-byte big-endian length followed by JSON, with a 16 MiB frame limit.
The broker, clients, and published runtimes must match major and minor versions.

## Message flows

### Join and local attach

1. The client exchanges a one-use SEER2 invitation for a room identity and
   credential. It saves these in its private servers.toml.
2. It starts or finds its local runtime and attaches through the local socket.
3. The runtime sends the current tree and frames. The window can type locally.
4. In the background, the window authenticates to the room for people and
   grants. The runtime independently publishes its terminal service.

### Publish and watch

1. The runtime sends PublishRuntime with its credential and process generation
   on an outbound control connection.
2. A viewer asks the broker for another person's terminals or frames.
3. The broker sends OpenStream with a fresh random token to that runtime.
4. The runtime opens a stream and writes RuntimeStream with the token first.
   Iroh delivers a new stream only after the opener writes. This order avoids
   a deadlock. TCP uses the same exchange on a new connection.
5. The broker checks the credential and pending token, then routes the stream
   to the viewer. The runtime sends the current frame even for a quiet shell.

### Input and frames

Own input, create, split, close, focus, and resize go through the local socket.
PTY output updates the runtime's terminal grid, then subscribed windows.

Remote viewing goes through the broker. A remote stream can watch and list
terminals. GrantedInput is allowed only after the broker checks the owner's
can-type-here grant. Revoke blocks subsequent input; it cannot recall bytes
already written to a PTY. A remote stream cannot create, close, focus, split,
or resize terminals.

## Design record

- [ADR 0009: participant-owned terminals](adr/0009-participant-owned-terminals.md)
  supersedes the server execution and broker lifetime rules in ADRs 0004 and 0007.
- [ADR 0008: user picker](adr/0008-top-right-user-picker.md) defines the top right
  people control. Terminal keys go to the shell until an explicit menu opens.
- [ADR 0002: built-in connection](adr/0002-builtin-connect.md) records iroh.
- [Research index](research/README.md) records earlier design evidence.
