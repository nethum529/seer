# Version skew of screen updates

Date: 2026-09-15
Issue: 418 (T-418). Related: 411 (R-411), 417 (T-417), ADR 0007,
docs/research/22-stream-adapter.md.

## Scope

R-411 will change the screen update message (ServerMsg::Cells). This
document records what each Seer process does today when the process on the
other side runs a different version. It covers the three pairs that carry
screen updates or the messages around them:

1. The local client and its own runtime.
2. The runtime and the broker.
3. The broker and a watching client.

For each pair it states whether the pair compares versions, what it
compares, what it does on a mismatch, and what the receiver does with an
unknown message kind, an unknown field in a known message, and a missing
field. The last section gives the rule that a change to the screen update
message must follow.

The source is origin/main at a6ee98f (version 0.5.6). Code reading gives the
paths. Tests that pass against this code confirm each receiver answer. They
are listed in the section Tests.

## Why skew happens

- seer update replaces the three binaries and stops no process
  (crates/seer/src/update.rs:32-47).
- The client starts the runtime with setsid, so the runtime is not in the
  broker process group (crates/seer/src/local.rs:147-181). The runtime runs
  until seer stop, seer leave, a SIGTERM, or a crash. ADR 0007 still says
  that the broker starts the runtime. After issue 338 the broker never starts
  a process (crates/seer-broker/src/runtime.rs:15-16).
- A new client attaches to a runtime that already runs. It connects to the
  socket and accepts any RuntimeReady. It compares the generation only for a
  runtime that it just started (crates/seer/src/local.rs:24-41, 99-114).
- seer start with a live broker prints "Server already running." and
  compares no version (crates/seer/src/start.rs:75-82).
- A guest runtime and a guest client connect to the host broker from another
  computer. The guest and the host update at different times.

## Wire format and decode rules

Every message is a 4 byte big endian length and a serde_json body
(crates/seer-core/src/proto/codec.rs:8-47). ClientMsg and ServerMsg are
externally tagged enums (crates/seer-core/src/proto/mod.rs:8-200). No type
uses deny_unknown_fields or serde(other). So every receiver decodes the
same way:

| Case | Decode result |
| --- | --- |
| Unknown message kind | Error (InvalidData) |
| Unknown field in a known message | Field is ignored |
| Missing field | Error, unless the field has serde(default) or is an Option |

Fields with serde(default) today: the Person fields
(proto/mod.rs:236-250), TerminalInfo.last_typist (proto/mod.rs:262), and
TerminalModes.alt_screen (crates/seer-core/src/terminal.rs:115).

The rows of a TerminalFrame use an untagged enum with two forms, runs or a
plain cell list (crates/seer-core/src/proto/frame_rows.rs:38-43). A row in
a third form is a decode error for the whole message.

What differs between receivers is the action after a decode error. The
pair sections record that action.

## Version checks today

| Check | Where | What it compares | On mismatch |
| --- | --- | --- | --- |
| Hello, client to broker | crates/seer-broker/src/server.rs:353-361, 384-398 | major.minor of CARGO_PKG_VERSION, as strings | Refused "version mismatch: server X, client Y. Run: seer update", connection ends |
| PublishRuntime, runtime to broker | crates/seer-broker/src/server.rs:362-370, 384-398 | Same | Same |
| TerminalCapabilities, client to runtime | crates/seer-runtime/src/user_session.rs:483-492, server/status.rs:21-37 | protocol_version == 1 | Refused "unsupported terminal capability version", connection stays |
| TerminalInput, client to runtime | crates/seer-runtime/src/pane_grid.rs:111-117 | protocol_version == 1 on every input | That input is refused |

The broker checks the version one time, when the connection opens. It
never checks an open connection again. The runtime stream messages
(RuntimeStream) and Join carry no version. TERMINAL_PROTOCOL_VERSION is 1
(crates/seer-core/src/terminal.rs:5). The client sends it at
crates/seer/src/tui.rs:79-89 and in each TerminalInput.

Result of the broker check:

| Difference | Result |
| --- | --- |
| Patch (0.5.6 and 0.5.7) | Accepted. No notice. |
| Minor (0.5.6 and 0.6.0) | Refused at connect. |
| Major | Refused at connect. |

## Pair 1: local client and its own runtime

Unix socket. The runtime sends screen updates for the person's own
terminals directly to the client. The client sends input, resize, and
capabilities.

Compares versions: no. Only the terminal protocol number in
TerminalCapabilities and TerminalInput, which is 1 in every release so far.

| Receiver | Unknown kind | Unknown field | Missing field |
| --- | --- | --- | --- |
| Runtime | Closes the connection with no reply (server.rs:56-69, connection.rs:65-68; before attach connection.rs:73-87) | Ignored, the message is applied | Closes the connection with no reply |
| Client | Window closes and prints "Server stopped." (tui_link.rs:38-53, tui.rs:141-161, commands.rs:247-249, 444-448) | Ignored, the frame shows | Window closes and prints "Server stopped." |

"Server stopped." is not true in these cases. The runtime still runs. Only
this connection ended.

### A new client and a runtime from before seer update

This is what the person sees today when they run seer attach (or bare
seer) after seer update, while the runtime from before the update still
runs:

- The window opens on the old runtime. No message tells the person that the
  runtime is older. Nothing compares the versions.
- When the old and new messages still decode on both sides, everything
  works as before.
- When the new client sends a message kind that the old runtime does not
  know, the runtime closes the connection. The window closes and prints
  "Server stopped." The runtime and its shells keep running.
- When the old runtime sends a message without a field that the new client
  requires, the window closes and prints "Server stopped."
- When a new client raises TERMINAL_PROTOCOL_VERSION, the old runtime shows
  "unsupported terminal capability version" and refuses every key the
  person types.
- The old runtime also publishes to the room with its old version. When the
  broker was restarted with another minor version, the broker refuses the
  runtime. runtime.log gets "runtime room connection ended: room refused
  this runtime: version mismatch ...", and the runtime tries again after 1 s,
  then up to every 30 s, without end (crates/seer-runtime/src/room.rs:56-98).
  The other people cannot see this person's terminals. The person sees no
  notice.

## Pair 2: runtime and broker

TCP over loopback for the host runtime, an iroh session for a guest
runtime (crates/seer-runtime/src/room.rs:126-154). The runtime keeps one
control link (PublishRuntime, then OpenStream requests). For each viewer
the broker asks for a stream, and the runtime opens it. The runtime sends
Tree, Cells, and Terminals on that stream. The broker sends Watch, Unwatch,
granted input, and queries.

Compares versions: yes, PublishRuntime, major.minor, at connect. Patch
difference: accepted. Minor difference: refused. The runtime logs the
refusal and tries again with backoff without end.

| Receiver | Unknown kind | Unknown field | Missing field |
| --- | --- | --- | --- |
| Broker, on a runtime stream | Ends that runtime stream (forwarding/transport.rs:114-148, forwarding.rs:145-167, 193-198) | Dropped. The broker decodes into its own types and encodes again, so the watcher never gets the field (forwarding.rs:344-405) | Ends that runtime stream |
| Runtime, on a room stream | Closes the stream with no reply (same code as pair 1, remote true, server/room_link.rs:11-26) | Ignored, the message is applied | Closes the stream with no reply |
| Runtime, on the control link | Ends the link, publishes again after backoff (room.rs:100-124) | Ignored | Ends the link, publishes again after backoff |

More behaviour of the broker:

- When a runtime stream ends, the broker tells the watcher nothing. The
  watcher keeps the last frame. The broker opens a new stream only on the
  next list refresh (every 1 s, and only when the watcher listed that
  person) or on the next Watch (forwarding.rs:96-107, 256-275). The runtime
  then sends the same bad message again, so the stream ends again.
- The broker forwards only Tree, Cells, Terminals, and Refused from a
  runtime. It ignores every other known kind (forwarding.rs:344-405).
- When it opens a runtime stream, the broker requires the exact order
  RuntimeReady, Tree, Terminals (forwarding/transport.rs:26-60).

## Pair 3: broker and a watching client

TCP or iroh. The broker sends Cells for each watched pane, and Tree,
Terminals, People, Presence, Grants. The client sends Watch, Unwatch,
Terminals, TypeInto, and room commands.

Compares versions: yes, Hello, major.minor, at connect. Patch difference:
accepted. Minor difference: refused. What the person sees on a refusal
depends on the command:

- seer join prints "refused: version mismatch: server X, client Y. Run:
  seer update" and stops.
- seer attach and bare seer open the window without the room and reach the
  room in the background (crates/seer/src/commands.rs:136-143,
  tui_link.rs:100-116). A refusal there is silent: the person sees only
  their own terminals, no people, and no notice.
- seer list shows the room as "unreachable" (commands.rs:190-214).

| Receiver | Unknown kind | Unknown field | Missing field |
| --- | --- | --- | --- |
| Broker | Before Welcome: Refused "invalid message" (server.rs:340-381). After Welcome: ends the client connection (forwarding.rs:123, forwarding/transport.rs:93-112) | Ignored, the message is applied | Ends the client connection |
| Client | Drops the room, shows "Room offline. Your terminals keep running." (tui.rs:156-179) | Ignored, the frame shows | Drops the room, same notice |

After the client drops the room, it connects again after 1 s, up to every
30 s (tui_link.rs:100-116), and shows "Room back." When the broker sends the
same message again, the room drops again. The person sees the room go
offline and come back in a loop.

A known kind that the broker does not serve for a client gets Refused
"unsupported client message", and the connection stays (forwarding.rs:252).

## Example: alt_screen, not yet released

PR 407 (commit fe53fef, after release 0.5.6) added TerminalModes.alt_screen
with serde(default). When the next release is a patch release:

- Old runtime to new client: the field is missing, the client uses false.
  This works.
- New runtime, old broker, new watching client: the old broker drops
  alt_screen when it encodes the frame again. The watcher always gets false
  and does not center a full screen frame. The broker accepts all three
  processes because only the patch differs.

This is the fault that the rule below prevents for R-411.

## Tests

Each test passes against origin/main a6ee98f with no code change.

| Receiver | Test |
| --- | --- |
| Runtime, from its own window | crates/seer-broker/tests/version_skew.rs a_runtime_ignores_unknown_fields_from_its_own_window_and_closes_it_on_other_changes |
| Runtime, from the room (room stream and control link, fake broker) | crates/seer-broker/tests/version_skew.rs a_runtime_ignores_unknown_fields_from_the_room_and_drops_the_link_on_other_changes |
| Broker, from a watching client | crates/seer-broker/tests/version_skew.rs the_broker_ignores_unknown_fields_from_a_watcher_and_ends_its_link_on_other_changes |
| Broker, from a runtime (fake runtime) | crates/seer-broker/tests/version_skew.rs the_broker_drops_unknown_fields_from_a_runtime_and_ends_its_stream_on_other_changes |
| Client, from its own runtime and from the room | crates/seer/src/tui/skew_tests.rs, three tests |
| Broker version check, patch and minor | crates/seer-broker/tests/handshake.rs the_room_accepts_another_patch_release_and_refuses_another_minor_release |

The fake broker and the fake runtime use ports from the operating system
and temporary directories. They do not touch port 7321.

## Follow-up issues

These are problems this document found. They are not fixed here.

1. The local link has no version check. A new client on an old runtime
   gets no notice, and a decode failure shows "Server stopped." while the
   runtime still runs.
2. After seer update across a minor version, seer stop on the host sends
   Hello to the old broker and gets "version mismatch ... Run: seer update"
   (crates/seer/src/commands/lifecycle.rs:23-37). The person just ran seer
   update. The message tells the wrong action for an old broker.
3. seer attach hides a version refusal from the room. The person sees no
   people and no reason.
4. When a runtime message does not decode, the broker ends the runtime
   stream and does not tell the watcher. The watcher sees a frozen screen,
   and the stream opens and fails again in a loop.
5. An old runtime that a new broker refuses tries again without end and the
   person gets no notice. Only runtime.log shows it.

## Rule for a change to the screen update message

A change to ServerMsg::Cells, TerminalFrame, or the row encoding is safe
during an upgrade only when it follows all of these rules:

1. Release it in a new minor version, never in a patch release. The broker
   refuses a client or a runtime of another minor version when it connects.
   So on every room path (runtime, broker, watching client) the three
   processes run the same minor version. A connection that is open was
   checked when it opened, and a running process never changes version.
   This part needs no new version check.
2. On the local link nothing compares versions, so:
   - The new client must still decode the old full frame, because it can
     attach to a runtime that started before seer update.
   - The new runtime must send the new form on the local link only to a
     window that said it can read it. With the code of today, the only
     safe way to say so is a new field with serde(default) in
     TerminalCapabilities. An old runtime ignores the unknown field
     (confirmed by test), and a new runtime reads a missing field as an old
     client. This field is a capability check, so in effect it is a new
     version check for the local link. Without it, no safe rule exists for
     the local link.
   - Do not raise TERMINAL_PROTOCOL_VERSION. An old runtime then refuses
     every key of a new client.
3. In a patch release, add only fields that have serde(default) and that no
   receiver needs. An old broker in the middle drops every field it does
   not know, and a missing required field ends the stream.
