# Screen data that a guest receives

Date: 2026-09-15
Issue: 417 (T-417). Related: 411 (R-411), 418 (T-418), 393.

## Scope

This document measures the screen updates that a guest receives when it
watches a host terminal in a normal session. Research doc 22 (issue 393)
has only sizes for synthetic screens. This document has the real update
rate, the bytes per update, and the cells that change per update, for four
scripted workloads at 80x24 and at 200x50. It ends with three estimates of
the bytes that a change-only update would need, and with a recommendation
for R-411 in the last section.

Not in this document: a change to the update format or the encoding, the
poll interval, update merging, version skew (doc 24, issue 418), the host's
own local link, and the stream adapter (doc 22).

## Terms

- Update: one Cells message for one pane that arrives at the guest client.
- Changed cells: the cells that differ from the previous update for the
  same pane on the same link. A cell differs when its character or any of
  its style fields differ. The cursor is not a cell.
- Window: the time from the first key of a run to the last update of the
  run. Rates divide by the window.

## Method

- Probe: crates/seer-core/examples/screen_probe. One process plays the
  host window and the guest. It attaches to the host runtime over the local
  Unix socket, sends Resize to the wanted size, joins the room over TCP as
  bob, sends Watch for the host's first pane at the same size, and waits
  1.5 seconds for the screen to settle. Then a thread types a script into
  the host window at a fixed rate, one TerminalInput message per key. The
  main thread reads raw frames from the guest socket and records, for each
  Cells message of the watched pane, the frame bytes (4 byte length prefix
  plus JSON body), the changed cells against the previous update, and the
  three change-only estimates. The run ends 2 seconds after the last update
  that follows the last key.
- Driver: scripts/perf/screen.sh. It builds the release broker, runtime,
  and probe, starts a broker on port 47417 and a runtime for alice in a
  temp dir, and runs every workload at 80x24 and then at 200x50, three
  repeats each. The shell is bash with a temp HOME whose .bashrc sets
  PS1='$ ', clears PROMPT_COMMAND, and unsets HISTFILE, so the prompt is
  the same on every machine.
- Summary: scripts/perf/summarize_screen.sh. Per update values pool the
  three repeats. Rates divide the total bytes and updates by the total
  window time. The p95 uses the nearest rank method.
- Link: the guest is a TCP client of the broker, like the broker tests.
  The bytes counted are the codec frames. A guest on an iroh link gets the
  same frames; QUIC and relay overhead is not counted.
- All 24 runs of one driver call share one shell. The screen keeps what
  the earlier runs left. Repeat 1 of the typing workload at 80x24 starts on
  a nearly blank screen (median 61677 bytes per update); repeats 2 and 3
  start on a full screen (95841 bytes), which is the normal case in a
  session. Repeat 1 of the typing workload at 200x50 starts with the
  numbers that the 80x24 burst left on the screen.

### Text content

The editor and pager open input/long.txt. It is the repository's own Rust
source: the files crates/seer-runtime/src/*.rs at commit da51f6b, joined in
name order, 2676 lines, 108 of them longer than 80 columns. It is committed
as the input, so the run does not depend on the checkout.

The content sets the numbers. Source code has indented lines, blank lines,
and short lines, so run-length encoding shrinks a row that has runs of
spaces, and lines that scroll past differ in most cells. The first version
of this measurement (commit 32bd94b, replaced) used a synthetic file whose
5000 lines differed only in digits. With that file the pager update was
285771 bytes today and a plain cell diff was 8463 bytes, because the
scrolled lines matched cell for cell. With real text the same update is
113448 bytes today and a plain cell diff is 118788 bytes. A reader who
wants numbers for another kind of content (prose, logs, wide tables) must
rerun with that content as input/long.txt.

### Workloads

| Workload | Program | Input file | Keys | Rate | Duration |
| --- | --- | --- | --- | --- | --- |
| typing | bash prompt, 12 short commands | input/typing.txt | 308 | 10 keys/s (100 ms) | 30.7 s |
| editor | vim -N -u NONE on long.txt: move, insert, undo, delete, quit | input/editor.txt | 233 | 10 keys/s (100 ms) | 25.2 s |
| pager | less on long.txt: 600 j keys, then q | input/pager.txt | 617 | 25 keys/s (40 ms), the X11 key repeat rate | 25.7 s |
| burst | bash prompt: seq 1 5000000 (38 MB of output) | input/burst.txt | 14 | 10 keys/s, then the output runs | 1.3 s of keys, then 1.7 to 1.9 s of output |

Input files are under docs/research/perf-samples/417/input/. In a script a
newline is Enter, \e is Escape, \t is Tab, \\ is a backslash, and \w is a
one second pause. The duration is the window of one repeat.

### Change-only estimates

All three estimates use the same JSON codec, the same Cell encoding, and
the same header (user, pane, cursor, modes) as today's Cells message, so
they are comparable with the bytes measured today. None of them designs a
message. They only size one.

- Row diff: the header plus only the rows that hold a changed cell, each
  row run-length encoded like today, plus a JSON list of the row indexes.
- Cell diff: the header plus a JSON list of (row, column, cell) for each
  changed cell.
- Scroll diff: the probe moves the previous frame up or down by every
  number of rows from 1 to rows minus 1 and keeps the shift that gives the
  fewest changed cells. If no shift beats the unshifted count, the scroll
  diff is the cell diff. Otherwise it is the header plus a JSON field
  "moved" with the shift plus the cell diff after the shift. This assumes
  that the receiver can move the rows of its own copy before it applies
  the cells. It counts the rows that scroll in as changed cells.

Assumptions for all three: the guest already holds the previous frame; an
update with zero changed cells still sends the header (a cursor move); the
first update after Watch is a full frame and is not counted. The header is
198 bytes, and one changed cell adds about 160 bytes: the Cell field names
(crates/seer-core/src/cells.rs:11) and the row and column. Every
change-only design that keeps the Cell encoding pays this per cell.

### Environment

Machine, kernel, and build are the issue 389 baseline (AMD Ryzen 7 7800X3D,
16 CPUs, Linux 7.1.6, rustc 1.98.0, release profile). bash 5.3.15, vim 9.2,
less 704. Code revision c9b8dc1. environment.txt names fc1a1cd and marks it
dirty: the probe and driver changes of c9b8dc1 were in the tree but not yet
committed when the driver ran. Both runs held
/tmp/claude-1000/perf-run.lock. Both errors.log files are empty. probe.log
holds the final screen of every repeat, so a reader can check that each
script ended at a shell prompt.

Commands:

    flock /tmp/claude-1000/perf-run.lock scripts/perf/screen.sh 3 docs/research/perf-samples/417
    flock /tmp/claude-1000/perf-run.lock scripts/perf/screen.sh 3 docs/research/perf-samples/417/second

One workload alone, for example the pager:

    flock /tmp/claude-1000/perf-run.lock scripts/perf/screen.sh 3 target/perf/417 pager

## Results

Run 1, docs/research/perf-samples/417/summary.csv. Bytes are per update as
they arrive at the guest socket. Cells are changed cells per update.

### 80x24

| Workload | Updates/s | Bytes median | Bytes p95 | Bytes max | Bytes/s | Cells median | Cells p95 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| typing | 10.0 | 92818 | 106323 | 109344 | 835096 | 1 | 1 |
| editor | 9.4 | 183178 | 226528 | 235906 | 1570012 | 2 | 1342 |
| pager | 24.1 | 113448 | 157622 | 223833 | 2767054 | 731 | 1108 |
| burst | 85.2 | 26878 | 30694 | 91877 | 2379051 | 99 | 138 |

### 200x50

| Workload | Updates/s | Bytes median | Bytes p95 | Bytes max | Bytes/s | Cells median | Cells p95 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| typing | 10.0 | 149517 | 180683 | 185295 | 1308956 | 1 | 1 |
| editor | 9.4 | 324566 | 327905 | 329505 | 2751732 | 2 | 1732 |
| pager | 24.1 | 202799 | 251753 | 325974 | 4932689 | 1602 | 1962 |
| burst | 80.4 | 58802 | 63651 | 177344 | 4731638 | 245 | 301 |

The burst rows above cover the whole window, which includes 1.3 seconds of
typing the command. The next section has the output part alone.

### Burst and the poll ceiling

The runtime checks each pane for changes every 5 ms while a client is
attached (POLL_INTERVAL, crates/seer-runtime/src/server.rs:25). The wait
starts after the previous check ends, so one cycle is 5 ms plus the time
to build and send the frame. The real ceiling is one update per cycle.

Output part only, from the last key to the last update
(after_keys columns in summary.csv):

| Size | Run | Updates/s | Bytes/s | Gap median | Gap p95 |
| --- | --- | --- | --- | --- | --- |
| 80x24 | 1 | 140.4 | 3811604 | 7 ms | 8 ms |
| 80x24 | 2 | 141.0 | 3837569 | | |
| 200x50 | 1 | 132.1 | 7587848 | 7 ms | 20 ms |
| 200x50 | 2 | 130.5 | 7458018 | | |

The burst runs at the ceiling. The median gap is 7 ms, that is the 5 ms
wait plus about 2 ms of work, and every cycle gives an update, so the rate
is about 140 updates per second at 80x24 and 130 at 200x50, not 200. The
work per cycle includes building and encoding the frame. If R-411 makes a
frame cheaper to build, the cycle gets shorter and the rate can rise
toward 200 per second. So the change-only bytes per second for the burst
below, which use today's rate, may be low. The burst finished 38 MB of
shell output in 1.7 to 1.9 seconds, so a fast burst is short, and the
guest receives about 250 updates for it. Each update replaces the whole
screen: the scroll search found no shift for any burst update, because
more lines pass between two checks than the screen holds.

### Change-only estimates next to today's bytes

Medians per update. Run 1. Scrolled is the share of updates for which a
shift beat the unshifted cell diff.

| Size | Workload | Today | Row diff | Cell diff | Scroll diff | Scrolled |
| --- | --- | --- | --- | --- | --- | --- |
| 80x24 | typing | 92818 | 2757 | 362 | 362 | 3 percent |
| 80x24 | editor | 183178 | 6578 | 524 | 524 | 26 percent |
| 80x24 | pager | 113448 | 113178 | 118788 | 15845 | 97 percent |
| 80x24 | burst | 26878 | 26941 | 16196 | 16196 | 0 |
| 200x50 | typing | 149517 | 2917 | 362 | 362 | 3 percent |
| 200x50 | editor | 324566 | 6256 | 525 | 525 | 9 percent |
| 200x50 | pager | 202799 | 202599 | 260895 | 35829 | 97 percent |
| 200x50 | burst | 58802 | 56303 | 39838 | 39838 | 0 |

Bytes per second over the same window. Run 1.

| Size | Workload | Today | Row diff | Cell diff | Scroll diff |
| --- | --- | --- | --- | --- | --- |
| 80x24 | typing | 835096 | 65410 | 33505 | 11790 |
| 80x24 | editor | 1570012 | 412084 | 376406 | 82146 |
| 80x24 | pager | 2767054 | 2695948 | 2843863 | 410099 |
| 80x24 | burst | 2379051 | 2204459 | 1399849 | 1399849 |
| 200x50 | typing | 1308956 | 84720 | 64734 | 29151 |
| 200x50 | editor | 2751732 | 336364 | 319277 | 111827 |
| 200x50 | pager | 4932689 | 4818714 | 6086480 | 885267 |
| 200x50 | burst | 4731638 | 4382183 | 3027086 | 3027086 |

The median of the typing and editor updates is one or two cells, but the
bytes per second are set by the few updates that scroll the screen: a
prompt that moves the screen up one line, or a vim jump that redraws it.
That is why the scroll diff saves more per second than the cell diff even
where the medians are equal.

### Repeatability

Run 2, docs/research/perf-samples/417/second/summary.csv, same command
ten minutes later. Every bytes median is the same as run 1 except the
bursts (27832 against 26878 at 80x24, 3.5 percent; 56655 against 58802 at
200x50, 3.7 percent) and typing at 200x50 (149676 against 149517). The
changed cells medians are the same except the burst at 80x24 (115 against
99, 16 percent). Bytes per second differ by less than 2 percent in every
row. The 20 percent bound of the ticket holds.

## Findings

- One key gives one update. The typing, editor, and pager rates are the
  key rates: 10, 9.4, and 24 updates per second. The runtime does not merge
  or split updates at these rates.
- A screen of source code costs 38 to 62 percent of the worst case. The
  editor update at 80x24 is 183 KB and the pager update 113 KB, against the
  297841 byte synthetic worst case from doc 22. Run-length encoding helps
  on indented and blank lines. The typing update grows from 62 KB on a half
  blank screen to 96 KB on a full one.
- Almost nothing changes per update while typing and editing. The median
  is 1 cell for typing and 2 for the editor. Of the typing updates, 156 of
  924 change zero cells (a cursor move only).
- Scrolling changes most cells but is one row move. A pager step at 80x24
  changes 731 cells of 1920, so a plain cell diff (118788 bytes) is larger
  than today's frame (113448) and a row diff saves nothing. After the one
  row shift only 96 cells differ, and the scroll diff is 15845 bytes, 14
  percent of today. The same holds at 200x50: 1602 cells, cell diff larger
  than today, scroll diff 18 percent of today.
- A cell costs about 160 bytes today. Every change-only design that keeps
  the JSON Cell encoding pays this per changed cell. It is why the cell
  diff for a burst is still 16 KB per update at 80x24 and 40 KB at 200x50,
  only 32 to 40 percent below today, and why a scroll diff still needs 16
  KB for a pager step.
- The burst runs at the real poll ceiling: 130 to 140 updates per second,
  one per 5 ms wait plus 2 ms of work.

## Checks for the ticket

- No shipped code changed. The branch adds an example, two scripts, the
  input files, the samples, and this document. Byte identity was checked
  with cargo run -p seer-core --example frame_sizes on this branch and on
  a scratch checkout of origin/main: both print 4297, 151069, 297841 for
  80x24 and 8793, 779543, 1550293 for 200x50, and 115 for a key. These
  are 19 bytes above the doc 22 numbers because the frame gained a field
  on main after doc 22 and before this branch. The measurement added no
  option to the binaries, so a guest built from origin/main watches a
  host built from this branch with no change.
- Test suite: cargo test --workspace on this branch gives 118 passed and 8
  failed. The 8 failures are the 7 tests of crates/seer/tests/start.rs
  and the 1 test of crates/seer/tests/restore.rs, all with AddrInUse on
  port 7321, which a live seer-broker holds on this machine. They are not
  related to this branch.

## Not measured and open items

- A guest on an iroh link. The bytes are the same frames; the transport
  overhead is not counted.
- A person typing. The rates are scripted: 10 keys per second is fast
  typing, 25 keys per second is the key repeat rate.
- Other text content. Prose, logs, and wide tables give other numbers.
  The synthetic file result above shows the range.
- More than one watched pane, or more than one guest. The bytes scale
  with both.
- The burst rate after R-411, when a frame is cheaper to build.

## Recommendation for R-411

Do R-411, and size it as a scroll diff, not a plain cell diff.

Reason: today every update sends the whole screen, 93 to 183 KB at 80x24
and 150 to 325 KB at 200x50, while the median update changes 1 or 2 cells
when a person types or edits. A plain cell diff is worse than today for
scrolling, the most common heavy case: a pager step changes 731 of 1920
cells at 80x24, and the diff is larger than the frame. A change-only
update that can also say that the rows moved by N turns that step into 96
cells and 15845 bytes.

Expected saving in bytes per second at today's update rate, from the run 1
tables above (today, then scroll diff):

| Workload | 80x24 | 200x50 |
| --- | --- | --- |
| typing | 835096 to 11790 (99 percent) | 1308956 to 29151 (98 percent) |
| editor | 1570012 to 82146 (95 percent) | 2751732 to 111827 (96 percent) |
| pager | 2767054 to 410099 (85 percent) | 4932689 to 885267 (82 percent) |
| burst | 2379051 to 1399849 (41 percent) | 4731638 to 3027086 (36 percent) |

Two costs stay. First, about 160 bytes of JSON per changed cell: the Cell
field names (crates/seer-core/src/cells.rs:11) plus the row and column.
Every change-only design that keeps the Cell encoding pays it, and it sets
the floor for bursts and for scrolling. A smaller cell encoding is a
separate decision and is out of scope here. Second, the burst runs at the
poll ceiling, so a cheaper frame can raise the update rate, and the burst
saving in bytes per second may be smaller than the table says.

The change must follow the rule of doc 24 (issue 418): a changes-only
update is a new message kind, it ships in a new minor version, never a
patch release, and on the local link the runtime sends it only to a
window that said it can read it, through a capability field with
serde(default) in a message the client already sends at attach.
