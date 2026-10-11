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

The hues stay neutral. Some steps change from
crates/seer-desktop/src/palette.rs and from 05:

| Token | 05 / palette.rs | v2 | Why |
| --- | --- | --- | --- |
| line | #262626 | #1f1f1f | Quieter dividers |
| line-2 | #404040 | #2a2a2a | Quieter menu and input borders |
| hover | #171717 | #131313 | Hover is a hint, not a fill |
| sel | none | #1c1c1c | Only the active row has a fill |
| faint | #737373 | #8a8a8a | 4.18:1 failed; now 5.73:1 |
| fainter | #525252 | #6b6b6b | Idle ring and offline dot now 3.72:1 |
| attn | none | #fbbf24 | The only terminal state color (waiting) |

- All text pairs pass 4.5:1. Faint text is 5.73:1 on the window, 4.94:1 on
  the active row, and 5.19:1 on menus. The full list of measured pairs is
  in the skill log for this task.
- Known gap (WCAG 1.4.11): input borders (#2a2a2a) are 1.38:1. We keep them.
  A 3:1 border is about #626262, which is too heavy for this window.

## Sidebar tab row

- Line 1: status glyph (16 px) | name, 13/18, weight 500 | time, 12 px,
  tabular, faint.
- Line 2: task, 12/16, faint. On the active row it is muted.
- Padding 7 px top, 10 px sides, 8 px bottom. Gap between rows 2 px.
  Radius 8.
- One grid for the sidebar: icon center at x=26, text at x=44. The window
  lights, avatars, plus, status glyphs, and the room dot use it.
- Status glyphs on a 16 px grid. The glyph shows state, not kind, so agents
  and shells use the same glyphs:
  - Idle: empty ring.
  - Working: ring with a half disc.
  - Waiting for its person: amber ring with a full disc.
- A long task line is cut with an ellipsis. The full text is in the
  terminal. The GPUI tooltip must also open on keyboard focus, not only on
  hover.
- New terminal is a quiet row with a plus, at the end of your own list
  only. Its shortcut is in its tooltip.

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
    menu has one item now. R-452 can add Close later.
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
| Menu open | 150 ms | Opacity, scale .97 from the trigger |
| Menu close | 80 ms | Same properties |
| Sheet open | 200 ms | Centered, scale .97 |
| Sheet close | 140 ms | |
| Button press | 150 ms | Scale .96 |
| Copy icon | 300 ms | Cross-fade, cubic-bezier(0.2, 0, 0, 1) |

- Curve for all other motion: cubic-bezier(0.23, 1, 0.32, 1).
- A menu or sheet that opens or closes from the keyboard has no motion.
  People use these keys many times a day.
- Reduced motion: no scale. Opacity stays.

## Owner decisions

The owner approved this design on 2026-10-11, as shown in the mockup.

1. Font: Inter and JetBrains Mono.
2. The "Read only" mark in the title bar replaces the floating read only
   bar of 03.
3. The people menu keeps the agent count ("2 working").
