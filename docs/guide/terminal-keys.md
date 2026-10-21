# Window Option key

On macOS, Seer reads the Option as Alt setting from the same terminal app
config as its font and colors. It checks Ghostty, kitty, Alacritty, then
iTerm2. The first app with a config wins. Settings from later apps do not
fill gaps. Config paths and include rules are in [terminal font](terminal-font.md).
Restart the window after a config change.

| App | Setting | Both sides | Left only | Right only | Off |
| --- | --- | --- | --- | --- | --- |
| Ghostty | macos-option-as-alt | true | left | right | false |
| kitty | macos_option_as_alt | both | left | right | no |
| Alacritty | window.option_as_alt | Both | OnlyLeft | OnlyRight | None |
| iTerm2 | Option Key Sends / Right Option Key Sends | 2 / 2 | 2 / 0 | 0 / 2 | 0 / 0 |

Alacritty values are TOML strings in the window table. The two iTerm2 keys
are integers in the selected profile. Value 2 means +Esc. A missing right
value uses the left value, as in iTerm2. The old Meta value 1 does not enable
Alt in Seer. It is a separate high-bit encoding, outside this import.
kitty also accepts yes, true, and y for both sides.

On a selected side, Option B sends ESC b and Option Shift B sends ESC B.
The other side keeps native macOS text and composition. With no explicit
setting, an invalid setting, or an unreadable config, Seer keeps native
Option input. Option E followed by E still enters e with acute. Seer does
not infer Ghostty's keyboard-layout default when its setting is absent.

Option Backspace sends ESC DEL, including after Kitty keyboard negotiation.
Other configured Alt keys keep the Alt modifier in Kitty flag 1 sequences.
Configured Option keypad Enter sends ESC followed by native ETX (03) in
legacy mode, and CSI 57414;3u in Kitty flag 1 mode.
This setting belongs to the sender's window. When you type into another
person's terminal with a grant, your setting applies to your input.
Linux, the TUI, custom key bindings, and live reload do not change.

Sources: [Ghostty](https://ghostty.org/docs/config/reference#macos-option-as-alt),
[kitty](https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.macos_option_as_alt),
[Alacritty](https://alacritty.org/config-alacritty.html#window), and
[iTerm2](https://iterm2.com/documentation-preferences-profiles-keys.html).
