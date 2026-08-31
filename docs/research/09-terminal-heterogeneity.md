# Terminal heterogeneity and per-user shell environments

## Scope

This document covers a macOS and Linux server. It does not cover Windows. The
server has several human users. Each user has a private tree of workspaces,
tabs, and panes.

The research separates two terminal interfaces:

- The inner terminal is the virtual terminal that a pane application sees.
  tmux uses a `screen` or `tmux` derivative for this interface, independent of
  the attached client terminal. [tmux manual, `default-terminal`](https://man.openbsd.org/tmux#default-terminal)
- The outer terminal is the terminal used by one attached human client. SSH
  sends its terminal name, character size, pixel size, and terminal modes when
  it requests a PTY. [RFC 4254 section 6.2](https://datatracker.ietf.org/doc/html/rfc4254#section-6.2)

These interfaces must not share one unqualified `TERM` value. A multiplexer is
a terminal emulator between them. GNU Screen also sets an inner `TERM` and
adapts its emulation to the attached physical terminal.
[GNU Screen term documentation](https://www.gnu.org/software/screen/manual/html_node/Term.html)
[GNU Screen termcap documentation](https://www.gnu.org/software/screen/manual/html_node/Termcap.html)

## Decision: server-side PTYs

Choose option A. The server owns every persistent PTY and shell. It launches a
user's panes under that authenticated user's OS identity. The client owns input
capture, display, fonts, clipboard access, and desktop actions. The server owns
the workspace tree, terminal state, and pane processes. This follows the
persistence boundary used by tmux and GNU Screen.
[tmux manual, clients and server](https://man.openbsd.org/tmux#DESCRIPTION)
[GNU Screen detach documentation](https://www.gnu.org/software/screen/manual/html_node/Detach.html)

Here, one logical server is one public broker plus one runtime per active user.
The user runtime owns that user's PTYs and children. The broker owns identity,
routing, supervision, and metadata. This is the process boundary already chosen
by the multi-user research.
`docs/research/08-multiuser-prior-art.md:394-408`
`docs/research/08-multiuser-prior-art.md:484-505`

| Option | PTY and shell location | Disconnect behavior | Heterogeneity boundary | Verdict |
| --- | --- | --- | --- | --- |
| A. Server PTY | The server starts and retains the PTY. SSH sends terminal metadata when it asks the server for a PTY. [RFC 4254 section 6.2](https://datatracker.ietf.org/doc/html/rfc4254#section-6.2) | Pane processes can remain after the display detaches. This is the tmux and Screen model. [tmux manual](https://man.openbsd.org/tmux#DESCRIPTION) [GNU Screen detach](https://www.gnu.org/software/screen/manual/html_node/Detach.html) | The server emulates one stable inner terminal. Each client reports outer capabilities. tmux already separates its inner `default-terminal` from per-client terminal features. [tmux manual](https://man.openbsd.org/tmux#default-terminal) [tmux client flags](https://man.openbsd.org/tmux#CLIENTS) | Recommended. It matches persistent server workspaces and permits one security identity per user. JupyterHub's default local spawner uses local UNIX users for the same identity boundary. [JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html#jupyterhub.spawner.LocalProcessSpawner) |
| B. Client PTY | The human client starts the shell and PTY. | The shell depends on that client unless a second persistence service exists. Server-side tmux avoids this dependency by keeping the session and PTY at the server. [tmux manual](https://man.openbsd.org/tmux#DESCRIPTION) | The local terminal is easy to identify, but the shell no longer runs beside the server checkout and tools. VS Code Remote instead runs its integrated terminal on the remote host. [VS Code Remote SSH](https://code.visualstudio.com/docs/remote/ssh#_open-a-terminal-on-a-remote-host) | Reject for normal panes. It conflicts with server-owned persistent workspaces. [GNU Screen detach](https://www.gnu.org/software/screen/manual/html_node/Detach.html) |
| C. Hybrid PTY | Some panes use a server PTY. Other panes use a client PTY. Mosh is a different hybrid: the server owns the PTY while both ends keep synchronized terminal state. [Mosh technical description](https://mosh.org/#techinfo) | A client-local pane cannot promise the same lifetime as a server pane. Mosh keeps its server-side session but only synchronizes visible terminal state. [Mosh FAQ](https://mosh.org/#faq) | Every pane needs a visible location and lifetime contract. Moving one live shell between local and remote PTYs is not defined by the cited protocols. [RFC 4254 section 6.2](https://datatracker.ietf.org/doc/html/rfc4254#section-6.2) | Do not use as the base architecture. A later explicit, nonpersistent `local pane` type can be separate from server panes. [VS Code Remote SSH](https://code.visualstudio.com/docs/remote/ssh#_open-a-terminal-on-a-remote-host) |

Option A does not copy a shell environment from the client. User A can use fish
and User B can use zsh only when those shells and their configurations exist in
their separate server accounts. VS Code Remote likewise runs the terminal on
the remote host. Coder and Gitpod add explicit dotfile installation because
client dotfiles do not appear in a remote workspace by themselves.
[VS Code Remote SSH](https://code.visualstudio.com/docs/remote/ssh#_open-a-terminal-on-a-remote-host)
[Coder dotfiles](https://coder.com/docs/user-guides/workspace-dotfiles)
[Gitpod Classic dotfiles](https://www.gitpod.io/docs/classic/user/configure/user-settings/dotfiles)

Option B gives each user their client machine's shell, dotfiles, tools, and OS.
It also puts the pane process and its filesystem access on that client. The
server cannot retain that process after the client leaves. The server-side tmux
and Screen models retain processes because their PTYs remain on the server.
[tmux manual](https://man.openbsd.org/tmux#DESCRIPTION)
[GNU Screen detach](https://www.gnu.org/software/screen/manual/html_node/Detach.html)

Option C means two explicit pane classes. A server pane has a server account,
server filesystem, and persistent lifetime. A client pane has a client account,
client filesystem, and connection lifetime. Mosh is not this split. It keeps a
server PTY and synchronizes terminal state to the client.
[Mosh technical description](https://mosh.org/#techinfo)

The server should advertise one inner terminal contract, such as
`herdr-256color`. It must install that terminfo entry on every shell host. A
missing `xterm-kitty` entry can make remote programs report an unknown terminal
and can break keys. Kitty works around this by copying terminfo during SSH
setup. [kitty SSH FAQ](https://sw.kovidgoyal.net/kitty/faq/#i-get-errors-about-the-terminal-being-unknown-or-opening-the-terminal-fails-with-a-message-such-as-xterm-kitty-unknown-terminal-type)
[kitty SSH kitten](https://sw.kovidgoyal.net/kitty/kittens/ssh/)

The inner contract must describe only features that the server can parse and
preserve. tmux requires its inner `TERM` to be a `screen` or `tmux` derivative
and uses a separate per-client feature set. [tmux manual,
`default-terminal`](https://man.openbsd.org/tmux#default-terminal)
[tmux manual, `terminal-features`](https://man.openbsd.org/tmux#terminal-features)

## Client capability record

Do not infer the complete outer feature set from `TERM`. Store a capability
record on each attachment. tmux tracks terminal name, feature flags, terminal
type, and UTF-8 support per client. [tmux client flags](https://man.openbsd.org/tmux#CLIENTS)

The record should contain:

- terminal name and terminfo-derived features;
- color level and a truecolor flag;
- keyboard input protocol and enabled Kitty keyboard flags;
- Kitty graphics, Sixel, and synchronized-output support;
- OSC 52 permission and clipboard direction;
- cell and pixel dimensions;
- mouse coordinate modes;
- Unicode version, width policy, and grapheme policy;
- client render protocol version.

The feature categories above exist in tmux's per-terminal feature model, the
Kitty keyboard and graphics protocols, SSH PTY metadata, and Unicode terminal
text sizing work. [tmux manual, terminal features](https://man.openbsd.org/tmux#terminal-features)
[Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
[Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
[RFC 4254 section 6.2](https://datatracker.ietf.org/doc/html/rfc4254#section-6.2)
[Kitty text sizing protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/)

Capability detection can combine terminfo, explicit client configuration, and
safe terminal queries. `COLORTERM` alone is not reliable through `sudo` or SSH.
The termstandard color proposal recommends terminfo or active queries when
possible. [termstandard color support](https://github.com/termstandard/colors)

Nested terminal paths add another emulator boundary:

- SSH sends `TERM` and dimensions with the server PTY request. The server still
  needs a matching terminfo entry. [RFC 4254 section
  6.2](https://datatracker.ietf.org/doc/html/rfc4254#section-6.2)
- Kitty's SSH helper copies its terminfo and shell integration to avoid the
  missing `xterm-kitty` case. [kitty SSH
  kitten](https://sw.kovidgoyal.net/kitty/kittens/ssh/)
- tmux replaces the inner `TERM` and has explicit `extended-keys` and
  per-client terminal features. A client inside tmux can use only the input and
  output protocols that tmux preserves. [tmux `extended-keys`](https://man.openbsd.org/tmux#extended-keys)
  [tmux terminal features](https://man.openbsd.org/tmux#terminal-features)
- Mosh owns a server PTY and synchronizes terminal state. Its FAQ documents
  `xterm-256color` and `screen-256color` for 256 colors. UNKNOWN: its current
  end-to-end preservation of each newer protocol in this document. Resolve it
  with the same byte and semantic probe suite used for tmux. [Mosh technical
  description](https://mosh.org/#techinfo) [Mosh FAQ](https://mosh.org/#faq)

## Capability matrix

| Capability | Owner | Difference and failure if ignored | Required server or client behavior | Safe fallback |
| --- | --- | --- | --- | --- |
| `TERM` and terminfo | Negotiated | The outer terminal name can be absent from the remote host's terminfo database. Kitty documents this for `xterm-kitty`. An unknown or false entry breaks application startup, keys, cursor movement, or redraw. tmux gives pane applications a different inner terminal name. [kitty SSH FAQ](https://sw.kovidgoyal.net/kitty/faq/#i-get-errors-about-the-terminal-being-unknown-or-opening-the-terminal-fails-with-a-message-such-as-xterm-kitty-unknown-terminal-type) [tmux `default-terminal`](https://man.openbsd.org/tmux#default-terminal) | Give pane applications one installed inner terminfo entry. Keep the outer name and outer feature flags on the client attachment only. tmux follows this split. [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) | Use the documented inner baseline. Do not copy an unknown outer `TERM` into the pane. Kitty's SSH helper shows that copying terminfo is the alternative when the remote shell directly sees `xterm-kitty`. [kitty SSH kitten](https://sw.kovidgoyal.net/kitty/kittens/ssh/) |
| 24-bit color and `COLORTERM` | Negotiated | Terminals commonly use `COLORTERM=truecolor` or `24bit`, but the value can be lost through SSH or `sudo`. If ignored, RGB colors are reduced, wrong, or emitted to a client that cannot render them. [termstandard color support](https://github.com/termstandard/colors) | Accept RGB in the inner contract only if the server render model preserves RGB. Downsample separately for each outer client that lacks truecolor. tmux models RGB as a terminal feature. [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) | Map RGB to the xterm 256-color palette for that client. Do not reduce the server's canonical pane state because another attachment is weaker. [termstandard color support](https://github.com/termstandard/colors) |
| Kitty keyboard protocol and CSI u | Negotiated | Legacy input loses reliable Super and multi-modifier keys, alternate-layout keys, associated text, release and repeat event types, and distinctions such as `Esc` versus an escape-sequence prefix. [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) | The client must send structured key events to the server. The server must encode them for the pane app according to the app's enabled keyboard mode. If the client sends legacy bytes first, lost distinctions cannot be reconstructed. The Kitty specification lists Kitty, Alacritty, foot, Ghostty, iTerm2, Rio, Warp, WezTerm, xterm.js, and TuiOS. [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) | Send the legacy encoding when the pane app has not enabled an extended mode. A terminal that cannot produce a distinct event cannot support that distinct binding. [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) |
| Kitty graphics | Server parse, client render | Outer terminal support varies. Kitty lists implementations in terminals and libraries. The protocol can also use Unicode placeholders so an aware intermediary can retain placement. If ignored, image commands disappear, print control text, or lose image placement. [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) | Parse image commands into server-owned image data and placements. Render a supported image protocol per client. Do not forward arbitrary control strings across users. Kitty's placeholder method is designed to survive an intermediary that handles Unicode text. [Kitty graphics protocol, Unicode placeholders](https://sw.kovidgoyal.net/kitty/graphics-protocol/#unicode-placeholders) | Show a bounded text placeholder with dimensions and image identity. UNKNOWN: the first supported client set and transfer limits. Resolve it with a product decision and memory, bandwidth, and terminal integration tests. [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) |
| Sixel graphics | Server parse, client render | Sixel is a DCS raster format. tmux exposes Sixel as a terminal feature. If ignored, the image is missing or its bytes are misread as terminal controls. [xterm control sequences, Sixel](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Sixel-Graphics) [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) | Parse Sixel into the same image abstraction as Kitty graphics. Re-encode only for clients that report Sixel. tmux shows that a multiplexer must explicitly know this capability. [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) | Use the same text placeholder as Kitty graphics. UNKNOWN: required Sixel fidelity and animation limits. Resolve it with a corpus from target applications and clients. [xterm Sixel documentation](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Sixel-Graphics) |
| Passthrough | Server policy, client output | tmux can pass a DCS payload wrapped with `tmux;` when `allow-passthrough` is enabled. Without parsing or passthrough, an escape protocol terminates at the multiplexer and never reaches the outer client. Zellij's notification work explicitly parses, sanitizes, and re-emits supported sequences instead of adding raw passthrough. [tmux `allow-passthrough`](https://man.openbsd.org/tmux#allow-passthrough) [Zellij PR 5099](https://github.com/zellij-org/zellij/pull/5099) | Prefer parsed semantic messages. If passthrough is ever added, allowlist the protocol, cap payload size, and bind output to the originating pane and authorized attachment. This is stricter than tmux's optional raw DCS path and follows Zellij's parse-and-sanitize direction. [tmux `allow-passthrough`](https://man.openbsd.org/tmux#allow-passthrough) [Zellij PR 5099](https://github.com/zellij-org/zellij/pull/5099) | Drop the unsupported sequence and expose a diagnostic. Do not silently broadcast it to all clients. tmux itself distinguishes visible-only and all-client passthrough. [tmux `allow-passthrough`](https://man.openbsd.org/tmux#allow-passthrough) |
| Synchronized output, DEC mode 2026 | Server parse, client render | `CSI ? 2026 h` begins a buffered update and `CSI ? 2026 l` ends it. Ignoring the mode can expose partial frames and tearing. tmux calls this the `sync` feature and `Sync` terminfo extension. [synchronized output specification](https://gist.github.com/christianparpart/d8a62cc1ab659194337d73e399004036) [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) [tmux extensions](https://man.openbsd.org/tmux#TERMINFO_EXTENSIONS) | Treat mode 2026 as a server-side frame transaction. Send one completed semantic frame. An outer client may also use synchronized update while drawing if it reports support. Kitty uses synchronized updates to avoid partial rendering. [Kitty performance](https://sw.kovidgoyal.net/kitty/performance/) | Apply a time and byte limit, then flush. Render normally on clients without synchronized-output support. [synchronized output specification](https://gist.github.com/christianparpart/d8a62cc1ab659194337d73e399004036) |
| OSC 52 clipboard | Negotiated | OSC 52 carries base64 selection data. Terminal policy can disable reads or writes. Multiplexers must route the request to a client. If ignored, a server-side copy cannot reach the human's local clipboard. If routed to the wrong attachment, it crosses a user boundary. [xterm OSC 52](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Operating-System-Commands) [Kitty clipboard protocol](https://sw.kovidgoyal.net/kitty/clipboard/) | Parse the request. Apply per-user and per-client policy. Send a clipboard action only to the controlling, focused attachment. Never use one user's server clipboard for another user. tmux has `set-clipboard` and per-client clipboard features. [tmux `set-clipboard`](https://man.openbsd.org/tmux#set-clipboard) [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) | Deny with a visible diagnostic or keep data in a private server paste buffer. Zellij documents OSC 52 as the remote-session clipboard path. [Zellij FAQ](https://zellij.dev/documentation/faq.html) |
| OSC 7 and OSC 133 | Server metadata | OSC 7 reports the current directory. OSC 133 marks prompt, command, output, and completion boundaries. If ignored, terminal text still works but command navigation, output selection, and reliable current-directory metadata are lost. Neovim documents both sequences. VS Code recognizes OSC 133 and uses shell integration for command detection. [Neovim terminal documentation](https://github.com/neovim/neovim/blob/master/runtime/doc/terminal.txt) [VS Code shell integration](https://code.visualstudio.com/docs/terminal/shell-integration) | Parse these sequences into pane metadata. Use the metadata for workspace navigation and command status. Re-render only a protocol that a client needs. [VS Code shell integration](https://code.visualstudio.com/docs/terminal/shell-integration) | Keep terminal text and omit enhanced navigation. UNKNOWN: which shells receive automatic integration in version 1. Resolve it with the shell support decision and integration tests. [VS Code shell integration](https://code.visualstudio.com/docs/terminal/shell-integration) |
| Mouse, SGR, and pixels | Negotiated | SGR mouse mode 1006 reports cell coordinates. Mode 1016 reports pixel coordinates. SSH PTY requests include both cell and pixel dimensions. Ignoring the active mode sends wrong coordinates or makes clicks fail. [xterm mouse tracking](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Mouse-Tracking) [RFC 4254 section 6.2](https://datatracker.ietf.org/doc/html/rfc4254#section-6.2) | The client sends structured buttons, modifiers, cell coordinates, and optional pixel coordinates. The server encodes the mode requested by the pane app. Pixel reports require known client cell geometry. UNKNOWN: a current official support matrix for pixel mode across all target clients. Resolve it with mode queries and click probes in Kitty, Ghostty, iTerm2, WezTerm, Alacritty, foot, and xterm.js. [xterm mouse tracking](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Mouse-Tracking) | Use cell coordinates. If the app requested pixels and geometry is unknown, mark pixel mouse unsupported for that attachment. [xterm mouse tracking](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Mouse-Tracking) |
| Bracketed paste | Negotiated | In bracketed paste mode, the terminal surrounds pasted text with CSI 200~ and CSI 201~. If ignored, an app cannot distinguish one paste from typed keys and may apply interactive keybindings to pasted text. [xterm bracketed paste](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Bracketed-Paste-Mode) | The client sends a paste event, not synthetic key events. The server adds brackets only when the pane app enabled the mode. [xterm bracketed paste](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Bracketed-Paste-Mode) | Insert text through the pane's normal input path when bracketed paste is off. Keep paste size limits independent of key handling. [xterm bracketed paste](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Bracketed-Paste-Mode) |
| Fonts, ligatures, undercurl, and box drawing | Client render, server style | Font choice, fallback, ligature shaping, and final glyph pixels are client-only. The app still sends box-drawing characters and underline style. If the client font or style support differs, alignment, boxes, or undercurls look wrong. Kitty selects fonts locally. tmux advertises underline style and color. [Kitty font selection](https://sw.kovidgoyal.net/kitty/kittens/choose-fonts/) [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) | Send cells, style, underline kind, and color. The client chooses glyphs and fonts. Ligatures must not change the server's cell addresses. Box drawing should have a client fallback when the chosen font has gaps. [Kitty font selection](https://sw.kovidgoyal.net/kitty/kittens/choose-fonts/) | Reduce unsupported underline forms to a single underline. Render missing glyphs with the client fallback font. UNKNOWN: the required box-drawing fallback algorithm. Resolve it with visual tests on the supported macOS and Linux clients. [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) |
| Unicode width and graphemes | Server layout, client render | A grapheme cluster can contain several code points. East Asian Width has context-dependent ambiguous characters. Different Unicode tables and segmentation algorithms can produce different cell layouts. If ignored, cursors, selections, borders, and later text move to different columns on different clients. [Unicode UAX 29](https://www.unicode.org/reports/tr29/) [Unicode UAX 11](https://www.unicode.org/reports/tr11/) [Kitty text sizing protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/) | The server is authoritative for grapheme segmentation and cell width. Frames carry explicit cell occupancy. The handshake records the client's Unicode policy for diagnostics, not for changing shared pane state. Kitty's text sizing protocol exists to resolve these disagreements. [Kitty text sizing protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/) | Render the server's cells with replacement glyphs when needed. UNKNOWN: one universal width negotiation supported by all target clients. Resolve it by testing target clients and adopting a fixed server Unicode version plus explicit compatibility cases. [Unicode UAX 29](https://www.unicode.org/reports/tr29/) [Kitty text sizing protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/) |

The Zellij 0.45 release says it supports Kitty graphics in addition to Sixel and
queries the attached terminal for graphics support. This confirms that image
support through a multiplexer needs protocol-specific detection and handling.
[Zellij 0.45 release](https://github.com/zellij-org/zellij/discussions/5499)

## Keyboard ownership and per-user bindings

Input has three owners in this order:

1. The outer terminal or native client owns local shortcuts. A terminal mapping
   can consume a key or unmap it so the program receives it. A consumed key
   never reaches this tool's server. [Kitty key
   mapping](https://sw.kovidgoyal.net/kitty/mapping/)
2. The multiplexer owns its command bindings. tmux uses `C-b` by default and
   permits `bind-key` and `unbind-key`. GNU Screen uses `C-a` by default and
   permits `bind`. Zellij uses configurable modes and keybindings.
   [tmux key bindings](https://man.openbsd.org/tmux#KEY_BINDINGS)
   [GNU Screen bind](https://www.gnu.org/software/screen/manual/html_node/Bind.html)
   [Zellij keybindings](https://zellij.dev/documentation/keybindings.html)
3. The pane application receives any key that the first two layers do not use.
   The Kitty keyboard protocol lets the application request a less ambiguous
   encoding for that final step. [Kitty keyboard
   protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)

This tool's multiplexer bindings are server-side. The client first converts an
outer-terminal event to a structured event. The server then applies that user's
binding profile. Only an unclaimed event is encoded for the pane application.
This order preserves the distinction that the Kitty protocol adds and matches
the server-side binding layer in tmux. [Kitty keyboard
protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
[tmux key bindings](https://man.openbsd.org/tmux#KEY_BINDINGS)

Bindings must be stored per authenticated user. A user's binding profile must
apply to all of that user's attachments. It must not change another user's
profile. Client-local bindings remain local because the terminal may consume a
key before the server sees it. The separation follows tmux's configurable
binding layer and Kitty's local mapping layer. [tmux key
bindings](https://man.openbsd.org/tmux#KEY_BINDINGS)
[Kitty key mapping](https://sw.kovidgoyal.net/kitty/mapping/)

The client should show a conflict diagnostic when a requested multiplexer key
never arrives. UNKNOWN: no portable protocol in the reviewed sources reports
every outer terminal shortcut. Resolve this with a client key tester and setup
documentation for each supported terminal. [Kitty key
mapping](https://sw.kovidgoyal.net/kitty/mapping/)

## Per-user shell environments

Use separate OS users as the version 1 isolation boundary. JupyterHub's
`LocalProcessSpawner` requires local UNIX users and starts each user's server as
that user. Coder instead starts an agent in each remote workspace. Both keep
user processes on the remote execution host.
[JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html#jupyterhub.spawner.LocalProcessSpawner)
[Coder architecture](https://coder.com/docs/admin/infrastructure/architecture)

| Rank | Option | Small self-hosted team cost | Isolation and environment result | Decision |
| --- | --- | --- | --- | --- |
| 1 | Separate OS users | Low to medium. Accounts, homes, groups, and file ownership need administration. JupyterHub uses this model with local UNIX users. [JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html#jupyterhub.spawner.LocalProcessSpawner) | Each shell gets its user's home, login shell, permissions, and dotfiles. LocalProcessSpawner exposes `shell_cmd` for a login shell. [JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html#jupyterhub.spawner.LocalProcessSpawner) | MUST be the default. Start the PTY only after authentication and identity change. Keep each workspace tree owned by that user. [JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html#jupyterhub.spawner.LocalProcessSpawner) |
| 2 | OS users plus per-user Nix profiles | Medium. Nix adds package and garbage-collection operations. A profile is a versioned set of packages and is normally stored per user. [Nix profiles](https://releases.nixos.org/nix/nix-2.34.0/manual/command-ref/new-cli/nix3-profile.html) | Keeps OS identity isolation and adds reproducible per-user tools. It does not replace home and dotfile ownership. [Nix profiles](https://releases.nixos.org/nix/nix-2.34.0/manual/command-ref/new-cli/nix3-profile.html) | SHOULD be an optional toolchain layer. Do not make Nix a requirement for the PTY model. [Nix profiles](https://releases.nixos.org/nix/nix-2.34.0/manual/command-ref/new-cli/nix3-profile.html) |
| 3 | Per-user dotfile directories under one service UID | Medium. Each supported shell needs explicit startup-file routing. | Configuration differs, but processes and ordinary file access still have the same kernel user identity. Linux credentials define file and process permission checks by user and group IDs. [Linux credentials](https://man7.org/linux/man-pages/man7/credentials.7.html) | Reject as a security boundary. It may be a test fixture only. [Linux credentials](https://man7.org/linux/man-pages/man7/credentials.7.html) |
| 4 | Dotfile synchronization | Medium. Coder clones a user's dotfiles into a workspace and runs an install command. Gitpod Classic clones and installs dotfiles in the workspace home. [Coder dotfiles](https://coder.com/docs/user-guides/workspace-dotfiles) [Gitpod Classic dotfiles](https://www.gitpod.io/docs/classic/user/configure/user-settings/dotfiles) | Gives familiar shell configuration, but the fetched setup code runs inside the user's environment. It does not create a separate OS identity. [Coder dotfiles](https://coder.com/docs/user-guides/workspace-dotfiles) | MAY be opt-in after OS-user isolation. Pin the source and show the command before execution. [Coder dotfiles](https://coder.com/docs/user-guides/workspace-dotfiles) |
| 5 | One container per user | High. Images, storage, networking, UID mapping, updates, and runtime policy need administration. Docker notes that user namespace remapping adds bind-mount ownership complexity. [Docker user namespace remapping](https://docs.docker.com/engine/security/userns-remap/) | DockerSpawner starts each user's server in a separate container. Rootless Docker runs the daemon and containers in a user namespace. [DockerSpawner API](https://jupyterhub-dockerspawner.readthedocs.io/en/latest/api/index.html) [Docker rootless mode](https://docs.docker.com/engine/security/rootless/) | MAY be an administrator-selected backend. It is not the small-team default. On macOS, exact runtime and filesystem behavior are UNKNOWN for this project. Resolve them with a chosen container runtime and a macOS prototype. [Docker container security FAQ](https://docs.docker.com/security/faqs/containers/) |
| 6 | Refuse per-user environments and use one service account | Lowest implementation cost. | It gives every shell the same process identity and permission set. Linux permission checks use process credentials. [Linux credentials](https://man7.org/linux/man-pages/man7/credentials.7.html) | Reject. It does not meet the requirement for private user workspace trees. [Linux credentials](https://man7.org/linux/man-pages/man7/credentials.7.html) |

The default must not copy the server daemon's `SHELL`, `HOME`, or startup files
into another user's pane. The shell must come from the target user's account or
that user's explicit configuration. JupyterHub exposes a per-spawner login
shell command, and VS Code Remote runs the terminal on the remote host.
[JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html#jupyterhub.spawner.LocalProcessSpawner)
[VS Code Remote SSH](https://code.visualstudio.com/docs/remote/ssh#_open-a-terminal-on-a-remote-host)

## Prior art matrix

| Tool | PTY and state location | Terminal differences | User environment model | Lesson for this tool |
| --- | --- | --- | --- | --- |
| tmux | One server owns sessions, windows, panes, and PTYs. Clients attach to that server. [tmux manual](https://man.openbsd.org/tmux#DESCRIPTION) | It separates inner `default-terminal` from per-client name and feature flags. It has explicit RGB, Sixel, sync, clipboard, mouse, and passthrough features. [tmux terminal features](https://man.openbsd.org/tmux#terminal-features) [tmux extensions](https://man.openbsd.org/tmux#TERMINFO_EXTENSIONS) | A normal server is tied to its account and socket. The manual supports a socket with different permissions for shared access, but that does not create per-pane OS identities. [tmux `-S`](https://man.openbsd.org/tmux#S) | Copy the inner/outer terminal split. Do not copy its single-account assumption into a multi-user daemon. [tmux manual](https://man.openbsd.org/tmux#DESCRIPTION) |
| Zellij | Zellij is a terminal workspace and multiplexer with configurable sessions and layouts. [Zellij documentation](https://zellij.dev/documentation/) | Current work detects Kitty graphics and Sixel support. Its OSC work parses and sanitizes known sequences rather than offering raw passthrough. [Zellij 0.45 release](https://github.com/zellij-org/zellij/discussions/5499) [Zellij PR 5099](https://github.com/zellij-org/zellij/pull/5099) | Its configuration and keybindings are user files. [Zellij configuration](https://zellij.dev/documentation/configuration.html) [Zellij keybindings](https://zellij.dev/documentation/keybindings.html) | Prefer protocol-aware handling and per-user configuration. The cited docs do not establish a multi-user identity boundary. UNKNOWN: whether another Zellij deployment mode supplies one. Resolve it with an upstream architecture statement. [Zellij documentation](https://zellij.dev/documentation/) |
| GNU Screen | Screen retains programs while a display detaches and later reattaches. [GNU Screen detach](https://www.gnu.org/software/screen/manual/html_node/Detach.html) | It emulates a virtual terminal, adapts termcap on attachment, and can translate keys based on the attached terminal type. [GNU Screen termcap](https://www.gnu.org/software/screen/manual/html_node/Termcap.html) [GNU Screen input translation](https://www.gnu.org/software/screen/manual/screen.html#Input-Translation) | Screen has a multiuser session mode, but its password documentation describes session access in terms of users that can assume the session UID. [GNU Screen detach](https://www.gnu.org/software/screen/manual/html_node/Detach.html) | Reattachment across different terminals is old and real. Modern protocols still need explicit multiplexer support. [GNU Screen termcap](https://www.gnu.org/software/screen/manual/html_node/Termcap.html) |
| Mosh | The server owns the PTY. Client and server each keep screen state and synchronize the latest visible state over the State Synchronization Protocol. [Mosh technical description](https://mosh.org/#techinfo) | Mosh predicts local echo and sends terminal state instead of an SSH byte stream. Its FAQ notes limited scrollback because it synchronizes visible state. [Mosh technical description](https://mosh.org/#techinfo) [Mosh FAQ](https://mosh.org/#faq) | Mosh logs in to a remote account and starts a per-login server. [Mosh usage](https://mosh.org/#usage) | Semantic state synchronization helps poor networks. It does not remove the need for a server PTY or per-user server identity. [Mosh technical description](https://mosh.org/#techinfo) |
| Eternal Terminal | The ET server creates a PTY and the client connects its local terminal to it. [Eternal Terminal design](https://eternalterminal.dev/howitworks/) | ET reconnects the byte stream. Its documented design does not define a broad per-terminal feature negotiation layer. UNKNOWN: current handling of the protocols in this document. Resolve it with source tests against the current release. [Eternal Terminal protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md) | Authentication delegates to SSH setup on the host. [Eternal Terminal setup](https://eternalterminal.dev/howitworks/) | Reconnection alone does not solve terminal heterogeneity. [Eternal Terminal protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md) |
| tmate | The local tmate host is a tmux fork. It replicates state to a remote tmate daemon, where SSH clients attach. [tmate architecture](https://viennot.com/tmate.pdf) | It inherits tmux terminal interpretation and attachment behavior. [tmate architecture](https://viennot.com/tmate.pdf) | It is designed to share one terminal session with invited clients, not to provide each human a private OS environment. [tmate architecture](https://viennot.com/tmate.pdf) | Sharing and multi-user shell isolation are different problems. Keep authorization on every workspace and attachment. [tmate architecture](https://viennot.com/tmate.pdf) |
| VS Code Remote and server | The integrated terminal runs on the remote host. Local VS Code connects to a remote VS Code Server. [VS Code Remote SSH](https://code.visualstudio.com/docs/remote/ssh) | The terminal UI is local, while shell integration uses OSC 133 and related sequences to add command knowledge. [VS Code shell integration](https://code.visualstudio.com/docs/terminal/shell-integration) | Remote extensions, files, terminals, and settings have explicit local and remote scopes. [VS Code Remote SSH](https://code.visualstudio.com/docs/remote/ssh) | Keep desktop actions local and shell execution remote. Treat shell markers as structured metadata. [VS Code shell integration](https://code.visualstudio.com/docs/terminal/shell-integration) |
| Coder | An agent runs inside each remote workspace and starts workspace processes. The web terminal uses xterm.js. [Coder architecture](https://coder.com/docs/admin/infrastructure/architecture) [Coder web terminal](https://coder.com/docs/user-guides/workspace-access) | The browser supplies the terminal renderer. Workspace templates select VM, container, or Kubernetes infrastructure. [Coder templates](https://coder.com/docs/admin/templates) | Templates define infrastructure. Users can install dotfiles into their workspaces. [Coder templates](https://coder.com/docs/admin/templates) [Coder dotfiles](https://coder.com/docs/user-guides/workspace-dotfiles) | A per-workspace agent is a useful process boundary. It costs more than native OS users for one small server. [Coder architecture](https://coder.com/docs/admin/infrastructure/architecture) |
| Gitpod | Gitpod Classic workspaces were isolated, ephemeral development environments. The browser terminal ran a shell in the workspace. [Gitpod Classic introduction](https://www.gitpod.io/docs/classic/user/introduction/gitpod-tutorial/1-start-your-workspace) [Gitpod Classic browser terminal](https://www.gitpod.io/docs/classic/user/references/ides-and-editors/browser-terminal) | The Classic browser terminal had one controlled renderer, while SSH and desktop editor access could add other clients. [Gitpod Classic browser terminal](https://www.gitpod.io/docs/classic/user/references/ides-and-editors/browser-terminal) | Classic workspace images and dotfiles configured the `gitpod` user's environment. Current Gitpod documentation redirects to Ona. Current Ona runners create isolated VMs from Dev Container configuration. [Gitpod Classic images](https://www.gitpod.io/docs/classic/user/configure/workspaces/workspace-image) [Gitpod Classic dotfiles](https://www.gitpod.io/docs/classic/user/configure/user-settings/dotfiles) [Current Ona runners](https://ona.com/docs/ona/runners/overview) | The Classic and current sources describe different product generations. UNKNOWN: current terminal-specific capability handling. Resolve it by testing each Ona environment client. [Current Ona runners](https://ona.com/docs/ona/runners/overview) |
| JupyterHub | A Spawner allocates resources and starts one user server. LocalProcessSpawner starts a local process. DockerSpawner starts a separate container per user. [JupyterHub concepts](https://jupyterhub.readthedocs.io/en/stable/explanation/concepts.html) [DockerSpawner API](https://jupyterhub-dockerspawner.readthedocs.io/en/latest/api/index.html) | JupyterHub delegates terminal rendering and terminal protocol details to the selected single-user application. UNKNOWN: a JupyterHub-wide terminal capability contract is not defined in the cited architecture. Resolve it by selecting and testing that application. [JupyterHub concepts](https://jupyterhub.readthedocs.io/en/stable/explanation/concepts.html) | LocalProcessSpawner requires matching local UNIX users. DockerSpawner offers a container per user. [JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html#jupyterhub.spawner.LocalProcessSpawner) [DockerSpawner API](https://jupyterhub-dockerspawner.readthedocs.io/en/latest/api/index.html) | Make process identity and terminal rendering separate, replaceable layers. [JupyterHub concepts](https://jupyterhub.readthedocs.io/en/stable/explanation/concepts.html) |

## Local reference clones

Herdr and Luvus are single-machine, same-account tools in the inspected clones.
They do not face this ticket's shared-server user isolation problem.

- Herdr fixes the inner pane environment to `TERM=xterm-256color` and
  `COLORTERM=truecolor`. The comment says its own terminal layer, not the outer
  terminal, is the pane interface.
  `/home/nethum/Projects/_research/herdr/src/pane.rs:55-80`
- Herdr's handshake carries dimensions, pixel cell size, render encoding,
  keybindings, and launch mode. It does not carry the broader terminal feature
  record proposed here.
  `/home/nethum/Projects/_research/herdr/src/protocol/wire.rs:341-362`
- Herdr restricts its server socket to owner read and write mode `0600`. This is
  a same-account boundary, not a shared service boundary.
  `/home/nethum/Projects/_research/herdr/src/server/socket_paths.rs:11-12`
  `/home/nethum/Projects/_research/herdr/src/server/socket_paths.rs:72-75`
- Herdr selects a configured shell or its process `SHELL`. That is sufficient
  for a same-account tool but not for several authenticated OS users.
  `/home/nethum/Projects/_research/herdr/src/pane.rs:1359-1367`
  `/home/nethum/Projects/_research/herdr/src/pane.rs:1505-1524`
- Luvus describes its transport as local IPC and rejects a socket or peer that
  is not owned by the current effective user on Linux and macOS.
  `/home/nethum/Projects/_research/luvus/src/ipc/transport.rs:1-5`
  `/home/nethum/Projects/_research/luvus/src/ipc/transport.rs:260-315`
  `/home/nethum/Projects/_research/luvus/src/ipc/transport.rs:326-345`
- Luvus says connection to its owner-only socket is full command execution as
  the user.
  `/home/nethum/Projects/_research/luvus/src/ipc/transport.rs:650-657`
- Luvus selects one inherited or configured shell. POSIX shells then load that
  account's interactive startup files.
  `/home/nethum/Projects/_research/luvus/src/platform.rs:85-127`
- Luvus's hello message has protocol version, columns, and rows. Clipboard and
  URL actions are client-side. Truecolor detection uses the client's
  `COLORTERM` and falls back to 256 colors.
  `/home/nethum/Projects/_research/luvus/src/ipc/protocol.rs:19-35`
  `/home/nethum/Projects/_research/luvus/src/ipc/protocol.rs:49-59`
  `/home/nethum/Projects/_research/luvus/src/ipc/protocol.rs:372-381`

The useful local precedent is the split between a fixed inner terminal and
client-local rendering or desktop actions. The missing pieces are authenticated
multi-user identity, per-client protocol capabilities, and server-owned
per-user environments. The cited local lines show those exact boundaries.

## Remaining unknowns

- UNKNOWN: the exact terminal client support list for version 1. Resolve it by
  naming supported clients and running a protocol probe suite on every release.
  The Kitty protocol and tmux feature list provide the initial probe set.
  [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
  [tmux terminal features](https://man.openbsd.org/tmux#terminal-features)
- UNKNOWN: the maximum image memory, transfer size, and retention policy.
  Resolve it with explicit resource budgets and adversarial Kitty and Sixel
  corpus tests. Both protocols can carry raster data.
  [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
  [xterm Sixel documentation](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Sixel-Graphics)
- UNKNOWN: the canonical Unicode version and compatibility table. Resolve it
  before the wire format is stable. Pin one UAX 29 implementation and test it
  against each client renderer. [Unicode UAX 29](https://www.unicode.org/reports/tr29/)
- UNKNOWN: whether macOS server deployments will support containers. Resolve it
  by selecting a macOS runtime and measuring file ownership, startup time, CPU,
  memory, and volume behavior. Docker Desktop uses a Linux VM and explicit file
  sharing for containers. [Docker container security FAQ](https://docs.docker.com/security/faqs/containers/)
- UNKNOWN: how simultaneous attachments of different sizes select the pane's
  canonical rows and columns. Resolve it as a product rule and test resize
  races. tmux exposes window-size policies because attached clients can differ.
  [tmux `window-size`](https://man.openbsd.org/tmux#window-size)

## Design implications for our tool

### MUST

- MUST keep normal pane PTYs and processes on the server so they survive client
  disconnects. [tmux manual](https://man.openbsd.org/tmux#DESCRIPTION)
- MUST authenticate the human before workspace lookup. MUST launch every pane
  with that user's OS identity, home, shell, groups, and file permissions.
  [JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html#jupyterhub.spawner.LocalProcessSpawner)
- MUST keep one fixed, installed inner terminfo contract. MUST NOT inherit an
  arbitrary outer `TERM` into the pane. [tmux `default-terminal`](https://man.openbsd.org/tmux#default-terminal)
  [kitty SSH FAQ](https://sw.kovidgoyal.net/kitty/faq/#i-get-errors-about-the-terminal-being-unknown-or-opening-the-terminal-fails-with-a-message-such-as-xterm-kitty-unknown-terminal-type)
- MUST negotiate and store terminal capabilities per attachment. MUST preserve
  stronger canonical pane state when a weaker client attaches. tmux stores
  terminal identity and features per client. [tmux client
  flags](https://man.openbsd.org/tmux#CLIENTS)
- MUST transport key, mouse, resize, and paste as structured events. MUST encode
  pane input only after multiplexer bindings are resolved. [Kitty keyboard
  protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
  [xterm mouse tracking](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Mouse-Tracking)
- MUST parse and authorize clipboard, image, working-directory, prompt, and
  passthrough controls. MUST NOT broadcast raw controls across users.
  [tmux `allow-passthrough`](https://man.openbsd.org/tmux#allow-passthrough)
  [Zellij PR 5099](https://github.com/zellij-org/zellij/pull/5099)
- MUST make grapheme segmentation and cell width server-authoritative. Frames
  must carry explicit cell occupancy. [Unicode UAX
  29](https://www.unicode.org/reports/tr29/)
  [Kitty text sizing protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/)
- MUST store workspaces, tabs, panes, and multiplexer keybindings under the
  authenticated user's namespace. tmux, Screen, and Zellij all expose user
  binding configuration. [tmux key bindings](https://man.openbsd.org/tmux#KEY_BINDINGS)
  [GNU Screen bind](https://www.gnu.org/software/screen/manual/html_node/Bind.html)
  [Zellij keybindings](https://zellij.dev/documentation/keybindings.html)

### SHOULD

- SHOULD send semantic cell frames and separate bounded image objects. SHOULD
  render RGB, underline style, and images according to each attachment's
  capabilities. [tmux terminal features](https://man.openbsd.org/tmux#terminal-features)
  [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
- SHOULD treat synchronized output as an atomic frame with time and byte caps.
  [Kitty performance](https://sw.kovidgoyal.net/kitty/performance/)
- SHOULD route OSC 52 only to the focused controlling attachment and require an
  explicit user policy for clipboard reads. [xterm OSC
  52](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Operating-System-Commands)
- SHOULD expose a capability diagnostic for missing terminfo, lost key
  distinctions, unsupported graphics, pixel mouse, and Unicode disagreement.
  [kitty SSH FAQ](https://sw.kovidgoyal.net/kitty/faq/#i-get-errors-about-the-terminal-being-unknown-or-opening-the-terminal-fails-with-a-message-such-as-xterm-kitty-unknown-terminal-type)
  [Kitty text sizing protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/)
- SHOULD allow optional per-user Nix profiles without making Nix part of the PTY
  security boundary. [Nix profiles](https://releases.nixos.org/nix/nix-2.34.0/manual/command-ref/new-cli/nix3-profile.html)

### MAY

- MAY add opt-in dotfile synchronization after OS-user isolation. [Coder
  dotfiles](https://coder.com/docs/user-guides/workspace-dotfiles)
- MAY add one container per user as an administrator-selected backend. It must
  preserve user ownership and must not expose a privileged container runtime to
  pane users. [Docker rootless mode](https://docs.docker.com/engine/security/rootless/)
- MAY add an explicit nonpersistent client-local pane type. It must have a
  different location and lifetime label from a normal server pane. VS Code
  Remote makes the remote execution location explicit. [VS Code Remote
  SSH](https://code.visualstudio.com/docs/remote/ssh)
- MAY support allowlisted passthrough only after parsed protocols are
  insufficient. The default remains disabled. [tmux
  `allow-passthrough`](https://man.openbsd.org/tmux#allow-passthrough)
