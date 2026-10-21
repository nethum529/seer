# Window terminal colors

Seer imports the viewer's 16 ANSI colors, foreground, background, and cursor
color when the window starts. It uses the same app, files, and includes as
[the font import](terminal-font.md): Ghostty, kitty, Alacritty, then iTerm2.
The first app with a config wins, even when it sets only fonts or only colors.
Seer does not fill missing colors from another app.

A missing or invalid color keeps that Seer default. If the selected config
cannot be read or parsed, all colors keep the Seer defaults. A remote terminal
uses the viewer's palette. Explicit RGB output and the standard color cube
and gray ramp at indices 16 to 255 keep their existing behavior. The TUI does
not change. Restart the window after a config change.

| App | Imported fields | Format |
| --- | --- | --- |
| Ghostty | palette = 0 through 15, foreground, background, cursor-color | key = value; palette values are index=color |
| kitty | color0 through color15, foreground, background, cursor | key value |
| Alacritty | colors.normal and colors.bright (black, red, green, yellow, blue, magenta, cyan, white), colors.primary.foreground, colors.primary.background, colors.cursor.cursor | TOML tables with RGB hex strings |
| iTerm2 | Ansi 0 Color through Ansi 15 Color, Foreground Color, Background Color, Cursor Color | RGB component dictionaries in the same default profile as Normal Font |

Ghostty and kitty accept RGB hex values, X11 color names, and rgb:R/G/B
components with one to four hex digits per channel. Ghostty also accepts
hex without a prefix. Ghostty palette indices accept decimal, 0x, 0o, and 0b.
Alacritty accepts #RRGGBB and 0xRRGGBB. iTerm2 accepts XML and binary plist
files. Its sRGB, P3, and legacy Calibrated colors convert to sRGB for drawing.

In solid mode, the imported background fills the terminal, including its
padding. In glass mode, it tints the existing picture with the saved Glass
darkness value as its opacity. Picture choice, brightness, blur, and uniform
glass keep their existing settings. An unset background keeps Seer's current
solid and glass defaults. Chrome keeps its own palette.

Seer does not resolve named themes, appearance-specific theme variants,
dynamic cell-relative cursor values, or terminal app opacity settings. It
imports the explicit color fields above. No theme picker or live reload is
added. Include limits and unsupported include forms are the same as the font
import.

Sources:

- [Ghostty color options](https://ghostty.org/docs/config/reference#palette).
- [kitty colors](https://sw.kovidgoyal.net/kitty/conf/#color-scheme).
- [Alacritty colors](https://alacritty.org/config-alacritty.html#colors).
- [iTerm2 profile colors](https://iterm2.com/documentation-preferences-profiles-colors.html)
  and [stored color format](https://github.com/gnachman/iTerm2/blob/master/sources/Categories/NSDictionary%2BiTerm.m).
- [X11 color names](https://gitlab.freedesktop.org/xorg/app/rgb).
  The bundled table retains its source license.
