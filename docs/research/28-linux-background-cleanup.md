# Linux background process lifetime

Date: 2026-10-25

## Contract and entry points

ADR 0009 supersedes the broker lifeline rule in ADR 0007. Local runtimes
own the shells. The broker owns room membership and routing. The desktop
and the TUI use the same local runtime and room commands.

| Action | Processes | Files |
| --- | --- | --- |
| Close the window, quit, or detach | The client ends. The runtime, shells, and hosted broker stay. | Keep room identities and layouts for reattach. |
| Lose the room connection or broker | Local shells stay. The runtime retries publication with a delay up to 30 seconds. | Keep the local socket and layout. |
| Stop the room on its host | Stop the broker and this person's local runtime and shell jobs. Other computers keep their shells. | Remove process files. Keep layouts, room membership, keys, and configuration for restore. |
| Stop a runtime through Processes or seer ps --stop | Stop the selected local runtime and its child process tree. | Remove its socket and PID file. Keep its layout. |
| Leave a room | Stop this person's local runtime and shell jobs. The room stays open. | Remove the local room identity and its runtime directory. |
| Start a new room | Replace the saved host identity. Stop its old local runtime and shell jobs. | Remove the replaced identity's runtime directory. Keep other rooms. |
| Restore a room | Keep the host ID and the saved people. Start replacement shells on the next attach if needed. | Keep the saved layout. A snapshot cannot restore process memory. |
| Crash or reboot | A broker crash does not stop local shells. A runtime crash closes its PTYs. A reboot ends all processes. | Abrupt exit can leave process files. The next stop removes stale files; attach replaces a stale socket. |

The desktop link uses seer::local::attach. RoomAction calls the same
lifecycle commands as the CLI. Closing the last desktop window quits the
client without a local stop. The CLI runtime launcher starts a separate
session and now reaps the child when it exits. The broker launcher also
starts a separate session and reaps its child. The broker never starts a
participant's runtime.

Relevant code: crates/seer/src/local.rs, start.rs, start/stop.rs,
commands/lifecycle.rs, commands/ps.rs, crates/seer-desktop/src/main.rs,
link.rs, window/room.rs, and crates/seer-runtime/src/room.rs.

## Proven failures and fixes

All reproductions used temporary HOME and XDG directories. CLI pseudo
terminals used script with output captured to a pipe. No window or screen
capture was needed.

| Reproduction | Failure before the fix | Fix |
| --- | --- | --- |
| Make runtime.pid a directory, then attach. | Startup reports failure but leaves a runtime listening without a usable PID record. | Roll back only the child started by this attach. Preserve an existing runtime if a second startup fails. |
| Remove a running runtime's socket, then stop its room. | Stop skips the runtime because it cannot connect. | Find the runtime by its process identity and socket argument. |
| Replace runtime.pid with another runtime's PID, then stop. | Stop signals the other runtime and leaves its own runtime running. | Use the validated process listing, not the PID file, to select the runtime. |
| Run a shell job that ignores HUP and TERM, then stop the runtime. | The direct shell is stopped, but its job survives. | Capture the child process tree before stopping and stop descendants from the leaves. Recheck identity before each signal. |
| Stop normally or with seer ps --stop. | runtime.pid remains after the runtime ends. | Remove runtime process files after a successful stop. Preserve the snapshot. |
| Kill the broker and runtime, then run stop. | Dead sockets, runtime.pid, and broker.pid remain. | Remove stale local process files even when the broker is unavailable. Keep the existing unavailable-room error. |
| Join, open local terminals, then leave. Repeat with a new identity. | Each departed identity leaves a runtime directory that cannot be reopened through the saved rooms. | Stop and remove local state before removing the saved identity. |
| Kill the broker, then start a new room. | The old owner runtime stays, and its identity and directory no longer appear in saved rooms. | Stop and remove the replaced host identity before saving the new owner. |

The new background_cleanup end-to-end tests failed before the fixes.
They also check that restore keeps the host ID and saved layout, that
another runtime survives a stale PID record, and that a failed second
startup preserves the existing runtime.

Validation on Linux:

- cargo test --workspace: 260 passed, 0 failed, 0 ignored.
- background_cleanup: 10 passed, 0 failed.
- cargo fmt --all -- --check: passed.
- cargo clippy --workspace --all-targets with warnings denied and a
  cognitive complexity threshold of 21: passed.
- cargo +nightly udeps --workspace --all-targets: no unused dependencies.
- Lizard on changed Rust files: maximum cyclomatic complexity 13.
- Changed Rust files: at most 421 lines.
- No runtime, broker, or desktop process remained after workspace tests.

## Limits

An abrupt kill cannot run file cleanup. Stale process files can therefore
exist until the next local stop or attach. A process that deliberately
ignores terminal hangup can survive a runtime crash; explicit runtime
stop captures its descendants before ending the runtime.

The current source does not create runtime-records. Its presence alone
does not show an active process or continuing growth. Saved layouts and
room keys are durable state, not running resources. This fix does not
scan and delete old folders with unknown ownership. The user's real
state and configuration were not changed.
