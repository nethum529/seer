# Window design v2

Mockup: docs/design/06-window-v2.html. It replaces 03-gpui-window.html and
05-room-screens.html. The structure stays the same: people tree sidebar,
flush terminal, no card, no eyebrow headers. Only the look changes.

Research notes: docs/design/06-research.md. The answers for R-454, R-455,
and R-456 in docs/design/05-room-screens.md stay valid. This document does
not repeat them.

## How to open the mockup

- Open the file in a browser. It loads the fonts from Google Fonts, so the
  computer must be online.
- The bar above the window picks a state, a font, and a width. It is mockup
  chrome, not product.
- Each state has a link: 06-window-v2.html#main/own. Add &font=geist or
  &font=plex for another font, and &narrow for the narrow window. Example:
  #main/own&font=geist&narrow.

## Font

Recommendation: Inter for the UI and JetBrains Mono for the terminal. Both
use the OFL, so Seer can bundle them.

- Inter has the largest x-height of the three candidates. At 12 px the task
  lines and the times are the easiest to read. We compared all three fonts
  in close crops.
- Inter has tabular figures, so times do not move when they change.
- Bundle static Inter Text TTFs (Regular, Medium, SemiBold). Use Inter
  Display for the 22 px join heading only. GPUI support for variable axes
  is limited.
- JetBrains Mono is the most legible mono of the three at 12.5 px. It has
  the full box drawing set, which the terminal needs.
- Second choice: Geist and Geist Mono. One family, a bit crisper and
  narrower, a smaller x-height.
- Third choice: IBM Plex Sans. It is wider and takes more room in the
  248 px sidebar.
- The owner calls Noto Sans, the system fallback today, basic.

## Type scale

| Role | Size / line | Weight |
| --- | --- | --- |
| Small: task line, time, meta | 12 / 16 | 400 |
| Body: names, menu items, buttons | 13 / 18 | 400 or 500 |
| Sheet titles | 15 / 20 | 600 |
| Join heading | 22 / 28 | 600 |
| Terminal | 12.5 / 19 | 400 |

- Weights are 400, 500, and 600 only. Person names in the sidebar use 600.
- One exception to the scale: avatar initials are 11 px, weight 600.
- No letter spacing. GPUI 0.2.2 has font_features but no letter spacing.
- Terminal padding is 8 px top and bottom, 12 px left and right, as in
  crates/seer-desktop/src/window.rs terminal(). Size and line come from
  crates/seer-desktop/src/screen.rs.
- Sentence case everywhere. Names start with a capital letter: Claude,
  Shell, Fish, Nethum, and the room name.

## Tokens

T-479 replaces the near-black base with opaque charcoal layers. Research,
measured product crops, and the full contrast matrix are in
[Dark surface depth](07-surface-depth.md).

| Token | Before T-479 | Current | Role |
| --- | --- | --- | --- |
| bg / WINDOW | #0a0a0a | #1b1b1b | Window and flush terminal base |
| surface / SURFACE | #0a0a0a | #222222 | Sidebar, title bar, narrow tab strip |
| hover | #131313 | #292929 | Quiet hover on chrome |
| raised | #171717 | #2b2b2b | Menus, tooltips, sheets |
| sel | #1c1c1c | #333333 | Active row |
| chip | #262626 | #383838 | Menu hover, secondary controls |
| chip-2 | #303030 | #404040 | Avatars, secondary control hover |
| line | #1f1f1f | #3a3a3a | Structural dividers |
| line-2 | #2a2a2a | #505050 | Menu and input edges |
| text / accent | #fafafa | #fafafa | Titles and primary action fill |
| text-2 | #d4d4d4 | #d4d4d4 | Default terminal text and menu labels |
| muted | #a3a3a3 | #b8b8b8 | Task text |
| faint | #8a8a8a | #adadad | Small metadata and hints |
| fainter | #6b6b6b | #939393 | Idle rings and offline dots |
| on-accent | #171717 | #171717 | Text on primary action fill |
| attn | #fbbf24 | #fbbf24 | Waiting status |

- Semantic text passes 4.5:1 on every fill it uses. The lowest pair is
  faint on chip-2, 4.62:1. Default terminal text is 11.62:1.
- Idle rings are 5.18:1 on chrome and 4.11:1 on the active row.
- Structural dividers are 1.40:1 on chrome. Input edges are 2.14:1 on
  bg; menu edges are 1.76:1 on raised. These improve the old levels.
- The known WCAG 1.4.11 border exception remains: borders are below 3:1.
- Tooltips use raised and text-2. Program-supplied terminal colors and
  dim, hidden, and inverse attributes remain under the program's control.
- No surface transparency, blur, or light appearance. The terminal stays
  flush with no card, border, radius, margin, or second fill around it.

## Sidebar tab row

- Line 1: status glyph (16 px) | name, 13/18, weight 500 | time, 12 px,
  tabular, faint.
- Line 2: task, 12/16, faint. On the active row it is muted.
- Padding 7 px top, 10 px sides, 8 px bottom. Gap between rows 2 px.
  Radius 8.
- One grid for the sidebar: icon center at x=26, text at x=44. The window
  lights, avatars, plus, status glyphs, and the room dot use it.
- Native macOS window controls keep their OS size and spacing.
- Status glyphs on a 16 px grid. The glyph shows state, not kind, so agents
  and shells use the same glyphs:
  - Idle: empty ring.
  - Working: ring with a half disc.
  - Waiting for its person: amber ring with a full disc.
- A long task line is cut with an ellipsis. The full text is in the
  terminal. The GPUI tooltip must also open on keyboard focus, not only on
  hover.
- New terminal is a quiet row with a plus, at the end of the list. It
  returns to your own terminals and focuses the new terminal, also when
  you watch another person. Its shortcut is in its tooltip: Cmd N on
  macOS, Ctrl Shift N on Linux.

## Sidebar visibility (T-514)

- Cmd B on macOS and Ctrl Shift B on Linux hide or show the sidebar.
  Plain Ctrl B stays with the terminal.
- Cmd Shift B on macOS and Ctrl Alt Shift B on Linux show the sidebar
  and focus its tabs. Shift Tab reaches the person menu. Tab returns to
  terminal input. The terminal title tooltip lists both sidebar shortcuts.
- Hide and show share the top bar's critically damped spring, with
  frequency 40. Retarget from the current position and velocity. A full
  change settles in about 280 ms. This motion is required for the sidebar
  shortcut. Initial visibility and OS reduced motion are instant.
- The terminal fills the free width. Its grid stays fixed during motion
  and changes once at the end. OS reduced motion makes the change instant.
- Selection and sidebar scroll stay in place. Hiding moves focus to the
  terminal. Showing restores the previous focus. An open menu closes and
  returns focus to its trigger when the sidebar comes back.
- New terminal keeps Cmd N or Ctrl Shift N while the sidebar is hidden.
  The focus shortcut reveals the tabs and gives access to the person menu.
- Save visibility in the existing settings file. Load it without motion.
  Existing settings show the sidebar by default. Narrow layouts keep their
  current top bar and tab strip rules.

## Terminal names (R-468)

- Where the name lives: in the local runtime, as Tab.title in the runtime
  tree (crates/seer-core/src/tree.rs). The runtime already saves the tree
  in its snapshot (crates/seer-runtime/src/persistence.rs), so the name
  stays after a restart. The local runtime owns its terminals, so the
  broker does not own the name. It only relays it.
- On the wire: TerminalInfo (crates/seer-core/src/proto/mod.rs) needs a new
  title field. Today name is the foreground program or "shell"
  (crates/seer-runtime/src/user_session/terminals.rs).
- No name set: the tab shows the program name with a capital letter
  (Claude, Fish, Cargo, Nvim). A shell shows its cwd on line 2.
- TUI: ADR 0010 allows bug fixes only. The TUI shows the name where it
  shows the program today. The TUI gets no rename UI.
- How to rename, only on your own terminals:
  - Double click a tab, the title, or a pill in the narrow strip.
  - Right click a tab, then Rename. Shift+F10 opens the same menu. The
    menu has Rename, then Close. Close is only for your own terminals.
  - Close ends a shell at once and selects the next terminal, or the
    previous terminal when the closed one was last. Other programs need
    confirmation in the room menu's shared sheet. The sheet names the
    program and focuses Cancel first. Cancel keeps the terminal.
  - After the last terminal closes, the list is empty and New terminal
    stays available.
  - F2 on the focused tab.
- In the field: Enter saves, Escape cancels, a click outside saves. An
  empty name goes back to the program name. 40 characters at most.
- Title bar: your own terminal shows only its name. A terminal of another
  person shows "Kyler / Name", with a lock and "Read only" at the right.

## What changed from 03 and 05

- The terminal is flush. No card (AGENTS.md wins over 03).
- The title shows only the name for your own terminals. For other people
  it shows "Kyler / Name" and a "Read only" mark. This replaces the
  floating read only bar of 03.
- The footer room button uses the capitalized room name. The room address
  moved into the room menu.
- New terminal is a quiet row, not a button. Its shortcut moved to the
  tooltip.
- People menu meta, by priority: Waiting (amber), Offline Nh, Away Nm,
  N working.
- Names are capitalized everywhere.
- Menu radius is 11 (item 6 + padding 4 + border 1). Segment buttons have
  radius 5.

## Motion

| What | Duration | Notes |
| --- | --- | --- |
| Tab switch, hover, rename | none | Instant |
| Menu open | 150 ms | Opacity only |
| Menu close | 80 ms | Opacity only |
| Sheet open | 200 ms | Centered, opacity only |
| Sheet close | 140 ms | Opacity only |
| Button press | none | Immediate feedback, no scale |
| Copy icon | 300 ms | Cross-fade, cubic-bezier(0.2, 0, 0, 1) |

- Curve for all other motion: cubic-bezier(0.23, 1, 0.32, 1).
- A menu or sheet that opens or closes from the keyboard has no motion.
  People use these keys many times a day.
- Scale is not used because GPUI 0.2.2 has no subtree transform.
- Reduced motion: opacity stays. No layout, text size, or position animates.

## Owner decisions

The owner approved this design on 2026-10-11, as shown in the mockup.

1. Font: Inter and JetBrains Mono.
2. The "Read only" mark in the title bar replaces the floating read only
   bar of 03.
3. The people menu keeps the agent count ("2 working").
