# Window terminal font

Seer reads terminal app config files when the window starts. It checks
Ghostty, kitty, Alacritty, then iTerm2. The first app with a config wins.
It does not mix settings from different apps. Restart Seer after a change.
The font belongs to the viewer. A remote terminal uses your font too.
The TUI does not change.

If the selected config cannot be read, has no font settings, or names a font
that is not installed, Seer uses JetBrains Mono at 12.5 with a 19 logical pixel row.
A missing size or family keeps the Seer default for that field. Invalid sizes
also use the Seer default. Sizes must be finite, above 0, and at most 256 points.
Colors, font styles, and live reload are not imported.

Config locations and syntax:

- Ghostty: $XDG_CONFIG_HOME/ghostty/config.ghostty and ghostty/config.
  XDG_CONFIG_HOME defaults to $HOME/.config. On macOS, files in
  $HOME/Library/Application Support/com.mitchellh.ghostty load after the XDG
  files. Seer reads font-family and font-size in key = value syntax.
  Relative config-file includes load after their parent file.
  [Ghostty config](https://ghostty.org/docs/config) and
  [font options](https://ghostty.org/docs/config/reference#font-family).
- kitty: $KITTY_CONFIG_DIRECTORY/kitty.conf, when set. Otherwise, the first
  kitty/kitty.conf in $XDG_CONFIG_HOME, $HOME/.config, or $XDG_CONFIG_DIRS.
  The last variable defaults to /etc/xdg. Seer reads font_family and font_size
  in key value syntax. Family names and the family, postscript_name, full_name,
  and system font selectors are accepted. A following line with a leading
  backslash continues the current line. File includes load in place. A missing
  or unreadable include is skipped; the parent settings stay. An unreadable
  main config still uses the Seer defaults.
  [kitty paths](https://sw.kovidgoyal.net/kitty/invocation/#cmdoption-kitty-config),
  [config syntax](https://sw.kovidgoyal.net/kitty/conf/), and
  [font selectors](https://sw.kovidgoyal.net/kitty/kittens/choose-fonts/).
- Alacritty: the first of $XDG_CONFIG_HOME/alacritty/alacritty.toml,
  $XDG_CONFIG_HOME/alacritty.toml, $HOME/.config/alacritty/alacritty.toml,
  $HOME/.alacritty.toml, or /etc/alacritty/alacritty.toml. Seer reads the TOML
  font.normal.family and font.size fields. Imports load before the parent.
  [Alacritty format and paths](https://alacritty.org/config-alacritty.html).
- iTerm2, macOS only: $HOME/Library/Preferences/com.googlecode.iterm2.plist.
  Seer reads XML and binary property lists. It selects the profile whose
  Guid matches Default Bookmark Guid, or the first profile if no default
  is set. Normal Font contains the font face name and point size.
  [iTerm2 path](https://iterm2.com/faq.html),
  [profile settings](https://iterm2.com/documentation-preferences-profiles-text.html),
  [stored keys](https://github.com/gnachman/iTerm2/blob/master/sources/Settings/Profiles/ITAddressBookMgr.h).

Seer does not run kitty envinclude or geninclude commands. App command line
flags, live window zoom, and non-default iTerm2 preferences locations are not
read. Include chains are limited to 16 files deep. These rules keep startup
bounded and leave each terminal app in control of its own config.
Includes use literal file paths. Globs and environment variable expansion are
not supported. Seer does not merge kitty system defaults into a user config.
Font sizes use terminal app points: one logical pixel per point
on macOS, and 96/72 logical pixels per point on Linux.
