# Dark surface depth

T-479. Measured on 2026-10-11 before changing Seer's palette.

## Evidence

Official pages and product images, not third party theme lists. Browser
captures use Chromium, 1440 x 1000, 1x. Values below are the most common
RGB pixel in each crop. Gradients, image compression, and transparency
can change these samples. They are observations, not product tokens.

Crops, full images, coordinates, and pixel counts are outside the repo:
~/Projects/_research/seer-gpui-briefs/artifacts/t479/references/.
measurements.json records each crop. The crop name is the image stem,
role, and -crop.png.

| Product / image stem | Base and layers | Border / tint | What Seer takes |
| --- | --- | --- | --- |
| [Linear](https://linear.app), linear-product | Sidebar #141516, content #171819, selected #1c1d1e | Divider #252627; very slight cool tint, with image noise | Small steps separate work from navigation; only active rows have a fill |
| [Raycast](https://www.raycast.com), raycast-product | Menu #0e0e0e, selected #272727 | Menu edge #282829; window shows the red wallpaper through it | A clear selection step; Seer uses opaque surfaces instead |
| [Warp surface study](https://www.warp.dev/blog/how-we-designed-themes-for-the-terminal-a-peek-into-our-process), warp-surfaces | Terminal #000000, menu and sheet #262626 | Edge #3b3b3a; neutral | Menus are brighter than terminal content; black is a choice, not a rule |
| [Zed](https://zed.dev), zed | Code #16191b, sidebar #202327, title #1b1e21, selected #2c2f35 | Divider pixel #292c32; cool tint | Terminal darker than chrome; no extra card around content |
| [Ghostty](https://ghostty.org), ghostty-site | Site terminal illustration #0f0f11 | Edge #4c4c50; cool tint | The site is not its default theme; see the CLI check below |
| [Arc](https://arc.net), arc-theme-picker | Colored sidebar crop #be6593, selected #fcf2f2, picker #dfdfdf | Gradient and texture, no fixed neutral base | Selection and popup layers differ; Seer keeps its neutral identity |
| [Vercel Geist](https://vercel.com/geist/colors), vercel-dark | Page #000000; gray samples #1a1a1a, #1f1f1f; active navigation #0f0f0f | Divider #2e2e2e; neutral | Role-based surface steps, borders drawn once |

Warp's official [Dark theme](https://github.com/warpdotdev/themes/blob/main/warp_bundled/warp_dark.yaml)
sets background #000000 and foreground #ffffff. Its surface study is from
2021, not a claim about every current Warp theme.

Ghostty's installed default configuration was checked with
/Applications/Ghostty.app/Contents/MacOS/ghostty +show-config --default.
It reports background #282c34 and foreground #ffffff. The complete output
is in references/ghostty-default.txt. Native Ghostty capture was denied by
the computer-use tool. The site crop is labelled separately above.

Arc's [Spaces guide](https://resources.arc.net/hc/en-us/articles/19228064149143-Spaces-Distinct-Browsing-Areas)
describes a theme per Space, with Light, Dark, and Automatic modes on
macOS. Its current official images show colored light themes, not a
single dark default. Dark Arc hex values were not verified. The help
page also returned a browser challenge. We measured the official theme
picker image from arc.net/theme-picker.png instead of inventing values.

## Decision

Use opaque neutral grays. A cool tint would fit the editors, but neutrality
keeps the existing amber state and program-supplied terminal colors clear.
No blur, transparency, light theme, or extra terminal container.

Generate the surface steps in OKLCH with culori, chroma 0, then round to
sRGB hex. Keep the existing semantic token names. SURFACE is the one new
role: navigation and title chrome. WINDOW still owns the terminal base.

| Token | Old | New | OKLCH L target / reason |
| --- | --- | --- | --- |
| WINDOW / bg | #0a0a0a | #1b1b1b | 0.22; darkest large surface, charcoal instead of near black |
| SURFACE / surface | #0a0a0a (WINDOW) | #222222 | 0.25; separates sidebar and title bar from terminal |
| HOVER / hover | #131313 | #292929 | 0.28; quiet instant feedback above chrome |
| RAISED / raised | #171717 | #2b2b2b | 0.29; menus, sheets, tooltips above chrome |
| SELECTED / sel | #1c1c1c | #333333 | 0.32; stronger active row than hover |
| CHIP / chip | #262626 | #383838 | 0.34; menu hover and secondary controls |
| CHIP_2 / chip-2 | #303030 | #404040 | 0.37; avatars and hovered secondary controls |
| LINE / line | #1f1f1f | #3a3a3a | 0.35; structural dividers on brighter chrome |
| LINE_2 / line-2 | #2a2a2a | #505050 | 0.43; input and menu edges remain visible |
| MUTED / muted | #a3a3a3 | #b8b8b8 | Keeps task text below titles without losing contrast |
| FAINT / faint | #8a8a8a | #adadad | Small text passes even on the brightest control fill |
| FAINTER / fainter | #6b6b6b | #939393 | Idle rings and offline dots retain their contrast |

TEXT and ACCENT stay #fafafa, TEXT_2 stays #d4d4d4, and WAITING stays
#fbbf24. The mockup's on-accent button text stays #171717. The Rust
ON_ACCENT constant was removed in T-479 because the tooltip was its only
user. T-456 uses it for text on destructive confirmation buttons. Tooltips
still use RAISED and TEXT_2 instead of accent colors.

Room controls use ONLINE and DANGER for state and destructive actions.
DANGER_HOVER is #393030, a 7% sRGB mix of DANGER over the new RAISED fill.
SHADOW and SCRIM keep the mockup's black overlay values in the palette.

## Contrast

WCAG 2 relative sRGB luminance, computed with culori. Each cell is a
foreground/background ratio. This covers all semantic text on these
fills, including hover states. FAINTER is used for status glyphs; the
mockup title slash uses it on SURFACE (5.18:1).

| Foreground | WINDOW | SURFACE | HOVER | RAISED | SELECTED | CHIP | CHIP_2 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TEXT | 16.50 | 15.24 | 13.94 | 13.57 | 12.10 | 11.23 | 9.93 |
| TEXT_2 | 11.62 | 10.73 | 9.81 | 9.55 | 8.52 | 7.91 | 6.99 |
| MUTED | 8.68 | 8.02 | 7.33 | 7.14 | 6.37 | 5.91 | 5.23 |
| FAINT | 7.68 | 7.09 | 6.48 | 6.31 | 5.63 | 5.23 | 4.62 |
| FAINTER (glyph) | 5.61 | 5.18 | 4.74 | 4.61 | 4.11 | 3.82 | 3.37 |
| WAITING | 10.32 | 9.53 | 8.71 | 8.48 | 7.57 | 7.02 | 6.21 |

Mockup on-accent text on ACCENT stays 17.18:1. Structural line on chrome changes
from 1.20:1 to 1.40:1. Input border on WINDOW changes from 1.38:1 to
2.14:1. Menu border on RAISED changes from 1.25:1 to 1.76:1. The known
sub-3:1 border exception in v2 remains. Idle ring on chrome changes from
3.72:1 to 5.18:1; on selection it changes from 3.20:1 to 4.11:1.

Terminal default text uses TEXT_2 (11.62:1), bold uses TEXT, and inverse
uses WINDOW. Explicit ANSI indices, the 256-color cube, RGB colors,
inverse, hidden, and dim attributes remain program-controlled. They
cannot all meet a text threshold: a program can request black on black.
The palette does not replace those requests.
