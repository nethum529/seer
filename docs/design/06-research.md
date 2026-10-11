# Design v2 research (WIP)

Status: research partly done. No mockup yet. Screenshots are not in the
repo.

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

Not decided yet. Leaning to Inter (with cv11, Display cut for titles) or
Geist plus Geist Mono. For GPUI, bundle static TTF instances (Regular,
Medium, Semibold), because variable axis support in GPUI is limited.
