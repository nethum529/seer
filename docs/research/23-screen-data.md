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
  its style fields differ (crates/seer-core/src/cells.rs:11). The cursor
  is not a cell.
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
  the same on every machine. The driver copies input/long.txt to the temp
  HOME and writes 1200 copies of it to ~/burst.txt for the burst.
- Summary: scripts/perf/summarize_screen.sh. Per update values pool the
  three repeats. Rates divide the total bytes and updates by the total
  window time. The p95 uses the nearest rank method. The after_keys
  columns count only the updates after the last key of a repeat, over the
  time from that key to the last update. They mean something only for the
  burst, where the output runs after the last key. For the other three
  workloads the part after the last key is the answer to one key: a few
  updates in a few milliseconds, so those columns are noise there and this
  document does not use them.
- Link: the guest is a TCP client of the broker, like the broker tests.
  The bytes counted are the codec frames. A guest on an iroh link gets the
  same frames; QUIC and relay overhead is not counted.
- All 24 runs of one driver call share one shell. The screen keeps what
  the earlier runs left. Repeat 1 of the typing workload at 80x24 starts on
  a nearly blank screen (median 61677 bytes per update); repeats 2 and 3
  start on a full screen (95841 bytes), which is the normal case in a
  session. Repeat 1 of the typing workload at 200x50 starts with the source
  text that the 80x24 burst left on the screen.

### Text content

The editor and pager open input/long.txt, and the burst prints 1200 copies
of it. It is the repository's own Rust source: the files
crates/seer-runtime/src/*.rs at commit da51f6b, joined in name order, 2676
lines, 108 of them longer than 80 columns, 85957 bytes. It is committed as
the input, so the run does not depend on the checkout.

The content sets the numbers. Source code has indented lines, blank lines,
and short lines, so run-length encoding shrinks a row that has runs of
spaces, and lines that scroll past differ in most cells. Two earlier
versions of this measurement used synthetic content, and both gave savings
that were too high:

- Commit 32bd94b (replaced) used a file whose 5000 lines differed only in
  digits. With that file the pager update was 285771 bytes today and a
  plain cell diff was 8463 bytes, because the scrolled lines matched cell
  for cell. With real text the same update is 113448 bytes today and a
  plain cell diff is larger than the frame.
- Commit 0517940 (replaced) used seq 1 5000000 as the burst. Digits line
  up column by column, so only 99 of 1920 cells changed per update at
  80x24 (245 of 10000 at 200x50), the frame was 26878 bytes (58802), and a
  cell diff was 16196 bytes (39838), 40 percent below today. With real
  text 793 cells change (1741), the frame is 106795 bytes (194684), and a
  cell diff is as large as the frame. The seq burst also ran at 140
  updates per second at 80x24 (132 at 200x50), against 71 (66) with real
  text, because it moved fewer bytes per second through the terminal
  (38.9 MB in 1.8 seconds, against 103 MB in 1.95 seconds).

A reader who wants numbers for another kind of content (prose, logs, wide
tables) must rerun with that content as input/long.txt.

The editor runs vim -N -u NONE, so it has no syntax highlighting, no
ruler, no line numbers, and no scrolloff. Cell equality includes style,
so a vim with syntax highlighting changes more cells per key, and a ruler
changes cells on every cursor move. The 2 cell median for the editor is a
lower bound.

### Workloads

| Workload | Program | Input file | Keys | Rate | Duration |
| --- | --- | --- | --- | --- | --- |
| typing | bash prompt, 12 short commands | input/typing.txt | 308 | 10 keys/s (100 ms) | 30.7 s |
| editor | vim -N -u NONE on long.txt: move, insert, undo, delete, quit | input/editor.txt | 233 | 10 keys/s (100 ms) | 25.2 s |
| pager | less on long.txt: 600 j keys, then q | input/pager.txt | 617 | 25 keys/s (40 ms), the X11 key repeat rate | 25.7 s |
| burst | bash prompt: cat ~/burst.txt, 1200 copies of long.txt (103 MB, 3.2 million lines) | input/burst.txt | 16 | 10 keys/s, then the output runs | 1.5 s of keys, then 1.9 to 2.0 s of output |

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
  the cells. A row that the shift exposes is compared to blank cells (a
  space with default colors and no style), because a terminal scroll fills
  the exposed rows with blanks.

Every estimate is capped at the bytes of today's frame for the same
update. A real design sends the full frame when the diff is larger, so an
estimate above the frame would count bytes that no design sends. The cap
applies to the pager cell diff in 61 percent of the updates at 80x24 and
97 percent at 200x50, and to the burst cell diff in 81 and 90 percent. It
never applies to the scroll diff.

Assumptions for all three: the guest already holds the previous frame; an
update with zero changed cells still sends the header (a cursor move); the
first update after Watch is a full frame and is not counted. The header is
198 bytes, and one changed cell adds about 160 bytes: the Cell field names
(crates/seer-core/src/cells.rs:11) and the row and column. Every
change-only design that keeps the Cell encoding pays this per cell.

### Environment

Machine, kernel, and build are the issue 389 baseline (AMD Ryzen 7 7800X3D,
16 CPUs, Linux 7.1.6, rustc 1.98.0, release profile). bash 5.3.15, vim 9.2,
less 704. Code revision f06bc13. Run 1 ran on a clean tree. Run 2 is
marked dirty in its environment.txt because the sample files of run 1
(the six files under docs/research/perf-samples/417/) and a draft of this
document were uncommitted when it started. Both runs held
/tmp/claude-1000/perf-run.lock. Both errors.log files are empty. probe.log
holds the final screen of every repeat, so a reader can check that each
script ended at a shell prompt.

Run 2 was run twice. In the first attempt, repeats 2 and 3 of the burst at
200x50 took 6.4 and 8.0 seconds instead of 3.5, because another process
used the CPU at that time, so the whole run was repeated 11 minutes later
and the samples are from the second attempt. The driver of revision
f06bc13 appended to probe.log instead of truncating it, so the lines of
earlier attempts were removed from both probe.log files by hand, and the
driver now truncates the file at the start of a run.

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
| typing | 10.0 | 92818 | 106323 | 109344 | 834362 | 1 | 1 |
| editor | 9.3 | 183178 | 226528 | 235906 | 1564245 | 2 | 1342 |
| pager | 24.1 | 113448 | 157622 | 223833 | 2764637 | 731 | 1108 |
| burst | 44.7 | 106795 | 146013 | 181111 | 4839426 | 793 | 988 |

### 200x50

| Workload | Updates/s | Bytes median | Bytes p95 | Bytes max | Bytes/s | Cells median | Cells p95 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| typing | 10.0 | 166692 | 180683 | 185295 | 1666969 | 1 | 1 |
| editor | 9.3 | 324572 | 327905 | 329505 | 2740219 | 2 | 1732 |
| pager | 24.1 | 202799 | 251753 | 325974 | 4933325 | 1602 | 1962 |
| burst | 41.7 | 194684 | 256053 | 312333 | 8283446 | 1741 | 2143 |

The burst rows above cover the whole window, which includes 1.5 seconds of
typing the command. The next section has the output part alone.

### Burst and the poll ceiling

The runtime polls each pane in a loop (poll_driver,
crates/seer-runtime/src/server.rs:120). One cycle is the work of
poll_and_broadcast (server.rs:425) plus a 5 ms wait (POLL_INTERVAL,
server.rs:25). The work in a cycle is: drain the PTY output, feed it to
the VT parser (crates/seer-runtime/src/pane_host.rs:24), build the frame,
encode it, and send it to every connection. The wait starts after the
work ends. So the ceiling is one update per cycle, and the cycle is 5 ms
plus the work, not 5 ms.

Output part only, from the last key to the last update
(after_keys columns in summary.csv):

| Size | Run | Updates/s | Bytes/s | Gap median | Gap p95 |
| --- | --- | --- | --- | --- | --- |
| 80x24 | 1 | 71.4 | 7859239 | 14 ms | 29 ms |
| 80x24 | 2 | 76.7 | 8442816 | | |
| 200x50 | 1 | 65.6 | 13211883 | 14 ms | 32 ms |
| 200x50 | 2 | 68.1 | 13642164 | | |

The burst runs at the ceiling: every cycle finds new output and sends one
update, so the rate is the cycle rate. The median gap between updates is
14 ms, so the work in a cycle is about 9 ms. That 9 ms is inferred from
the gap and the 5 ms wait; the probe does not measure the runtime. The
work is large because about 53 MB of text per second pass through the
terminal during the burst, so each cycle drains and parses about 700 KB.
With the seq burst of commit 0517940 (22 MB per second) the gap was 7 ms
and the rate 140 per second at 80x24 (132 at 200x50). So the rate depends on the output volume
and on the frame cost, and the split between the two is not measured. A
cheaper frame after R-411 makes the cycle shorter and the rate higher,
but this document cannot say by how much. The change-only bytes per
second for the burst below use today's rate, so they may be low.

The burst finished 103 MB of shell output in 1.9 to 2.0 seconds, so a fast
burst is short, and the guest receives about 135 updates for it. Each
update replaces the whole screen: 793 of 1920 cells change at 80x24, and
the rest are mostly cells that were blank before and after. The scroll
search finds a shift for 90 percent of the burst updates, most often 23
rows up or down (49 at 200x50). That shift is not a real scroll match. It
keeps one row and compares the other 23 rows to blanks, so the scroll
diff for a burst is a list of the non-blank cells of the new screen: 488
cells at 80x24 and 1066
at 200x50. Any change-only design that skips blank cells gets the same
saving, and no design gets more from a burst without a smaller cell
encoding.

### Change-only estimates next to today's bytes

Medians per update. Run 1. Scrolled is the share of updates for which a
shift beat the unshifted cell diff. A cell diff equal to today is the cap.

| Size | Workload | Today | Row diff | Cell diff | Scroll diff | Scrolled |
| --- | --- | --- | --- | --- | --- | --- |
| 80x24 | typing | 92818 | 2757 | 362 | 362 | 3 percent |
| 80x24 | editor | 183178 | 6578 | 524 | 524 | 27 percent |
| 80x24 | pager | 113448 | 113178 | 111491 | 2977 | 98 percent |
| 80x24 | burst | 106795 | 106514 | 106316 | 79488 | 90 percent |
| 200x50 | typing | 166692 | 2917 | 362 | 362 | 4 percent |
| 200x50 | editor | 324572 | 6256 | 525 | 525 | 11 percent |
| 200x50 | pager | 202799 | 202599 | 202799 | 3301 | 98 percent |
| 200x50 | burst | 194684 | 194431 | 194684 | 173608 | 90 percent |

Bytes per second over the same window. Run 1.

| Size | Workload | Today | Row diff | Cell diff | Scroll diff |
| --- | --- | --- | --- | --- | --- |
| 80x24 | typing | 834362 | 65335 | 30501 | 4512 |
| 80x24 | editor | 1564245 | 413310 | 357580 | 42166 |
| 80x24 | pager | 2764637 | 2696441 | 2640761 | 105402 |
| 80x24 | burst | 4839426 | 4447759 | 4416296 | 3305168 |
| 200x50 | typing | 1666969 | 98835 | 69480 | 4518 |
| 200x50 | editor | 2740219 | 338555 | 294601 | 65713 |
| 200x50 | pager | 4933325 | 4818581 | 4827205 | 116577 |
| 200x50 | burst | 8283446 | 7538336 | 7539804 | 6701929 |

The median of the typing and editor updates is one or two cells, but the
bytes per second are set by the few updates that scroll the screen: a
prompt that moves the screen up one line, or a vim jump that redraws it.
That is why the scroll diff saves more per second than the cell diff even
where the medians are equal.

### Repeatability

Run 2, docs/research/perf-samples/417/second/summary.csv, same command
21 minutes later. The values compared are today's bytes per update
median, the changed cells median, today's bytes per second, the scroll
diff median, and the three estimate rates in bytes per second.

- Today's bytes per update median: the same or within 1 percent in every
  row.
- Changed cells median: the same or within 1 cell outside the burst
  (1601 against 1602 for the pager at 200x50); 789 against 793 for the
  burst at 80x24 and 1744 against 1741 at 200x50.
- Today's bytes per second: within 1 percent in every row except the
  burst, 8.4 percent at 80x24 (5243658 against 4839426) and 2.7 percent
  at 200x50. Repeat 1 of the 80x24 burst in run 2 gave 184 updates in a
  2.2 second output part, against 138 to 144 updates in 1.9 to 2.0
  seconds for the other five repeats.
- Scroll diff median: the same or within 1 percent in every row.
- Estimate rates: within 3 percent in every row except the burst, where
  the row diff, cell diff, and scroll diff rates are 9.3, 9.4, and 8.9
  percent higher at 80x24 and 2.9, 2.9, and 4.1 percent higher at 200x50,
  for the same reason as today's bytes per second.

The 20 percent bound of the ticket holds for every median.

## Findings

- One key gives one update. The typing, editor, and pager rates are the
  key rates: 10, 9.3, and 24 updates per second. The runtime does not merge
  or split updates at these rates.
- A screen of source code costs 38 to 62 percent of the worst case. The
  editor update at 80x24 is 183 KB and the pager update 113 KB, against the
  297822 byte synthetic worst case from doc 22 (297841 on main today,
  because the frame gained a field after doc 22). Run-length encoding
  helps on indented and blank lines. The typing update grows from 62 KB on
  a half blank screen to 96 KB on a full one.
- Almost nothing changes per update while typing and editing. The median
  is 1 cell for typing and 2 for the editor. Of the typing updates, 312 of
  1849 change zero cells (a cursor move only).
- Scrolling changes most cells but is one row move. A pager step at 80x24
  changes 731 cells of 1920, so a plain cell diff is larger than today's
  frame and is capped at it, and a row diff saves nothing. After the one
  row shift only 17 cells differ (the new line that scrolls in and the ':'
  of the less status row), and the scroll diff is 2977 bytes, 3 percent
  of today. The same
  holds at 200x50: 1602 cells, cell diff capped at today, scroll diff 3301
  bytes, 2 percent of today.
- A fast output burst saves almost nothing with a cell diff: for the
  output part alone, 0.7 percent of the bytes at 80x24 and 0 at 200x50.
  With real text 793 of 1920 cells change per update at 80x24, and the
  cell diff is as large as the frame. A scroll diff, which here means a
  list of the non-blank cells, saves 26 percent of the output part at
  80x24 and 11 percent at 200x50 (run 2: 26 and 10 percent). The whole
  window saving is higher (32 and 19 percent) because it includes the
  1.5 seconds of typing the command, where each update is a few cells.
- A cell costs about 160 bytes today. Every change-only design that keeps
  the JSON Cell encoding pays this per changed cell. It is why the burst
  saves so little: 488 non-blank cells at 80x24 cost 79 KB, against 107 KB
  for the whole frame with run-length encoding.
- The burst runs at the real poll ceiling: one update per cycle, where a
  cycle is the 5 ms wait plus the work, and the work grows with the output
  volume. The measured rate is 71 updates per second at 80x24 and 66 at
  200x50 with 53 MB of text per second, and was 140 at 80x24 (132 at
  200x50) with 22 MB per second.

## Checks for the ticket

- No shipped code changed. git diff origin/main...HEAD touches only an
  example, two scripts, the input files, the samples, and this document,
  so a build with no measurement option encodes the same bytes as a build
  from origin/main. As a check, cargo run -p seer-core --example
  frame_sizes prints the same lengths on this branch and on a scratch
  checkout of origin/main: 4297, 151069, 297841 for 80x24, 8793, 779543,
  1550293 for 200x50, and 115 for a key. It prints lengths, not bytes, so
  the diff is the proof and the lengths are the check. These lengths are
  19 bytes above the doc 22 numbers because the frame gained a field on
  main after doc 22 and before this branch. A guest built from origin/main
  watches a host built from this branch with no change.
- Test suite: cargo test --workspace on this branch gives 118 passed and 8
  failed. The 8 failures are the 7 tests of crates/seer/tests/start.rs
  and the 1 test of crates/seer/tests/restore.rs. One start.rs test and
  the restore.rs test fail with AddrInUse on port 7321, and the other six
  start.rs tests fail because that panic poisoned the lock they share. A
  live seer-broker holds port 7321 on this machine. The error text points
  to the port; the cause is not verified beyond that.

## Not measured and open items

- A guest on an iroh link. The bytes are the same frames; the transport
  overhead is not counted.
- A person typing. The rates are scripted: 10 keys per second is fast
  typing, 25 keys per second is the key repeat rate.
- Other text content. Prose, logs, and wide tables give other numbers.
  The synthetic results above show the range.
- An editor with syntax highlighting, a ruler, or line numbers.
- More than one watched pane, or more than one guest. The bytes scale
  with both.
- The burst rate after R-411, when a frame is cheaper to build, and the
  split of the cycle work between parsing the output and building the
  frame.

## Recommendation for R-411

Do R-411, and size it as a scroll diff, not a plain cell diff.

Reason: today every update sends the whole screen, 93 to 183 KB at 80x24
and 167 to 325 KB at 200x50, while the median update changes 1 or 2 cells
when a person types or edits. A plain cell diff saves nothing for
scrolling, the most common heavy case: a pager step changes 731 of 1920
cells at 80x24, and the diff is as large as the frame. A change-only
update that can also say that the rows moved by N turns that step into 17
cells and 2977 bytes. A fast output burst saves little with any design
that keeps today's cell encoding: a cell diff saves 0.7 percent of the
output bytes at 80x24 and 0 at 200x50, and a scroll diff saves 26 and 11
percent.

Expected saving in bytes per second at today's update rate, from the run 1
tables above (today, then scroll diff):

| Workload | 80x24 | 200x50 |
| --- | --- | --- |
| typing | 834362 to 4512 (99 percent) | 1666969 to 4518 (99 percent) |
| editor | 1564245 to 42166 (97 percent) | 2740219 to 65713 (98 percent) |
| pager | 2764637 to 105402 (96 percent) | 4933325 to 116577 (98 percent) |
| burst | 4839426 to 3305168 (32 percent) | 8283446 to 6701929 (19 percent) |

The burst row includes 1.5 seconds of typing the command. The output part
alone saves 26 percent at 80x24 and 11 percent at 200x50. For a burst,
the scroll diff is a list of the non-blank cells.

Two costs stay. First, about 160 bytes of JSON per changed cell: the Cell
field names (crates/seer-core/src/cells.rs:11) plus the row and column.
Every change-only design that keeps the Cell encoding pays it, and it sets
the floor for bursts. A smaller cell encoding is a separate decision and
is out of scope here. Second, the burst runs at the poll ceiling, so a
cheaper frame can raise the update rate, and the burst saving in bytes per
second may be smaller than the table says.

The change must follow the rule of doc 24 (issue 418): a changes-only
update is a new message kind, it ships in a new minor version, never a
patch release, and on the local link the runtime sends it only to a
window that said it can read it, through a capability field with
serde(default) in a message the client already sends at attach. The new
client must still decode the old full frame, because it can attach to a
runtime that started before seer update. Do not raise
TERMINAL_PROTOCOL_VERSION: an old runtime then refuses every key of a new
client.
