# Output speed of the own terminal in the desktop window

Date: 2026-10-08
Issue: 462 (T-462). Related: 448 (T-448), 449 (R-449).

## Scope

This document measures how fast the desktop window shows the output of
the person's own terminal, against Alacritty, Ghostty and kitty. It also
records the design that T-462 uses and the Linux PTY quirk that caused
most of the gap to Alacritty.

Not in this document: typing latency (R-449), the terminals of other
people (they stay on cell frames), camera latency, and a change of the
terminal engine.

## Method

- Driver: scripts/perf/terminal.sh. It opens each terminal as a floating
  Hyprland window, at a size that gives the same grid: 40 rows, 148
  columns for Seer and 149 for the others (font size 13, empty configs).
  For Seer it starts a release seer-runtime with its own state directory
  and then seer-desktop.
- Shell: scripts/perf/terminal_io.zsh runs as the shell of the terminal
  under test. For each corpus it runs cat six times and sends DA1
  (CSI c) after each cat. A terminal answers DA1 only after it parses
  every earlier byte, so cat plus DA1 is the time to parse all the
  output. The result is the mean of runs 2 to 6. Then it measures 300
  idle DA1 round trips, 10 ms apart, and reports p50 and p99.
- Corpora: plain.txt is the 11 MB Shakespeare file from
  moktavizen/terminal-benchmark (the script checks its SHA-256).
  sgr.txt is 11 MB of words with 256 color, true color, bold and
  background on every word, made by the script with seed 7.
- Load: the script records the load average before each terminal. Runs
  at a load above 3 are not used. Other agents compile on this machine
  often.
- Machine: AMD Ryzen 7 7800X3D, Linux 7.2.8 (CachyOS), Hyprland.

## Design

- The runtime stays the only part that answers queries (DA1, cursor
  position report). The grid in the window drops its replies. So a
  query gets exactly one answer, and answers still work with no window.
- The PTY reader thread of the runtime parses each read into the pane
  grid, writes the replies at once, and pushes the same bytes to each
  window that follows the pane. There is no poll interval on this path.
- A window that sends raw_output in TerminalCapabilities gets the raw
  PTY bytes of the pane that it watches, not cell frames. Raw frames use
  the top byte of the 4 byte length prefix (0x80 plus a kind), so JSON
  frames do not change. Kinds: Output, Start (size), Size (resize in
  stream order), SyncEnd (a synchronized update ended by time in the
  runtime).
- On attach, the runtime sends Start and then VT bytes that rebuild the
  screen, the scrollback (1000 lines), the modes, the cursor and the
  primary screen under the alternate screen. Then live bytes follow,
  with no gap and no overlap, because both happen under the pane lock.
- Backpressure: before it parses a read, the reader waits until each
  window has fewer than RAW_LIMIT (64 KiB) raw bytes queued
  (crates/seer-runtime/src/outbox.rs). So a window is at most 64 KiB plus
  one PTY read (64 KiB) plus the socket buffer behind the runtime, and a
  fast program cannot run far ahead of the window. A window that takes
  no bytes for 2 seconds is disconnected. The attach restore is sent in
  one burst and is bounded by the scrollback size.
- Queued raw output that the writer thread did not take yet is merged
  into one frame, and frames are encoded at write time. Under load this
  cuts the frame count by about 10 times.
- The window parses raw bytes on its socket reader thread into its own
  grid. Paint copies and shapes only the rows that alacritty_terminal
  marks as damaged.

## Linux PTY quirk

Before the fix, plain cat plus DA1 took 174 ms in Seer and 137 ms in
Alacritty. The cause was not Seer's parse or stream:

- zsh time of cat itself: cat used 170 ms of system CPU under Seer and
  135 ms under Alacritty, with fewer than 12 voluntary waits in both. So
  cat did not wait for the reader. Its own kernel output path (ONLCR on
  the tty) ran slower.
- A small probe (openpty, cat, a blocking read loop that parses each
  read) gave the same 170 ms. The same probe with the master descriptor
  in an epoll set gave 142 ms, the Alacritty number. Alacritty reads the
  PTY from an epoll loop.
- The gain comes only when the epoll set holds the original master
  descriptor from openpty. An epoll set on a dup of the master, or a
  poll(2) wait, gives no gain. With portable-pty: 163 ms without the
  epoll set, 136 ms with it.
- Not the cause: IUTF8, setsid and the controlling tty, closing the PTY
  descriptors in the child, a nonblocking master, a 1 MiB read buffer,
  allocation or a mutex per read, the cgroup, and a second parser thread.

The fix (crates/seer-runtime/src/pty.rs): on Linux the reader thread
waits on an epoll set that holds the master descriptor before each read.
macOS keeps the blocking read.

## Results

Run 20261008T082159Z, load 1.2 to 1.8. Times in ms.

| Terminal | Grid | Plain cat+DA1 | SGR cat+DA1 | Idle DA1 p50 | Idle DA1 p99 |
|---|---|---|---|---|---|
| Seer (T-462) | 40x148 | 147 | 81 | 0.054 | 0.076 |
| Alacritty | 40x149 | 135 | 79 | 0.071 | 0.095 |
| Ghostty | 40x149 | 174 | 278 | 0.067 | 0.097 |
| kitty | 40x149 | 163 | 211 | 3.083 | 3.107 |
| Seer T-448 baseline | 40x148 | 170 | 60 | 4.553 | 4.617 |
| Seer T-462 before epoll | 40x148 | 174 | 78 | 0.054 | 0.076 |

- Over 6 runs of Seer and Alacritty on 2026-10-08 at a load below 2.5,
  Seer plain was 145 to 147 and Alacritty plain was 135 to 139. SGR was
  77 to 81 for both.
- The T-448 SGR number is low because the old runtime dropped output
  above 1 MiB, so it did not parse all bytes.
- kitty idle DA1 is 3 ms because of its default input_delay.
- Long burst: yes in the pane for 60 seconds. seer-runtime RSS stayed at
  11.5 MB and seer-desktop RSS at 243 MB from 5 s to 65 s, and the window
  painted during the burst.

## Findings

- SGR output and idle DA1 match or beat Alacritty.
- Plain output is 8 to 9 percent slower than Alacritty (about 11 ms). In
  this run, cat used about 10 ms more system CPU under Seer than under
  Alacritty. The probe did not find the cause: a second parser thread
  and extra work per read did not change it.

## Known gaps

- Not restored on attach, because alacritty_terminal 0.26 keeps them
  private: scroll region, tab stops, active charset, title stack.
  Hyperlinks and colors set by OSC 4 are not restored either. The
  renderer does not use them.
- An attach that lands in the middle of an escape sequence during a
  heavy burst can show a few wrong cells until the program draws them
  again.
