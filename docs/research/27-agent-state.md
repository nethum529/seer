# Agent state in the local runtime

Date: 2026-10-11
Issue: 481

## Reference findings

Read-only Herdr reference: dc6b6e5f42831ea7114f34dcf12900680eb36ae0.
No reference code is copied into Seer.

- src/detect/mod.rs identifies the foreground agent from the process
  group, executable name, and wrapper arguments. Shells have no agent.
- src/detect/manifest.rs evaluates ranked rules against bounded screen
  regions, the OSC title, and OSC progress. Claude and Codex have separate
  manifests under src/detect/manifests/. The rules use live prompt chrome
  and dialog controls rather than arbitrary words in an old transcript.
- Claude rules cover its prompt box, interrupt line, permission choices,
  and question forms. Codex rules cover its prompt, timed activity line,
  confirmation footer, question controls, and Action Required OSC title.
- src/pane.rs runs detection outside the PTY reader. Content revisions
  avoid repeated scans. PTY activity tells it when content can change;
  output alone cannot prove that an agent works. A redraw or echoed input
  can happen while the agent waits.
- src/pane/agent_detection.rs delays weak Working to Idle transitions
  with three confirmations and a 700 ms cap. A live blocker or visible
  idle prompt is stronger evidence. Transcript viewers keep the state.
- src/integration installs optional agent hooks. The current Claude hook
  reports SessionStart identity only. Codex hooks report SessionStart,
  UserPromptSubmit, Stop, and Interrupt. These do not cover every blocker.
- src/terminal/state.rs selects live full lifecycle hooks first for
  integrations that cover the whole lifecycle. Otherwise a newer visible
  blocker can override a non-blocked hook for the same detected agent.
  Process exit clears hook authority. The screen is the fallback.

## What Seer takes

Use foreground process identity before screen evidence. Use separate,
small Claude and Codex rules against the live terminal grid. Give live
question and permission controls priority over idle prompts. Use explicit
work chrome before a short output-activity fallback. Keep detection off
the PTY output and input paths, bound the scan, and publish only changes.

Do not install hooks or change agent configuration. The current reference
hooks do not cover all required states. Screen detection is required even
with those hooks. Do not add Herdr's manifest updater, other agents, resume
tracking, or integration installer to this ticket.

## Seer starting point

UserSession::terminals currently maps a running non-shell program to busy.
PaneGrid already holds parsed terminal cells. PaneHost owns the PTY and
the grid. Terminals messages already go to own and read-only streams.
The broker routes them. On macOS PtySession::foreground_name currently
returns an empty string, so process identification must work there too.

Terminal speed is measured with the corpora and DA1 barrier method from
25-terminal-speed.md. The existing driver requires Linux and Hyprland;
the Mac check will use the same shell workload and raw parse barrier.

## Design and checks

### Design before implementation

- Add an optional agent_state to TerminalInfo. Keep the old state field
  and its idle/busy meaning for older clients. Older senders omit the new
  field; older readers ignore it. Use the existing Terminals message.
- A worker for each pane samples at 250 ms. It owns a duplicate PTY
  descriptor and a weak grid reference. Closing the pane cancels it.
  OS process reads and classification run outside the session and PTY
  reader locks. A busy grid is skipped with try_lock.
- The worker identifies Claude or Codex in the foreground process group.
  It collects the last 32 nonempty live rows, within 256 physical rows
  and 512 columns. Owner scrollback does not change these rows. OSC title
  is extra evidence.
- Live permission or question controls win. Then live work chrome, then
  the ready prompt. Weak output activity lasts at most 750 ms. A previous foreground job's title is ignored until the title changes
  again. A prompt below old dialog text excludes the old dialog controls.
- The poll driver reads the cached result and publishes changed terminal
  metadata to own and read-only streams. The broker does no detection.
- The desktop uses the existing 16 px ring assets and colors. A missing
  agent_state leaves an empty slot. No new motion or extra status text.
- On macOS a Terminals event requests the existing native frame after
  applying metadata and notifying GPUI. State changes and agent exit
  repaint without mouse or key input. This adds no UI polling timer.

### Implementation and contract checks

The protocol adds AgentState with working, idle, and waiting values. An
absent agent_state means no agent state. The old state field remains.
The existing Terminals route carries it through the broker unchanged.
The detector supports native executables and Node or Bun CLI wrappers.
macOS uses proc_name, process group members, and KERN_PROCARGS2. Linux
uses the foreground process group and /proc. Detection adds no dependency.

An e2e fixture runs as Claude, Codex, and an unknown program in a real PTY.
It checks task, permission, question, prompt, old dialog text, quiet work,
exit, and identical owner and read-only metadata. The test failed against
868c5e8 with missing idle before the detector was wired in. The final
cargo test -p seer --test agent_state passed in 9.11 seconds after the
repaint fix. Subsequent
transitions must reach both streams within 1200 ms.

A separate real broker check used Iroh and two room identities, Alice and
Bob. Metadata was equal for Idle, Working, Waiting, Idle, and agent exit.
Transitions took 77.0, 252.5, 265.6, and 342.6 ms. No broker code changed.
A scratch wire check used the exact old and new TerminalInfo definitions:
old readers accept new metadata; new readers accept omitted agent_state.

### Real agents on this Mac

Claude Code 2.1.296 and Codex CLI 0.162.1 ran in the Seer runtime PTY.
No integration hooks or detector settings were installed.

- Claude showed Idle at its prompt, Working while it thought and ran a
  command, Waiting at AskUserQuestion, and Waiting at a Bash permission
  for touch /tmp/sol481/permission-check. After an answer it showed Idle.
  The command was approved once, without an always-allow rule.
- Codex showed Idle at its prompt, Working while it produced a response,
  and Waiting at request_user_input in Plan mode. After Red was selected,
  it finished and showed Idle. It ran with on-request approval and a
  read-only sandbox. These were session flags, not detector requirements.
- The desktop showed the three existing 16 px glyphs. Waiting used the
  existing amber token. Close-up crops checked the ring next to Claude.
  The 650 px compact strip showed the same Idle glyph next to Codex.
- The runtime cleared agent_state when each agent exited. The native
  repaint check below verifies that the shell loses its ring without
  mouse or key input.

### Native repaint regression on this Mac

The old native window stayed on Shell while a foreground Claude fixture
reported Idle. No mouse or key input was sent to the window. Metadata
arrived on the traced socket, and receive already called notify.
The macOS native layer request was limited to keyboard echo.

The fixed window requests that same native frame for Terminals events.
Foreground Claude and Codex fixtures changed state through the runtime
protocol. Native screenshots and ring pixel checks used no window input.
The first correct captured frames give these elapsed upper bounds:

| Fixture | Working | Waiting | Idle | Ring cleared on exit |
| --- | --- | --- | --- | --- |
| Claude | 612.9 ms | 212.5 ms | 286.6 ms | 973.4 ms |
| Codex | 430.9 ms | 365.5 ms | 494.2 ms | 893.4 ms |

Claude also went from Idle to question Waiting by 188.0 ms, then exited
from Waiting. Codex exited from Idle. Close-up captures show the existing
glyphs aligned with the labels and a blank slot on Shell. Initial Codex
screenshot calls were too slow to bound latency, so the capture was
repeated. These are captured-frame upper bounds, not exact paint times.
Evidence is in /tmp/sol481/native-repaint-result.json and native-*.png.

### Speed check

Use the method in 25-terminal-speed.md: the same SHA-checked 11 MB plain
corpus, seeded SGR corpus, 148 by 40 grid, six runs per corpus, discard
run one, and 300 DA1 samples per repetition. This Mac check used the
existing terminal_io.zsh workload and a PaneGrid raw mirror that confirms
Parsed only after it parses preceding output. It measures parsing, not
GPUI paint. Both runtime binaries were debug builds with Rust 1.99.
The before binary came from 868c5e8. A native foreground fixture named
Claude kept detection active in the after workload; metadata confirmed
Working. No agent configuration was needed for this fixture.

Initial runs during concurrent builds had large swings and were excluded.
Three later repetitions, in both phase orders, give 15 retained samples
per corpus and 900 DA1 samples per phase:

| Measure | Before | After |
| --- | --- | --- |
| Plain cat plus DA1 median | 1592.7 ms | 1646.1 ms |
| SGR cat plus DA1 median | 1070.5 ms | 1032.1 ms |
| DA1 p50 | 0.202 ms | 0.184 ms |
| DA1 p99 | 0.626 ms | 0.624 ms |

Plain changed by +3.4 percent; SGR changed by -3.6 percent. Plain sample
ranges were 1453.1 to 6634.1 ms before and 1436.0 to 2028.4 ms after.
The last repetition used 18.12 runtime CPU seconds before and 15.92 after.
The mixed direction and large shared-machine swings limit the conclusion:
no clear slowdown was isolated, but this is not proof of zero overhead.
A quiet release build and GPUI paint comparison remains useful.

A separate 300-input echo check used a recognized raw-mode agent fixture.
Input to parsed echo was 0.241/1.810 ms p50/p99 before and 0.192/0.934 ms
after in one pair. In reversed order it was 0.212/3.163 ms before and
0.195/4.540 ms after. There was no consistent direction under shared load.
The detector uses try_lock and does OS reads and classification outside
the PTY reader lock. The scan itself holds that lock for a bounded copy.

### Gates and limits

- cargo fmt --all -- --check and git diff --check passed.
- Workspace and all-target clippy passed with warnings denied on Rust
  1.98.0, also with cognitive-complexity-threshold set to 21. Rust 1.99
  warns on the unchanged seer-net AtomicUsize::fetch_update call.
- cargo +nightly udeps --workspace --all-targets found no unused dependency.
- A conservative syntax-tree branch count, including let-else and ?,
  found at most 13 in the changed production files. Files are under 500
  lines; the largest changed Rust file is pane_grid.rs at 443 lines.
- Desktop build, the new e2e contract, terminal_contract, and
  terminal_names passed. The runtime e2e suite is Linux-only and ran
  zero tests on this Mac. Linux live agent checks were not run.
- The Mac TCP broker probe closes accepted nonblocking connections. Iroh
  passed. The TCP accept path is unchanged and is outside this ticket.
- A Mac paint blocker was found: correct metadata reached the window, but
  a frame can stay stale until keyboard input forces a native frame.
  A socket proxy and a process sample ruled out missing metadata and a
  stopped reader. A fresh window could also retain its initial blank
  frame. An Enter key drew the current Shell row with no ring.
  The receive path already calls notify. It requested the macOS native
  layer draw and presentation only for keyboard echo. The coordinator
  approved fixing this path. Terminals events now request the same
  native frame after the model update, including metadata with no agent.
  Keyboard echo tracking is cleared only for an actual echo. No UI timer
  or change to GPUI is needed.
- Keep the agent-only status calls when merging T-478. That ticket removes
  the old misleading rings; these calls use the new agent_state instead.

### Review corrections, 2026-10-12

The supplied Claude 2.1.296 screen has a short Gesticulating spinner, a
wrapped tip, and the ready prompt. It has no interrupt hint. Replay it
through the runtime without an OSC title and retain Working after output
has stopped for one second. Reject a completed summary, quoted spinner,
and a spinner followed by a new response. The new e2e replay failed on
18507a5 with a missing Working state before changing the rule.

Linux must not enumerate the host process table for an unknown job.
Walk the foreground leader and shell descendants instead. Limit each
sample to 128 processes, 128 task entries, and 64 KiB of child PID data.
Check each candidate's current process group before recognizing it.
Read command arguments only for Node and Bun. Take a fresh bounded
snapshot each sample: a child spawn, exit, or exec can change the agent
without changing the foreground group. This avoids stale negative caches.
The Mac path already asks the kernel for at most 128 foreground members;
apply the same Node/Bun argument restriction there.

Rebase onto current main, 9ddea20, which includes d9e3e41. Keep the
optional agent state and visible rings,
with both T-478 cap_icon wrappers and the T-479 palette. Native captures
must show a changed ring and removal after exit without UI input.
Measure several Mac panes with vim, less, sleep, and a Node wrapper.
The coordinator will run the supplied check commands on Linux.

The final native build showed the short spinner ring by 275.7 ms,
Waiting by 272.2 ms, Idle by 167.8 ms, and no ring after exit by 379.2 ms.
Codex Working and Waiting were visible by 242.9 and 390.6 ms. At 650 px,
Idle, Working, Waiting, and exit were visible by 371.9, 158.6, 394.8,
and 384.9 ms. These are screenshot upper bounds. No mouse or key input
was used during the transitions. Enlarged row crops retained cap alignment.
A Node wrapper also showed its child agent and then cleared its ring
while the same wrapper PID and foreground group remained alive.

Two Mac timing pairs used five panes: a benchmark pane, vim, less, sleep,
and a quiet Node wrapper at a recognized Claude CLI path. The old binary
was 18507a5; the fixed binary used the same Rust 1.99 debug profile.
The controller used raw output on the quiet pane. A raw mirror parsed
the benchmark pane at 148 by 40, using the existing corpus and DA1 barrier.
Discard the first of six corpus runs. Ten retained samples per corpus:
plain median 8873.9 ms before and 11181.6 ms after; SGR median 5764.4 ms
before and 6076.8 ms after. Plain ranges were 4214.3 to 17681.9 ms before
and 7753.5 to 17531.7 ms after. DA1 p50/p99 over 600 samples was
0.544/37.932 ms before and 0.485/27.670 ms after. For 300 parsed echoes
per run, the first before/after p50/p99 pair was 0.348/5.947 ms and
1.063/25.016 ms. The reversed pair was 0.719/25.731 ms before and
0.319/5.449 ms after. Other builds were active. Output medians increased,
while input and query results had mixed directions. These loaded debug
runs cannot establish a performance verdict. The Linux check remains
with the coordinator; no host-wide /proc enumeration remains in this code.

Final clippy, fmt, udeps, agent-state e2e, terminal_contract, and
terminal_names passed. An isolated harness compiled and linted the actual
Linux walker source on this Mac; it did not exercise Linux process APIs.
The wider workspace test stopped at an unchanged local socket test:
45 passed and one failed. A separate Unix socket pair reproduced EINVAL
when socket2 resets the read timeout after the peer closes, without any
agent code. This is outside T-481.

### Skill log

Before initial ring code: read better-ui SKILL.md, animations.md,
enter-exit.md, icons.md, icon-transitions.md, performance.md, surfaces.md,
review-output.md, and agents/openai.yaml in full. Read the full Emil
emil-design-eng SKILL.md and ui-taste SKILL.md. Read UI taste principles.
Pulled the Emil and Jakub reference clones; all were current. Kept the
existing 16 px assets, palette, spacing, and instant state changes. Kept
an empty fixed slot for shells and unknown programs.

Before the UI commit: reread those better-ui files and Emil and ui-taste
skills in full. Rechecked close-up captures and the compact strip. No
asset, spacing, color, or motion change was needed.

During the paint investigation: read diagnosing-bugs in full. Captured
stale and current frames, traced the UI socket, sampled the live reader,
and tested a fresh attachment and a harmless Enter key. Read unstuck and
its techniques in full; compared twelve recovery angles. Reusing the
native frame request is the smallest fix.

Before the repaint fix: reread better-ui and all its reference files,
Emil emil-design-eng, and ui-taste in full. Kept state feedback instant
and reused native_frame.rs. The old native window remained on Shell
while the runtime reported Claude Idle, without mouse or key input.

Before the amended UI commit: reread the same full better-ui files,
Emil emil-design-eng, and ui-taste. Inspected the native screenshot
sequences and enlarged ring crops. Kept the existing geometry and
instant updates. Read review-animations and STANDARDS in full; no
animation was added and no motion defect was found.

Before the review rebase: reread every listed better-ui file, Emil, and
ui-taste in full. Pulled the three reference clones; all were current.
Kept both cap_icon calls, the current palette, and visible agent-only rings.
Before the review amendment and report: reread those files in full and
Emil review-animations and STANDARDS. Inspected enlarged wide and compact
ring crops and autonomous exit captures. No motion or alignment defect
remained in this change.
