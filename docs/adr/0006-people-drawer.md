# ADR 0006: The people view is a drawer

Date: 2026-09-04
Status: Accepted

## Context

The multiplayer MVP needs a way to see who else is on the server, what
each person does, and what their screen shows. Cross user viewing is
read only.

Three shapes were drawn as mockups and shown to the owner with the
Lavish tool: ideas A, B, and C. Idea A was a sidebar list. Idea B was
a full screen people view. Idea C was a drawer. The owner picked idea
C on 2026-09-04. The tickets 248 to 252 record the choice. Reasons
beyond the choice itself are not recorded.

## Decision

### The handle and the drawer

The right most column of the client screen is a one column handle with
a vertical marker. The pane area is one column narrower and the
runtime gets the narrower size. A click on the handle, or the prefix
then u, opens the drawer. The same click or the same keys close it.
Esc closes it. The drawer covers the right part of the screen. Its
width is one third of the columns, with a minimum of 40 columns. The
panes keep their size while the drawer is open. See
crates/seer/src/drawer.rs, constants HANDLE_WIDTH and
MINIMUM_DRAWER_WIDTH.

### The list

Each person is one row: a state dot, the name, the foreground process
name, and the time since the last key. Up and Down move the highlight.
The list is replaced when a new People message arrives. Up, Down,
Enter, and Esc go to the drawer. Other keys go to the focused pane.

### The preview

The lower part of the drawer shows a live view of the active tab of
the highlighted person. It is drawn at most once per second. See
crates/seer/src/preview.rs, constant DRAW_INTERVAL. The preview keeps
the top left of the frame and cuts what does not fit. There is no
character scaling.

The preview opens a second connection and uses the existing Peek and
StopPeek messages and the cell stream. No new protocol message was
added. When the highlight moves, the client stops the old peek and
opens a new one. See sync_preview in crates/seer/src/drawer.rs.

Only a person whose runtime is running is peekable. The broker sets
the peekable field. The drawer previews a person only when that field
is true.

### Full peek

Enter on a highlighted peekable person closes the drawer and shows the
peek over the whole screen. It is the same view as seer peek <name>.
Esc stops the peek and returns to the person's own tree with the
drawer closed. The code reuses the seer peek path, set_peek_person in
crates/seer/src/peek_mode.rs. It is not a second implementation.

### Read only

Every cross user view is read only. Keys never reach the peeked pane.
In view only mode the client handles the drawer toggle and ignores
every other action. See is_view_only in crates/seer/src/peek_mode.rs
and handle_key in crates/seer/src/tui.rs.

### The status ticker that feeds the drawer

The broker runs a status ticker thread. Once per second it asks the
runtime of each person with at least one attached client for a status
reply, then builds the People list and broadcasts it. It broadcasts
only when the list is different from the last one it sent. See
STATUS_INTERVAL, spawn_status_ticker, refresh_statuses, and
publish_people in crates/seer-broker/src/server.rs.

The runtime status reply carries the tab count, the foreground process
name of the active tab, and the seconds since the last terminal input.
See crates/seer-runtime/src/server/status.rs. The active tab is the
tab in the viewport of the size owner connection.

The broker adds the number of attached clients, the peekable flag, and
the state. The state rule is in person_state: away when no client is
attached, active when the last key is under 60 seconds old, idle under
10 minutes, away after that.

## Consequences

- The pane area loses one column to the handle on every client.
- A person sees a change in the drawer at most one second late. The
  ticker period sets the limit.
- The preview needs a second connection to the server for each
  highlighted person.
- The foreground name and the active tab follow the size owner client.
  A second client of the same person follows that pane.
- The foreground fact is shared with the herdr pass through in
  ADR 0005. It is built once.
- The sidebar list and the full screen people view (Lavish ideas A and
  B) are not built.
