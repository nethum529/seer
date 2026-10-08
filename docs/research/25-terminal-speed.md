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
  output. In Seer the runtime answers, but only after each raw window
  confirmed that it parsed every byte before the query (see Design). So
  the Seer number includes the parse in the window. The result is the
  mean of runs 2 to 6. Then it measures 300 idle DA1 round trips, 10 ms
  apart, and reports p50 and p99.
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
- When a read holds a query and a raw window follows the pane, the
  runtime holds the answer. It sends a Mark after the bytes of the read,
  and the window sends Parsed with the same number after it parsed them.
  The runtime writes the answer when every raw window confirmed. A window
  that does not confirm in 2 seconds (RAW_WAIT) is disconnected, and the
  answer goes out. With the window stopped by SIGSTOP, DA1 took 2002 ms
  and the window then showed that the link stopped.
- The answers go to a queue under the pane lock, and one answer thread
  per PTY writes them. So they reach the program in order, and the
  reader never waits for the PTY writer. Input that waits for a program
  that does not read cannot stop the output.
- The PTY reader thread of the runtime parses each read into the pane
  grid, writes the replies at once, and pushes the same bytes to each
  window that follows the pane. There is no poll interval on this path.
- A window that sends raw_output in TerminalCapabilities gets the raw
  PTY bytes of the pane that it watches, not cell frames. Raw frames use
  the top byte of the 4 byte length prefix (0x80 plus a kind), so JSON
  frames do not change. Kinds: Output, Start (size), Size (resize in
  stream order), SyncEnd (a synchronized update ended by time in the
  runtime), Mark (see above).
- The window always sends raw_output. A runtime from before T-462 does
  not know the field (serde default) and sends cell frames. The window
  shows whichever comes. A cell frame never replaces a raw screen,
  because a runtime can send one cell frame before it reads the request.
- On attach, the runtime sends Start and then VT bytes that rebuild the
  screen, the scrollback (1000 lines), the modes, the cursor, the
  primary screen under the alternate screen, the G0 to G3 character sets
  and the active set. Then it sends the bytes of an escape sequence or
  UTF-8 character that the program did not finish yet. Control bytes
  inside an escape or CSI sequence ran at once in the runtime, so they
  are not sent again. Then live bytes
  follow, with no gap and no overlap, because all of it happens under the
  pane lock.
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

Run 20261008T092403Z, load 2.1 to 2.6. Times in ms. The Seer numbers
include the Mark and Parsed round trip.

| Terminal | Grid | Plain cat+DA1 | SGR cat+DA1 | Idle DA1 p50 | Idle DA1 p99 |
|---|---|---|---|---|---|
| Seer (T-462) | 40x148 | 147 | 80 | 0.077 | 0.109 |
| Alacritty | 40x149 | 134 | 81 | 0.073 | 0.111 |
| Ghostty | 40x149 | 171 | 282 | 0.068 | 0.102 |
| kitty | 40x149 | 163 | 209 | 3.094 | 3.124 |

Earlier rows from run 20261008T082159Z, before the Mark round trip:

| Terminal | Grid | Plain cat+DA1 | SGR cat+DA1 | Idle DA1 p50 | Idle DA1 p99 |
|---|---|---|---|---|---|
| Seer T-448 baseline | 40x148 | 170 | 60 | 4.553 | 4.617 |
| Seer T-462 before epoll | 40x148 | 174 | 78 | 0.054 | 0.076 |

- After the answer thread (run 20261008T100007Z, load 1.5): Seer plain
  146, SGR 80, idle DA1 0.084 / 0.120; Alacritty plain 135, SGR 80, idle
  DA1 0.075 / 0.115.
- Over many runs of Seer and Alacritty on 2026-10-08 at a load below
  2.5, Seer plain was 145 to 148 and Alacritty plain was 133 to 139. SGR
  was 77 to 81 for both.
- The T-448 SGR number is low because the old runtime dropped output
  above 1 MiB, so it did not parse all bytes.
- kitty idle DA1 is 3 ms because of its default input_delay.
- Long burst: yes in the pane for 60 seconds. seer-runtime RSS stayed at
  11.5 MB and seer-desktop RSS at 243 MB from 5 s to 65 s, and the window
  painted during the burst.

## Findings

- SGR output and idle DA1 match Alacritty, and idle DA1 includes the
  parse in the window.
- Plain output is about 10 percent slower than Alacritty (147 against
  134 ms). The cause is not known. What the probes show:
  - cat almost never waits (1 to 6 voluntary waits) and uses no user
    CPU. Its system CPU is 136 to 148 ms under Seer and 134 to 137 ms
    under Alacritty. So the gap is in the kernel tty write path of cat,
    not in a wait for the reader.
  - Reads: for 6 cats, the Seer reader made about 28 500 reads per cat
    (about 390 bytes each). Alacritty made about 55 000, and about half
    of them are the EAGAIN read that ends each drain loop. So the number
    and size of data reads are about the same.
  - The runtime started from the Hyprland session scope (the scope of
    Alacritty) gave 148 ms. So the cgroup is not the cause.
  - Not the cause in probes: CPU pinning and SMT, closing descriptors in
    the child, a nonblocking master, allocation or a mutex per read, a
    second parser thread, and up to 4 busy threads (at most 7 ms).

## Known gaps

- Plain output is about 10 percent slower than Alacritty (see Findings).
- Not restored on attach, because alacritty_terminal 0.26 keeps them
  private: scroll region, tab stops, title stack. Hyperlinks and colors
  set by OSC 4 are not restored either. The renderer does not use them.
