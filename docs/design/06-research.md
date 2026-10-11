# Design v2 research

Notes behind docs/design/06-window-v2.html. Screenshots are not in the
repo. Some type facts come from third party write-ups, not from the
product. Those are marked "third party".

## Products

- Linear (https://linear.app). Inter Variable with stylistic alternates.
  In-between weights (510, 590). Hierarchy comes from steps of white
  opacity, not color. Borders are faint (5 to 8 percent white). Sidebar
  rows are one line: 16 px icon plus label. Only the active row gets a
  fill. Source for type facts is a third party write-up:
  https://open-design.ai/plugins/design-system-linear-app/
- Cursor (https://cursor.com). Agent list rows: status icon, title, time
  at the right, one muted summary line. Only the selected row has full
  contrast. Other titles are muted. This is the closest model for the
  Seer tab row.
- Zed and Delta (https://zed.dev). Delta is "a multiplayer environment
  for coding with agents", the closest peer to Seer. Zed is GPUI. It
  bundles IBM Plex Sans for UI and Lilex for code, both OFL. It dropped
  "Zed Plex" because the OFL forbids the reserved name Plex in a fork:
  https://zed.dev/docs/visual-customization
- Harvey (https://www.harvey.ai). Custom serif for display, neutral sans
  for UI. Dark sidebar, one line items, names capitalized, one quiet
  accent. Design post:
  https://www.harvey.ai/en-US/blog/how-we-approach-design-at-harvey
- Ramp (https://ramp.com). Lausanne (commercial, not usable). Only two
  weights, regular and medium. Dense two line rows: name over a muted
  value.
- Raycast (https://raycast.com). Inter, weight 500 as the base in dark
  UI, small positive tracking on small text, Geist Mono for code. Third
  party source: https://open-design.ai/plugins/design-system-raycast/
- Superhuman (https://superhuman.com). Mail rows use three contrast
  levels (sender, subject, preview) and no dividers.
- Warp (https://warp.dev). The app has switches to remove block dividers
  and tab indicators, so less chrome is the default advice:
  https://docs.warp.dev/university/developer-workflows/beginner/how-to-make-warps-ui-more-minimal
- Mercury (https://mercury.com). Marketing page only, nothing for app
  rows.
- Vercel (https://vercel.com/geist/typography). Geist splits type into
  Label and Copy. Labels are "designed for single-lines" and have room
  for icons. Label 14 strong is the menu style, Label 13 is a second
  line, Label 12 is tertiary text in busy views. This matches the Seer
  tab row: a 13 px name, a 12 px task line.
- Arc (https://arc.net). Sidebar tab labels are 13 px, weight 500.
  Padding 8/12, radius 8, a 16 px icon with an 8 px gap. Only the
  current tab has a fill. Third party:
  https://open-design.ai/plugins/design-system-arc/
- Stripe (https://stripe.com). One family (Sohne), light weights, tight
  tracking on large sizes, tabular figures everywhere. Sohne is a
  commercial Klim font, so Seer cannot bundle it. Third party:
  https://styles.refero.design/style/48e5de76-05d5-4c4e-a269-c7c245b291ec

## Common rules

- One line or two line rows. Name at full contrast, everything else
  muted. Time is small, muted, tabular.
- Only the active row has a fill. No borders between rows.
- Two or three weights at most. Names are capitalized.
- Space separates groups, not lines or labels above groups.

## Font candidates (all OFL, all on Google Fonts)

- Inter Variable 4 (https://rsms.me/inter/). Has an opsz axis (Display
  cut), tabular figures, cv11 single storey a. Standard for Linear and
  Raycast. Very legible at 12 to 13 px.
- Geist and Geist Mono (https://github.com/vercel/geist-font). One
  family for UI and terminal. Crisp and modern.
- IBM Plex Sans and IBM Plex Mono. Zed uses Plex Sans in GPUI. More
  character, a bit wider.
- Mona Sans was tested and dropped: wide and quirky at 13 px.

## Decision

- First: Inter for the UI and JetBrains Mono for the terminal. Inter has
  the largest x-height, so 12 px task lines and times read best in close
  crops. JetBrains Mono is the most legible mono at 12.5 px.
- Second: Geist and Geist Mono. One family, crisper, a smaller x-height.
- Third: IBM Plex Sans. Too wide for the 248 px sidebar.
- Dropped: Mona Sans.
- For GPUI, bundle static TTF instances (Inter Text Regular, Medium,
  SemiBold, plus Inter Display for the join heading), because variable
  axis support in GPUI is limited. Details: docs/design/06-window-v2.md.
