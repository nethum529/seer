# ADR 0009: Terminals run on each participant's own computer

Date: 2026-09-09
Status: Accepted

Supersedes the server-only execution rule in AGENTS.md, ADR 0004 (one
OS account for each person on the host) and ADR 0007 (the broker owns
the runtime process lifecycle).

## Context

Until 0.4.7 every PTY ran on the room host. The broker started one
runtime for each person, under an OS account on the host, and gave it
the read end of a pipe as stdin. The person got a shell on the host,
with the host files, the host tools and no provider login of their
own.

Issue 338 records the product problem. Seer must let Alice run her own
Claude or Codex, with her own files and her own account, while Bob
runs his own tools on his own computer, and each can watch the other.
The owner approved participant-owned execution on 2026-09-09.

Research is complete and lives in the issue 338 approach note. This
ADR records the product contract and the process rules only. It does
not repeat the survey of other projects.

## Decision

### Each person runs a local runtime

A runtime runs on the computer of the person who owns it, under that
person's own OS account. It uses that account's shell, PATH, files,
installed tools and provider logins. Seer never copies a credential
file and never sends one to the room. Joining a room does not run
anything on the host.

The Seer window starts the runtime for its own person and room, and
connects to it over a private Unix socket in that person's own state
directory. The socket path is scoped by room and by person, so
changing rooms cannot publish the terminals of the previous room.

### The window has two routes

Own terminal work goes over the local socket: frames, input, create,
split, close, focus and resize. Room work goes over the broker:
people, grants, invitations, the terminal list of another person,
watching another person and typing into another person.

The room route is optional. The local route alone renders and types
in the owner's own terminals. Routes in crates/seer/src/routes.rs
decides the route from the message.

### The broker keeps people and routes shared frames

The broker owns identity, seats, people and grants. It no longer
starts a process, and it no longer maps a person to an OS account.

Each runtime opens one outbound iroh connection to the broker and
sends PublishRuntime with the person credential and its generation.
The broker records that live connection as the control stream.

When a viewer asks to watch or to list the terminals of that person,
the broker sends OpenStream with a fresh random token on the control
stream. The runtime then opens a new stream and writes RuntimeStream
with that token first. The broker matches the token against the
pending requests for that person and hands the stream to the viewer
that asked.

The runtime is the opener on purpose. iroh gives a stream to the far
side only after the opener writes, so a broker-opened stream that
waited for the runtime to speak first would deadlock. Making the
runtime open and write first removes that ordering problem instead of
depending on getting it right. The same request and token mechanism
works over TCP, where the runtime dials a second connection.

The broker still decides when a stream exists. It is the one that
asks, and it authenticates the answer: the token must match a pending
request and the stream must carry that person's credential.

### Remote streams carry a restricted role

A broker-opened stream is read only. It may watch, list terminals,
ask for status and targets, and carry GrantedInput that the broker
approved. It may not create, split, close or focus a terminal, and it
may not resize. Those stay on the private local socket.

This protects the ordinary protocol. It does not make a hostile
broker harmless. The broker terminates the connections, so it reads
shared frames and forwarded input, and it could fabricate input. The
room host stays trusted. Moving execution to each computer does not
change that, and relabelling the broker as a relay would not either.

### Grants do not change

The existing per-person can-type-here grant stays. The broker checks
it before it forwards input, and refuses new input after a revoke.
Bytes already delivered to a PTY cannot be recalled. There are no
per-pane permissions, no writer leases and no command replay.

### Lifetime does not depend on the room

The broker no longer gives the runtime a lifeline pipe. A broker that
stops, a room connection that drops and a window that closes all
leave the local shells running and the local typing working. Remote
views are marked unavailable and remote input stops.

On reconnect the client authenticates again and resubscribes. The
runtime republishes. Both take a fresh tree and fresh frames.
Keystrokes that were not sent are discarded, not replayed.

The runtime generation is separate from the network registration. A
generation lasts for the life of that process. A reconnect replaces
the registration only; it never replaces the PTYs and never starts a
second set of shells.

One published runtime for each person in each room. A second computer
that tries to publish for the same person is refused. It never stops
or replaces the work on the first computer.

### Stopping is local

seer stop ends the runtime of this computer for the current room, and
the broker as well when this computer hosts the room. It cannot stop
the shells of another participant. Snapshot restore reopens the
layout with new shells. It cannot resume the memory of a process that
already ended.

### Upgrade

Protocol 0.5.0. The existing major and minor handshake check makes
the broker, the clients and the published runtimes upgrade together.
Rooms keep their identity, people, seats and grants. Local terminal
trees start fresh. The upgrade does not import a host snapshot, and
it does not stop an old hosted process. People must finish or stop
those themselves.

## Consequences

- A person needs Seer installed on their own computer to run a
  terminal. There is no fallback that starts a shell on the host.
- The broker no longer needs a privileged account, os_users config or
  the OS identity code. All of it is removed.
- The host computer no longer holds the files or the logins of the
  other participants.
- A person can be visible in the room with no runtime published, for
  example before their first window opens. Their terminals show as
  unavailable.
- Watching costs one stream for each viewer and target. The broker
  asks for it and the runtime opens it.
