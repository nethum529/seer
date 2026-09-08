# Mission 15 findings: feel and behavior, closer to herdr

Status: QA pass done on 2026-09-04 against main 9fc6b03 with the release
build. Two accounts on one server on this machine: alice (owner, TCP)
and bob (joined with the pasted line, iroh). Every action from the
mission was tried: open, switch, close, resize, focus, input, mouse,
scroll, quit, reattach, server stop. Herdr rules come from the herdr
source at /home/nethum/Projects/_research/herdr.

Each item has one of four marks: fixed here, bundle, mission 14, or
new ticket. Fixed here means the fix is in this PR. Bundle and mission
14 items are not touched in this PR because those PRs own the files.

## Fixed here

1. Blank boxes after a resize, blank viewer in a small window.
   Did: shrank the owner window to 60 by 20 with two terminals. Then
   opened a terminal of the owner from a watcher window of 80 by 24.
   Seer: every box was empty. The watcher viewer was empty.
   Cause: the cell widget showed the last rows of the frame. The
   frame has empty rows below the cursor, so the last rows were blank.
   Herdr: a pane always has the size of its area, so herdr never
   clips a frame. The spec says a box shows the last rows. My call:
   the last rows with content. The widget now skips the empty rows at
   the end of the frame. File: crates/seer/src/terminal_cells.rs.
   Note for the bundle: viewer.rs computes the cursor row from
   rows.len() minus the height. When the remote pane is taller than
   the viewer, the widget start row differs from that. Part 4 of the
   bundle must use the same rule as the widget for the cursor row.

2. Solid background in every terminal cell.
   Seer: cells with the default background were painted with
   panel_bg. Section 4 says no solid fill anywhere.
   Herdr: cells with the default background use the host terminal
   background. Fixed: the default cell background is Color::Reset.
   File: crates/seer/src/terminal_cells.rs.

3. Cursor shape stays changed after quit.
   Did: in the viewer ran printf with the DECSCUSR bar sequence, then
   quit. Seer: the host cursor kept the shape seer had set.
   Herdr: never changes the host cursor shape. Fixed: the client sets
   the default shape before it leaves the alternate screen.
   File: crates/seer/src/terminal_session.rs.

## Bundle

4. The letter f in the viewer toggles follow and freezes the frame.
   Did: typed echo hello from alice. Seer: the shell got "echo hello
   rom alice" and the viewer froze. Herdr: every key goes to the pane.
   Bundle part 4 removes the follow key.

5. Left click on a person row opens the menu. Right click does
   nothing. Spec 2.5: left click selects, right click opens the menu.
   Bundle part 5.

6. Mouse wheel in the viewer does nothing. Scrollback only with shift
   plus page up. Herdr: the wheel scrolls the scrollback, sends arrow
   keys in the alternate screen, or sends mouse reports when the app
   asked for them. Bundle part 5.

7. The wheel over the people list uses a fixed 26 column split, wrong
   at narrow widths. Bundle part 5 with the narrow width rule.

8. Look: header reads "you may type: yes" not "input: allowed". The
   quit dialog has two empty rows. The footer is cut at 60 columns.
   The people column stays 26 wide at 60 columns. Bundle part 2.

9. The viewer covers the whole screen and Resize sends the whole
   screen size. Spec 2.2 puts the viewer inside the terminal area.
   Bundle part 4.

10. No tab strip, no x to close, no number keys. Bundle part 3.

## Mission 14

11. The marker enters the PTY. Did: bob typed echo hi bob into alice's
    shell with the grant. Seer: the shell printed
    "fish: Unknown command: '[seer:'". Spec 3.4. Mission 14.

12. Pane size. Bob's pane stayed 80 by 24 until bob opened the viewer.
    Boxes show a cut of a wider pane. A watcher with a smaller window
    sees a cut frame. Spec 8.2 size rule. Mission 14.

13. Broker log noise. Each client quit writes "runtime evicted slow
    connection N: Broken pipe" lines. Forwarding. Mission 14.

## New tickets

14. Issue 310. Esc never reaches the shell or the agent. Every Esc in the viewer
    goes back to the grid. Herdr: Esc goes to the pane, a prefix key
    leaves terminal mode. Spec 2.2 says esc back, so this needs an
    owner decision. Files: viewer.rs.

15. Issue 311. Focus events are not forwarded. The client enables focus change
    reporting and the runtime maps InputEvent::Focus, but tui.rs drops
    FocusGained and FocusLost. Herdr sends ESC [ I and ESC [ O to the
    pane when the app enabled mode 1004. Files: tui.rs.

16. Issue 312. The kitty keyboard protocol is not enabled. Herdr pushes the
    keyboard enhancement flags at start. Without them Shift+Enter and
    Ctrl+Enter are plain Enter, which matters for coding agents.
    Files: terminal_session.rs, input.rs, runtime input.

17. Issue 313. A shell exit starts a new shell in the same pane. Did: typed exit
    in the viewer. Seer: the pane went empty, then a new fish started
    two seconds later. Herdr marks the pane as exited and does not
    start a new shell. Files: seer-runtime pane_host.rs.

18. Issue 314. Server stop leaves a raw error and a slow exit. Did: seer stop
    while both clients were attached. Seer TCP client: exits at once
    with "error: failed to fill whole buffer". Seer iroh client: keeps
    running with stale state, keys still work, exits after 35 seconds
    with the same error. Wanted: one plain line and a prompt exit. The
    broker has no SIGTERM handler, so it never closes the QUIC
    connection. Files: seer-broker lifecycle, tui.rs, commands.rs.

19. Issue 315. The search filter stays after Enter. Did: slash, typed bo, Enter.
    Seer: the list shows only bob until slash then Esc. The owner row
    is hidden. Herdr rule not verified. Files: tui_navigation.rs,
    state.rs.

20. Issue 316. No host window title. Herdr sets the title with OSC 0. Seer leaves
    the title of the shell that started it. Files:
    terminal_session.rs.

## Checked and fine

- j and k wrap at the ends of the people list. Herdr next_workspace
  wraps the same way.
- Double click on a box opens the viewer. Enter, Tab, n, and the
  person menu grant toggle work.
- Bracketed paste and Ctrl+C reach the shell.
- Wide characters render in place.
- Reattach after quit shows the same terminals with their content.
  The grant survives the quit.
- Presence: the online count drops when a person quits, the row
  stays.
- seer detach from a second shell ends the client with the detached
  message.
