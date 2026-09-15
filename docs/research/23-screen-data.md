# Screen data that a guest receives

Date: 2026-09-15
Issue: 417 (T-417). Related: 411 (R-411), 418 (T-418), 393.

## Scope

This document measures the screen updates that a guest receives when it
watches a host terminal in a normal session. Research doc 22 (issue 393)
has only sizes for synthetic screens. This document has the real update
rate, the bytes per update, and the cells that change per update, for four
scripted workloads at 80x24 and at 200x50. It ends with an estimate of the
bytes that a change-only update would need, and with a recommendation for
R-411.

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
  two change-only estimates. The run ends 2 seconds after the last update
  that follows the last key.
- Driver: scripts/perf/screen.sh. It builds the release broker, runtime,
  and probe, starts a broker on port 47417 and a runtime for alice in a
  temp dir, and runs every workload at 80x24 and then at 200x50, three
  repeats each. The shell is bash with a temp HOME whose .bashrc sets
  PS1='$ ', clears PROMPT_COMMAND, and unsets HISTFILE, so the prompt is
  the same on every machine. The long file is 5000 lines of
  "line N: the quick brown fox jumps over the lazy dog M times", made by
  the driver in the temp HOME.
- Summary: scripts/perf/summarize_screen.sh. Per update values pool the
  three repeats. Rates divide the total bytes and updates by the total
  window time. The p95 uses the nearest rank method.
- Link: the guest is a TCP client of the broker, like the broker tests.
  The bytes counted are the codec frames. A guest on an iroh link gets the
  same frames; QUIC and relay overhead is not counted.
- Between the repeats the shell keeps its scrollback. Repeat 1 of the
  typing workload starts on a nearly blank screen. Repeats 2 and 3 start on
  a full screen, which is the normal case in a session.

### Workloads

| Workload | Program | Input file | Keys | Rate | Duration |
| --- | --- | --- | --- | --- | --- |
| typing | bash prompt, 12 short commands | input/typing.txt | 308 | 10 keys/s (100 ms) | 30.7 s |
| editor | vim -N -u NONE on the long file: move, insert, undo, delete, quit | input/editor.txt | 233 | 10 keys/s (100 ms) | 25.2 s |
| pager | less on the long file: 600 j keys, then q | input/pager.txt | 617 | 25 keys/s (40 ms), the X11 key repeat rate | 25.7 s |
| burst | bash prompt: seq 1 5000000 (38 MB of output) | input/burst.txt | 14 | 10 keys/s, then the output runs | 1.3 s of keys, then 1.7 to 1.9 s of output |

Input files are under docs/research/perf-samples/417/input/. In a script a
newline is Enter, \e is Escape, \t is Tab, \\ is a backslash, and \w is a
one second pause. The duration is the window of one repeat.

### Change-only estimates

Both estimates use the same JSON codec and the same header (user, pane,
cursor, modes) as today's Cells message, so they are comparable with the
bytes measured today. Neither changes the cell encoding, which is R-411's
decision.

- Row diff: the header plus only the rows that hold a changed cell, each
  row run-length encoded like today, plus a JSON list of the row indexes.
- Cell diff: the header plus a JSON list of (row, column, cell) for each
  changed cell.

Assumptions: the guest already holds the previous frame; an update with
zero changed cells still sends the header (a cursor move); the first update
after Watch is a full frame and is not counted.

### Environment

Machine, kernel, and build are the issue 389 baseline (AMD Ryzen 7 7800X3D,
16 CPUs, Linux 7.1.6, rustc 1.98.0, release profile). bash 5.3.15, vim 9.2,
less 704. Revision f893482. Both runs held /tmp/claude-1000/perf-run.lock.
Both errors.log files are empty. probe.log holds the final screen of every
repeat, so a reader can check that each script ended at a shell prompt.

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
| typing | 10.0 | 92818 | 106323 | 109344 | 834453 | 1 | 1 |
| editor | 9.3 | 286885 | 290862 | 297844 | 2470965 | 2 | 163 |
| pager | 24.2 | 285771 | 285771 | 290827 | 6780113 | 51 | 64 |
| burst | 84.5 | 26878 | 30694 | 91877 | 2356216 | 115 | 144 |

### 200x50

| Workload | Updates/s | Bytes median | Bytes p95 | Bytes max | Bytes/s | Cells median | Cells p95 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| typing | 10.0 | 149676 | 180683 | 185295 | 1309396 | 1 | 1 |
| editor | 9.4 | 484514 | 500743 | 501221 | 4216397 | 2 | 147 |
| pager | 24.0 | 488975 | 490565 | 490724 | 11536551 | 114 | 122 |
| burst | 78.7 | 57609 | 63651 | 177344 | 4617731 | 245 | 301 |

The burst rows above cover the whole window, which includes 1.3 seconds of
typing the command. The next section has the output part alone.

### Burst and the poll ceiling

The runtime checks each pane for changes every 5 ms while a client is
attached (POLL_INTERVAL, crates/seer-runtime/src/server.rs:25). The wait
starts after the previous check ends, so the ceiling is one update per
5 ms plus the time to build and send the frame, at most 200 updates per
second per pane.

Output part only, from the last key to the last update
(after_keys columns in summary.csv):

| Size | Run | Updates/s | Bytes/s | Gap median | Gap p95 | Gaps of 1 ms or less |
| --- | --- | --- | --- | --- | --- | --- |
| 80x24 | 1 | 138.3 | 3750801 | 7 ms | 9 ms | 11 percent |
| 80x24 | 2 | 140.0 | 3811037 | | | |
| 200x50 | 1 | 129.6 | 7411797 | 7 ms | 16 ms | 17 percent |
| 200x50 | 2 | 133.8 | 7706920 | | | |

The update rate does not reach the ceiling. The median gap between updates
is 7 ms, that is the 5 ms wait plus about 2 ms of work per update. Some
updates arrive in pairs less than 1 ms apart; the probe records them as
they arrive. The burst finished 38 MB of shell output in 1.7 to 1.9
seconds, so a fast burst is short, and the guest receives about 250
updates for it.

### Change-only estimate next to today's bytes

Medians per update, and bytes per second over the same window. Run 1.

| Size | Workload | Today median | Row diff median | Cell diff median | Today bytes/s | Row diff bytes/s | Cell diff bytes/s |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 80x24 | typing | 92818 | 2757 | 362 | 834453 | 65360 | 33480 |
| 80x24 | editor | 286885 | 8962 | 524 | 2470965 | 721722 | 61619 |
| 80x24 | pager | 285771 | 285501 | 8463 | 6780113 | 6675298 | 227350 |
| 80x24 | burst | 26878 | 26941 | 18778 | 2356216 | 2183199 | 1466019 |
| 200x50 | typing | 149676 | 2759 | 362 | 1309396 | 82494 | 62697 |
| 200x50 | editor | 484514 | 6738 | 525 | 4216397 | 581354 | 70146 |
| 200x50 | pager | 488975 | 488782 | 18703 | 11536551 | 11392502 | 471015 |
| 200x50 | burst | 57609 | 56001 | 39838 | 4617731 | 4268600 | 2954828 |

### Repeatability

Run 2, docs/research/perf-samples/417/second/summary.csv, same command
ten minutes later. Every bytes median and cells median is the same as run
1, except the burst at 200x50: 59517 bytes against 57609 (3.3 percent) and
244 cells against 245. Bytes per second differ by less than 5 percent in
every row. The 20 percent bound of the ticket holds.

## Findings

- One key gives one update. The typing, editor, and pager rates are the
  key rates: 10, 9.3, and 24 updates per second. The runtime does not merge
  or split updates at these rates.
- A full screen of text costs the worst case. The editor and pager updates
  at 80x24 are 286 KB, 96 percent of the 297841 byte synthetic worst case
  from doc 22. Run-length encoding only helps on blank rows: the typing
  update grows from 62 KB on a half blank screen (repeat 1) to 96 KB on a
  full one (repeats 2 and 3).
- Almost nothing changes per update. The median is 1 cell for typing, 2
  for the editor, 51 for the pager, and 115 for the burst. Of the typing
  updates, 156 of 924 change zero cells (a cursor move only).
- Scrolling changes every row but few cells. A pager step moves 23 rows
  but only 51 cells differ, because the lines of the file look alike. A row
  diff saves nothing there (285501 against 285771 bytes) and a cell diff
  saves 97 percent. The burst is the same shape: 24 changed rows, 115 cells.
- A cell costs about 160 bytes today. The cell diff header is 198 bytes
  and one changed cell adds about 160 bytes of JSON, so the cell diff for
  the burst is still 18 KB per update and only 38 percent below today. The
  cell encoding, not the frame shape, sets the floor for output bursts.
- The burst does not reach the poll ceiling. 130 to 140 updates per second
  against a ceiling of 200, with a 7 ms median gap.

## Checks for the ticket

- No shipped code changed. The branch adds an example, two scripts, the
  input files, and the samples. cargo run -p seer-core --example
  frame_sizes prints the same bytes on this branch and on origin/main
  (4297, 151069, 297841 for 80x24; 8793, 779543, 1550293 for 200x50). These
  are 19 bytes above the doc 22 numbers because the frame gained a field on
  main after doc 22, before this branch.
- The measurement did not add an option to the binaries, so a guest built
  from origin/main watches a host built from this branch with no change.

## Not measured and open items

- A guest on an iroh link. The bytes are the same frames; the transport
  overhead is not counted.
- A person typing. The rates are scripted: 10 keys per second is fast
  typing, 25 keys per second is the key repeat rate.
- More than one watched pane, or more than one guest. The bytes scale
  with both.
- The pairs of updates less than 1 ms apart during a burst. The probe
  records them; this document does not explain them.
