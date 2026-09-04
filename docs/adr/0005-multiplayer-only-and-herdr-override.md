# ADR 0005: Seer is the multiplayer layer and herdr overrides it

Date: 2026-09-04
Status: Accepted

## Context

Seer has its own tabs and panes. herdr is a terminal multiplexer for
coding agents, and many people run it already. A person can start
herdr inside a Seer pane. Then two multiplexers listen for the same
prefix key and the same tab and pane keys. Only one can win.

Issue 246 records the decision by the owner on 2026-09-04: Seer keeps
its own tabs and panes so nobody must use herdr. When herdr runs
inside a pane, herdr owns tabs and panes and Seer steps back.

The runtime already knows the foreground process of a PTY. It reads
tcgetpgrp on the PTY of the focused pane of the active tab and maps
the process group to a name. On Linux the name comes from
/proc/<pid>/comm. See process_name in crates/seer-runtime/src/pty.rs.
On other systems the name is an empty string. The people status work
(issue 248, ADR 0006) needed the same fact, so it is built once and
both features read it.

## Decision

Seer is only the multiplayer layer. It is not a herdr replacement.
Seer does not try to be a full multiplexer for people who want herdr.

### How the client learns that herdr is in front

The broker sends a People message to every attached client once per
second. Each Person row carries a foreground field. The client keeps
its own user id and copies the foreground value of its own row.
See note_people and herdr_in_front in crates/seer/src/state.rs.
herdr_in_front is true when the foreground name is exactly "herdr".
No protocol message was added.

### What Seer passes through

Key handling is in handle_key in crates/seer/src/tui.rs. When
herdr_in_front is true, and the action is not ToggleDrawer and not
Bytes, Seer does not run the action. It calls forward_prefix in
crates/seer/src/tui_navigation.rs instead. forward_prefix sends two
terminal inputs to the focused pane: first the literal prefix byte
U+0002, then the bytes of the key that followed the prefix. herdr
then sees prefix plus key and acts on it.

The keys that pass through are the tab and pane keys: create tab,
split pane, close pane, next tab, previous tab, focus pane by
direction, and focus pane by number.

### What Seer keeps

- Prefix u opens and closes the people drawer (InputAction
  ToggleDrawer). This is Seer only.
- Prefix prefix sends one literal prefix byte (InputAction Bytes).
  This is unchanged.
- Ctrl-q detaches the client. It is checked before the prefix.

### The prefix stays Ctrl-b

Seer keeps Ctrl-b as its prefix. The reason for this choice is not
recorded in the issue or the pull request.

### When herdr exits

The next People message carries the new foreground name, at most one
second later. herdr_in_front turns false and Seer handles the prefix
keys again. Issue 246 records that the one second window is accepted.

## Consequences

- A person who does not use herdr sees no change. Seer tabs and panes
  work as before.
- A person who runs herdr inside a Seer pane keeps the herdr key map.
  Only the people drawer key and the literal prefix stay with Seer.
- The foreground fact comes from the pane that the size owner client
  focuses. A second client of the same person follows that pane.
  Issue 246 accepts this limit.
- A person whose row is missing from the People message keeps normal
  Seer key handling.
- The pass through matches on the exact name "herdr". Another
  multiplexer in a pane gets no pass through.
- On a system that is not Linux the foreground name is empty, so the
  pass through never starts.
- Issue 240, a herdr compatible control surface for skills and
  plugins, is deferred. It is wave 3 work and is not part of this
  decision.
