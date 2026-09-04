# Seer architecture

Seer is a Rust terminal multiplexer for coding agents. The current multiplayer
MVP has one Linux server and Linux or macOS clients. Each person owns a private
workspace tree. Another person can view that tree through read-only peek.

## Crate map

The workspace has five crates.

| Crate | Job |
| --- | --- |
| `seer-core` | Defines frontend-neutral cells, colors, workspace trees, pane layout, protocol messages, and the length-prefixed JSON codec. |
| `seer-net` | Wraps iroh endpoint identity, encrypted remote transport, relay or direct connections, and blocking stream adapters. |
| `seer-broker` | Authenticates people, manages invitation seats, tracks clients, routes messages, supervises one runtime per person, and stores broker metadata. |
| `seer-runtime` | Owns one person's workspace tree, PTYs, shells, terminal grids, and runtime connections. It sends tree and cell updates. |
| `seer` | Provides the command-line client, start and update commands, local credential store, input adapter, and Ratatui TUI. It also builds the `seer-broker` and `seer-runtime` binaries. |

The crate list comes from the workspace `Cargo.toml`. Each job comes from the
crate manifest and its source entry point.

## Process model

One Linux host runs these server processes:

```text
Linux host
  seer-broker
    one seer-runtime process for Alice
      Alice workspace tree, PTYs, and shells
    one seer-runtime process for Bob
      Bob workspace tree, PTYs, and shells

Linux or macOS client
  seer command and TUI
```

The broker owns identity, routing, runtime supervision, invitations, client
attachments, and metadata. It does not own PTYs or agent child processes.

The broker starts a runtime when it first needs that person's tree. It connects
to the runtime through a Unix socket. The runtime starts one shell when the
first client connects to an empty tree. A pipe from the broker is the runtime
lifeline.

Detach closes one client connection. It does not stop the runtime or its live
PTYs. Cold restore after a broker or runtime restart is planned in issue 93. A
separate viewport and control lease for each attachment is planned in issue 95.

## Network and local transport

The broker listens on `127.0.0.1:7321` for local clients and tests. Remote
clients use iroh with ALPN `seer/1`. Iroh authenticates the server endpoint,
encrypts the connection, tries a direct path, and can use a relay.

The asynchronous iroh connection is bridged to a Unix stream. The existing
broker and client logic use the blocking stream interface. Remote transport
does not create a network interface, route, or DNS change.

## Protocol location

Protocol types are in `crates/seer-core/src/proto/mod.rs`:

- `ClientMsg` defines join, authentication, invitation, tree control, input,
  resize, peek, and detach requests.
- `ServerMsg` defines welcome, join, invitation, people, client, tree, frame,
  cell, refusal, and close replies.
- `Person` and `ClientInfo` are shared protocol records.

The codec is in `crates/seer-core/src/proto/codec.rs`. It uses a four-byte
big-endian length followed by JSON. One frame can be at most 16 MiB.

## Message flows

### Join

1. The client parses a one-use `SEER2` capsule and opens an iroh connection.
2. The client sends `ClientMsg::Join` with the seat token and display name.
3. The broker checks the unused seat and the unique name. It stores the new
   person and a hash of the new credential.
4. The broker sends `ServerMsg::Joined` with the person ID and credential.
5. The client saves the endpoint, person ID, name, and credential in its private
   `servers.toml` file.
6. The client starts the normal attach flow.

### Attach

1. The client loads the current server record and sends `ClientMsg::Hello` with
   its person ID, credential, and version.
2. The broker checks the major and minor version and authenticates the person.
3. The broker records a client attachment and sends `ServerMsg::Welcome`.
4. The broker connects to, or starts, that person's runtime.
5. The runtime sends the current `ServerMsg::Tree`. The broker forwards it to
   the client.

### Input

1. Crossterm reads a key. The client converts it to bytes and sends
   `ClientMsg::Input` with the focused pane ID.
2. The broker forwards owner input to the owner's runtime.
3. The runtime writes the bytes to the pane PTY.

### Cells

1. A runtime reader collects output bytes from each PTY.
2. The runtime poll driver feeds new bytes to an Alacritty terminal grid.
3. The runtime sends `ServerMsg::Cells` to its connected broker streams.
4. The broker forwards the cell rows to clients.
5. The client stores the rows and Ratatui draws them with Crossterm.

### Peek

1. The client authenticates as itself and requests the people list.
2. The client resolves an exact display name and sends `ClientMsg::Peek` with
   the target person ID and workspace ID.
3. The broker checks that the target exists. It changes only this client route
   from the owner's runtime to the target runtime.
4. The target runtime marks that connection read-only and sends its tree and
   cell updates.
5. The TUI blocks input and resize. The broker drops input during peek. The
   runtime also drops all mutating messages on the read-only connection.

## Settled decisions

Do not reopen these decisions:

- PTYs and shells run on the server. See [session lifecycle research](research/10-session-lifecycle.md).
- The process model is one broker and one runtime per person. See [session lifecycle research](research/10-session-lifecycle.md).
- The TUI uses Ratatui and Crossterm. See [rendering research](research/04-rendering-and-gpui.md).
- Linux and macOS are supported. Windows is not supported. See [ADR 0001](adr/0001-one-line-install-and-join.md).
- GPUI is a far-future frontend. Core types stay frontend-neutral. See [rendering research](research/04-rendering-and-gpui.md).
- Remote connections use iroh and expose only Seer. See [ADR 0002](adr/0002-builtin-connect.md), [security research](research/16-service-only-security.md), and [terminal sharing research](research/17-terminal-sharing-prior-art.md).
- Join uses one pasted install line and a one-use seat. See [ADR 0001](adr/0001-one-line-install-and-join.md) and [command UX research](research/11-session-command-ux.md).

The [research index](research/README.md) links the wider design record.
