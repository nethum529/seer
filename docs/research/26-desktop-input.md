# Desktop terminal input

T-449, checked on 2026-10-11.

## Path

The focused terminal sends text, keys, paste, mouse, and focus through
TerminalInput on its own local runtime socket. The runtime uses its existing
Alacritty key and mouse encoder. Printable text uses GPUI's native input
handler. No room connection is needed for this path.

The display keeps an Alacritty mirror. Scrollback and selection use that
mirror and its native selection rules. Shift permits selection when an
application requests mouse reports. Copy is Cmd C on macOS or Ctrl Shift C
on Linux. Paste is Cmd V or Ctrl Shift V. Normal paste changes LF to CR;
bracketed paste preserves LF and removes ESC and Ctrl C, as Alacritty does.
T-500 enables Kitty keyboard negotiation in the runtime. The desktop sends
the key with the legacy bytes made from its own platform rules. The runtime
uses the key only when the active PTY mode requests disambiguation.
Otherwise, it writes the supplied legacy bytes, including for input from
a person on another OS.

On macOS, Option text goes through AppKit. A local event monitor handles
named keys before GPUI 0.2.2 gives them to AppKit text services. AppKit would
otherwise consume F20 and Option Shift Tab. It also captures Control text
from NSEvent.characters because GPUI omits that text. This follows
Alacritty's native text rule after default bindings, rather than converting
the key name through a generic Control map. Plain keypad Enter sends LF;
modified keypad Enter keeps native text. Other printable text and marked
composition stay with the native input handler. The monitor also handles
Cmd B and Cmd Shift B before AppKit consumes those shortcuts.

Cmd B on macOS or Ctrl Shift B on Linux hides or shows the sidebar.
Cmd Shift B on macOS or Ctrl Alt Shift B on Linux reveals and focuses its
tabs. The Linux focus shortcut adds Alt so Ctrl Shift B can toggle.
Plain Ctrl B stays with the terminal. The title and terminal tab tooltips
show both sidebar shortcuts. Arrows select a terminal there.
Tab returns to terminal input. Tab, Escape, and arrows in the terminal go
to the shell. No animation delays input or focus.

## Contract checks

The desktop e2e test was written first. Before input existed, its raw shell
received no bytes instead of the expected keys. It now starts a real local
runtime, selects and copies Ready, checks keys and normal paste at the PTY,
and checks the sidebar shortcut and return path.

Native macOS checks used Seer and Alacritty 0.17.0 with default key bindings.
The same raw Python shell read bytes, saved them, and echoed them. The two
apps received the same sequence through native window control:

- 44 keys: text, Return, Backspace, Tab, navigation, F1 through F20, and
  Ctrl, Option, and Shift combinations. The full byte streams matched.
- 16 further keys with application cursor mode enabled. Arrow, Home, End,
  and combined modifier streams matched. Shift Home and Shift Page Up
  stayed local in the primary screen in both apps.
- After review, all earlier cases and 56 further cases matched individually:
  the seven review cases, Control letters, digits, punctuation, and modified
  keypad Enter. The 116 per-key captures had no differences. The expanded
  real-runtime e2e failed before the native text fix and passed after it.
- Option E then E committed UTF-8 c3a9. Option 2 committed e284a2.
- Bracketed paste sent 1b5b3230307e, the text with LF, and 1b5b3230317e.
- Mouse mode 1002 plus SGR mode 1006 sent press, release, drag, and wheel
  reports. Without mouse mode, the wheel showed history and drag selected
  Line052; Cmd C then Cmd V sent that selected text to the raw shell.
- A real Vim window saved the expected text after insert, Enter, arrows,
  Home, End, Backspace, Escape, and a two-line paste.
- The TUI and window attached to one PTY. The capture contained Tui449Win449,
  typed once from each frontend.
- Cmd Shift B, Tab, then z sent only z to the PTY, without an extra Tab byte.
  After the v2 rebase, the title tooltip replaces the permanent header hint.
  The updated contract test first failed because Tab focused New terminal;
  Tab now returns directly from the sidebar tabs to terminal input.
- A focused block cursor kept an ANSI concealed character hidden. The
  final release also repeated the sidebar return and Option composition.

These native checks did not run on Linux. The Linux GPUI input backends use
the same composition handler, but an actual Linux input method remains a
required follow-up check. macOS allowed window control and screenshots.

## Edit keys after negotiation

T-500, checked on 2026-10-15.

Fish 4 requests Kitty keyboard mode 5. The previous runtime ignored that
request, and the desktop converted Backspace, Enter, Tab, and Escape to
text before the runtime could see their modifiers. This lost the Control
modifier on Backspace.

The runtime now accepts the request and encodes modified edit keys with
the requested disambiguation. It remains the source of truth for keyboard
mode, including when a window attaches to an existing PTY. The sender makes
the legacy bytes so that macOS Control text and Linux Alt prefixes stay
unchanged, even when the receiving runtime uses the other OS.

Seer encodes Kitty flag 1 (disambiguation). Keyboard mode query replies are
masked to that flag. Release events, alternate key codes, all-key escape
encoding, and associated text flags are not supported.

The alacritty_terminal pin fixes upstream issue 9071 / PR 9073; remove it after a release includes the fix.

On macOS, Option Backspace keeps its legacy ESC DEL word deletion binding
in negotiated mode. This rule stays on the sending desktop. With native
Option text, the desktop clears Option from keys that carry legacy bytes
before negotiated encoding. The owner approved this difference from Alacritty 0.17.0
defaults, which send plain DEL in that mode. Other checked edit keys
follow those defaults. The checks cover separate Control, Option or Alt,
and Shift modifiers with Backspace, Delete, arrows, Home, End, Enter, and
Tab, with and without negotiation. Shift Home and Shift End remain local
scroll actions in the primary screen.

The GPUI e2e checks use a real local runtime and PTY. A separate fish 4
check types two words, deletes the second word, and reads the resulting
command line. Set SEER_TEST_FISH to the fish executable to run that check.
The raw capture check guards legacy bytes as well as negotiated bytes.

## Option as Alt from terminal app config

T-579, checked on 2026-10-21.

The existing font and color loader also reads explicit macOS Option as Alt
settings. App order, file paths, includes, and iTerm2 profile selection stay
in that loader. See [terminal keys](../guide/terminal-keys.md) for the values.
No setting, an invalid value, or an unreadable selected config keeps native
Option text. Ghostty's implicit keyboard-layout default is not imported.

The native key monitor reads the left and right Option device bits from
NSEvent. For a configured side, it gets text with Option removed while
keeping Shift and Control. It sends ESC plus that text as legacy bytes and
keeps Alt on the key used for Kitty flag 1. The other side still goes to
AppKit. Marked composition is never intercepted. Option Backspace keeps
ESC DEL in both modes, as required by the existing rule above.

Conversion runs before the shared local and remote input route. The sender
supplies the legacy bytes and negotiated modifiers. The receiver does not
read the sender's config or apply its own Option preference.

The native-text-e2e harness uses isolated HOME and XDG directories. Its
Option cases post native macOS key events to the app and assert the bytes
read by a raw shell in a real local runtime. They cover both sides, each
side alone, off, native composition, config includes, app precedence,
unreadable config, XML and binary iTerm2 profiles, and Kitty flag 1.
The first check failed before the change: Option B sent e288ab instead of
1b62 with Ghostty configured for both sides. Source references and raw byte
evidence are listed in the T-579 report.

## Input latency

The machine was an Apple M4 MacBook Air with 24 GB RAM, macOS 26.6.2.
The release comparison used JetBrainsMono Nerd Font Mono at 12.5 points,
a 91 by 30 grid in both apps, and the same raw Python echo program. Alacritty's config changed only geometry, font,
and cursor blinking. It did not change bindings or input scheduling.

Other work was running on this Mac. The apps were checked in alternating
order; the machine was not an isolated benchmark host.

The first, coarse check used Seer debug and Alacritty release. Alacritty
used Menlo and an 89 by 29 grid then, so it was only a capture sanity check.
It timed a native x key through the first screenshot whose
first text row changed. App order alternated. Each app had 24 samples;
the first four were warmup. Timing stopped before JPEG decoding.

| App | Samples | Minimum ms | Median ms | p95 ms | Maximum ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Seer debug | 20 | 380.9 | 397.8 | 412.2 | 421.1 |
| Alacritty release | 20 | 382.7 | 394.2 | 410.6 | 416.6 |

Every echo was present in the first capture. The roughly 400 ms capture
wait dominates these numbers. They are upper bounds, not useful measures
of terminal frame latency.

A second, temporary probe used the same local NSEvent KeyDown monitor and
monotonic clock in both apps. The physical x key started a sample. The
input callback recorded the expected echo cell. The draw code had to
include x at that cell; parsed output alone could not finish a sample.
The sample ended when the native presentation call returned. The probe
and fixture launchers were removed from the production build.

Both frontends and the local runtime were release builds. The process
paths were checked with lsof. Each app had two blocks of 24 x keys, in
Seer, Alacritty, Alacritty, Seer order. The first four samples in each
block were warmup and were excluded. Percentile 95 uses nearest rank.

| App | Samples | Minimum ms | Median ms | p95 ms | Maximum ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Seer release | 40 | 1.098 | 1.458 | 1.784 | 4.118 |
| Alacritty release | 40 | 1.095 | 1.290 | 3.631 | 7.374 |

The median is 0.168 ms slower in Seer. Its p95 is lower. This does not
prove the no-worse acceptance criterion. The coordinator accepted these
results on 2026-10-11. Physical screen latency remains unproved by the
coarse capture. The raw release samples are in
perf-samples/449/input-latency.csv. This comparison predates the v2 rebase;
the coordinator requested no further latency work.

Before removing redundant repaint notifications, the same release method
gave medians 1.417 and 1.329 ms, and p95 values 1.941 and 6.413 ms. The
repeat does not show a median gain from that cleanup. A further probe
disabled Metal display sync only for the input frame, then restored it.
It made Seer slower: median 2.230 ms, p95 8.512 ms, versus Alacritty
1.358 and 4.473 ms. That change was removed.

This software endpoint includes input delivery, the PTY echo, rendering,
and submission to the native presentation API. It excludes the compositor,
panel scanout, and the time before AppKit delivers KeyDown. It must not be
reported as physical key-to-photon latency.

GPUI normally waits for CVDisplayLink. On macOS the first screen update
after terminal input now requests display through the native view's layer.
GPUI's displayLayer callback draws and presents through its normal renderer.
The call runs on a later main-thread turn so it does not re-enter an App
borrow. A retained NSView keeps the delegate live. Ordinary output without
pending input keeps its current scheduling.

## Check limits

The focused desktop e2e and runtime terminal fixture pass. The unchanged
Mac attach test fails with EINVAL, including at base f974bde. The workspace
run also fails in the stop CLI, install replacement, real broker, and broker
startup tests. The stop test needs the Linux-only room server. The other
failures are outside the changed code; their causes have not all been
proved at base. Rust 1.99 clippy stops at an unchanged Atomic.fetch_update
deprecation in seer-net. The same workspace clippy gate passes with Rust
1.98. No unrelated code was changed for these failures.

## Sources

- Alacritty 0.17.0: the MIT source from the official Cargo registry,
  src/input/keyboard.rs, src/config/bindings.rs, src/input/mod.rs,
  src/input/bindings.rs, and src/display/mod.rs.
- Winit 0.30.13: src/platform_impl/macos/event.rs obtains native text
  from NSEvent.characters for text_with_all_modifiers.
- GPUI 0.2.2: EntityInputHandler, Window's frame callback, and the macOS
  displayLayer callback in the Apache-2.0 dependency source.
- alacritty_terminal 0.26.0: native selection, mouse modes, and the default
  Config with kitty_keyboard set to false.
- docs/research/25-terminal-speed.md for the raw output path from T-462.

No Zed application code was copied. The temporary Alacritty probe came
from its official MIT package and was built outside this repository.
