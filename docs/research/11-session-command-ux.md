# Session command UX

Research date: 2026-09-01.

## Scope

This note compares the command surface for tmux, GNU Screen, Zellij, tmate,
and Mosh. It covers create, name, list, attach, detach, kill, and share. The
friction notes are a command-surface assessment. They are not the result of a
new user study.

This note does not repeat the process, identity, isolation, or persistence
research in `docs/research/08-multiuser-prior-art.md`. It accepts the settled
model: one public broker, one runtime per human user, and one private workspace
tree per user. It focuses only on how a human enters and leaves that model.

`seer` is the product command.

## Short answer

Use seven plain verbs:

```text
seer start
seer invite
seer join
seer list
seer attach
seer detach
seer peek <person>
```

`start` creates or starts the one server. `invite` creates one unused seat.
`join` claims that seat, asks for the person's name, saves a device credential,
and performs the first attach. `attach` is only for a person who already joined.
`peek` is always read-only. No common command needs a flag, a socket path, a
session name, or a token on the command line.

Copy tmate's ready-to-send invitation and separate read-only authority. Copy
Zellij's plain words, useful behavior with no arguments, visible session
manager, and explicit attach-or-create behavior. Do not copy tmux's target and
socket flags, Screen's combinations of single-letter flags, or Mosh's lack of
a named reattach point.

## Command surface comparison

### tmux

| Action | Common command |
| --- | --- |
| Create | `tmux new` creates and attaches. `tmux new -d` creates in the background. |
| Name | `tmux new -s work` names a new session. `tmux rename-session -t old new` renames one. |
| List | `tmux ls` or `tmux list-sessions`. |
| Attach | `tmux attach -t work`. `tmux new -As work` means attach if present, otherwise create. |
| Detach | `C-b d` in a client, or `tmux detach-client`. |
| Kill | `tmux kill-session -t work`. `tmux kill-server` destroys every session on that socket. |
| Share | There is no single share command. The owner grants a local user with `tmux server-access -a user`, can make that user read-only with `tmux server-access -r user`, and must also change socket path permissions. The guest must select the same socket with `-S <path>` or the same socket name with `-L <name>`. |

The tmux manual defines separate commands for create, attach, list, detach, and
kill. It also states that `-A` changes `new-session` into attach-or-create, `-r`
makes an attached client read-only, and `-S` selects a full socket path
([tmux clients and sessions](https://man.openbsd.org/tmux#CLIENTS_AND_SESSIONS),
[tmux options](https://man.openbsd.org/tmux#S)). The access list and filesystem
permissions are separate. The manual warns that even read-only socket access
must not be given to an untrusted user
([tmux server-access](https://man.openbsd.org/tmux#server-access)).

Common friction:

- The user must learn commands and target flags together. `-s` sets a new
  session name, while `-t` selects an existing target.
- `tmux` with no command creates a session. `tmux attach` only attaches.
  Attach-or-create is a third form, `tmux new -As name`.
- A duplicate name makes `new -s name` fail. The recovery action depends on
  whether the user meant create, attach, or replace.
- `-L` names a server socket, `-S` gives a socket path, and `-s` names a
  session. These three namespaces are easy to confuse.
- `attach -d` detaches other clients before attaching the new client. It is
  easy to read `-d` as "start detached", which is its meaning on `new`.
- Sharing needs an application ACL and filesystem permission work. The guest
  must also know the socket selector. There is no invitation object.
- `kill-session` and `kill-server` are close in spelling but have very
  different scope.

tmux is consistent after a user learns its command language. It is not a good
model for a first-use invitation flow.

### GNU Screen

| Action | Common command |
| --- | --- |
| Create | `screen` creates and attaches. `screen -d -m` creates in the background. |
| Name | `screen -S work` gives a new session a name. |
| List | `screen -ls` or `screen -list`. |
| Attach | `screen -r work` resumes a detached session. `screen -d -r work` first detaches its other display. `screen -x work` adds another display. |
| Detach | `C-a d` in a display, or `screen -d work` from outside. |
| Kill | `screen -S work -X quit`, or the interactive `quit` command. |
| Share | The owner enables `multiuser on` and grants a user with `acladd user`. The guest uses an owner-qualified target such as `screen -x owner/work`. Cross-user attach needs a Screen build with multiuser support and setuid-root. |

The Screen invocation manual documents `-r`, `-R`, `-RR`, `-d -r`, `-D -R`,
`-ls`, `-S`, and `-x`. It also documents owner-qualified multiuser targets
([Screen invocation](https://www.gnu.org/software/screen/manual/screen.html#Invoking-Screen),
[Screen multiuser mode](https://www.gnu.org/software/screen/manual/screen.html#Multiuser-Session)).

Common friction:

- Small changes in capitalization and repetition change lifecycle behavior.
  `-r`, `-R`, `-RR`, `-d -r`, and `-D -RR` are different operations.
- `-r` refuses an attached session. The user must decide whether to detach the
  old display with `-d -r` or add a display with `-x`.
- Session identifiers include a process ID and a name. More than one session
  can have the same name suffix, so a short match can become ambiguous.
- `-S` names a new session in one context and helps select an owner-qualified
  multiuser session in another context.
- Killing from outside needs the general remote-command form `-X quit`. There
  is no direct `kill-session` verb.
- Sharing mixes interactive Screen commands, Unix user names, ACL commands,
  owner-qualified session names, and a privileged installation requirement.

Screen gives an expert many recovery choices. The flag combinations make the
normal create-or-attach decision hard to predict.

### Zellij

| Action | Common command |
| --- | --- |
| Create | `zellij` creates and attaches with a generated random name. `zellij -s work` creates a named session. `zellij attach --create-background work` creates without attaching. |
| Name | `zellij -s work`, or `zellij action rename-session work` from a session. |
| List | `zellij list-sessions` or `zellij ls`. |
| Attach | `zellij attach work` or `zellij a work`. With no name, it attaches when only one live session exists. `zellij attach -c work` is attach-or-create. |
| Detach | The configured Detach action, normally available from the TUI, or `zellij action detach`. |
| Kill | `zellij kill-sessions work` or `zellij k work`. `zellij kill-all-sessions` asks for confirmation. |
| Share | `Ctrl-o s` opens the built-in share UI. The CLI path starts `zellij web`, creates a token with `zellij web --create-token`, and can create view-only access with `zellij web --create-read-only-token`. A terminal guest uses `zellij attach <https-url> --token <token>`. |

Zellij documents full subcommands and short aliases. It makes a no-name attach
work when there is only one running session. Its attach command has explicit
create and create-in-background options
([Zellij commands](https://zellij.dev/documentation/commands.html),
[Zellij CLI recipes](https://zellij.dev/documentation/cli-recipes.html)). Its
session manager can list, create, rename, switch, resurrect, kill, delete, and
share sessions in one visible UI
([Zellij session manager](https://zellij.dev/documentation/session-manager-alias.html)).

Common friction:

- `zellij -s work` creates, while `zellij attach work` attaches. A collision
  requires the user to change to attach or to use attach-or-create.
- The attach-or-create option is under `attach` and is still a flag. A user who
  starts from `zellij -s work` does not discover it from the same command form.
- A generated name removes the first naming decision but is easy to forget.
  The user then needs `zellij ls` or the session manager.
- Live, exited, and resurrectable sessions can have the same visible name and
  different lifecycle states. The UI must explain which action will occur.
- Web sharing has a friendly in-app entry point, but remote setup still needs
  HTTPS, a web listener, a login token, and session sharing policy. A CLI guest
  must pass the token unless it was remembered
  ([Zellij web client](https://zellij.dev/documentation/web-client.html)).
- Zellij's modal keybindings are visible and configurable, but the modes add a
  separate learning task. The first run asks the user to choose a keybinding
  preset
  ([Zellij keybinding presets](https://zellij.dev/documentation/keybinding-presets.html)).

Zellij feels friendlier than tmux for these reasons:

- A bare `zellij` works and supplies a name. A user does not need to understand
  a server socket before the first session.
- Full command names such as `attach`, `list-sessions`, and `kill-sessions`
  state the action. Short aliases remain optional.
- Attach without a name has a useful unambiguous default.
- The status bar, welcome screen, and session manager make state and available
  actions visible. tmux expects more recall of a prefix language and target
  syntax.
- The share UI groups web-server state, tokens, read-only access, and current
  session sharing. tmux exposes these as socket and access-list work.
- Destructive all-session kill asks for confirmation.

### tmate

| Action | Common command |
| --- | --- |
| Create | `tmate` creates a local tmux-derived session, opens an outbound relay connection, and displays guest connection strings. |
| Name | Hosted named sessions use `tmate -k <api-key> -n <write-name> -r <read-name>`. Ordinary sessions use generated capability tokens. |
| List | There is no simple global list of all local tmate sessions. tmux-compatible list commands operate only after the caller selects a particular tmate socket. |
| Attach | A local connection string has the form `tmate -S <generated-socket-path> attach`. A guest runs the displayed `ssh <token>@<relay>` command or opens the displayed web URL. |
| Detach | `C-b d` uses the inherited tmux behavior. A local command must select the correct tmate socket. |
| Kill | The inherited `kill-session` command works against the selected tmate socket. Closing the host session ends guest access. |
| Share | Copy one displayed SSH command or web URL. tmate prints separate read-write and read-only forms. |

tmate describes itself as a tmux fork for instant pairing
([tmate repository](https://github.com/tmate-io/tmate)). Its 2.4 release added
named sessions, authorized-key control, and foreground mode. The named form
requires an API key and separate `-n` and `-r` values
([tmate 2.4 release](https://github.com/tmate-io/tmate/releases/tag/2.4.0)).
The relay sends the host separate SSH and web strings for read-write and
read-only access
([tmate relay source](https://github.com/tmate-io/tmate-websocket/blob/master/lib/tmate/session.ex)).

What made tmate sharing feel easy:

- The host runs one command.
- The connection is outbound. The host does not first configure an inbound
  socket, firewall rule, local Unix user, or port forward.
- The result is a complete command or URL that is ready to send to another
  person.
- Possession of the connection string supplies the guest authority. There is
  no separate account creation flow.
- Read-write and read-only access are separate outputs with clear labels.
- The guest's first action also attaches. There is no register-then-attach
  sequence.

Common friction:

- The easy token is a bearer credential. Sending it to the wrong place grants
  its authority until the session ends or the token is revoked by ending it.
- Users must trust the relay or operate their own relay.
- The local attach command exposes a generated socket path. More than one host
  session is hard to discover and select without saving its output.
- Stable names make the simple path less simple because they need an API key
  and two flags.
- The tmux command and prefix model remains after attach.
- A relay token shares one host session. It does not create a durable human
  identity or a private tree for each guest.

### Mosh

| Action | Common command |
| --- | --- |
| Create | `mosh user@host` authenticates through SSH, starts one remote `mosh-server`, and opens one shell. |
| Name | Not supported. A Mosh connection has no user-facing persistent session name. |
| List | Not supported. There is no Mosh session registry command. |
| Attach | The original running client reconnects after network loss or an address change. A new `mosh` command starts a new remote shell; it does not attach by name to the old one. |
| Detach | There is no multiplexer detach that leaves a named session for a new client. Temporary transport loss keeps the same client and server pair alive. |
| Kill | Exit the remote shell or terminate the Mosh client and server. There is no named kill command. |
| Share | Not supported. Mosh is one authenticated client and one remote shell, not a shared multiplexer. Run tmux, Screen, or another multiplexer inside it when named detach and reattach are required. |

The Mosh usage surface is deliberately one command. SSH starts the remote
server, then the client and server synchronize terminal state over UDP. The
same client can roam and survive intermittent connectivity
([Mosh README](https://github.com/mobile-shell/mosh#usage),
[Mosh technical description](https://mosh.org/#techinfo)).

Common friction:

- "Reconnect" means that the original client process recovers its transport.
  It does not mean that a later client can find and attach to a named session.
- There is no create-versus-attach choice because every new command creates a
  new connection. Users who expect multiplexer persistence can lose the old
  shell when they close the client.
- There is no list, name, kill-by-name, multi-client attach, or share surface.
- The simple command hides an operational requirement: UDP ports, normally in
  the 60000 to 61000 range, must reach the server
  ([Mosh README](https://github.com/mobile-shell/mosh#how-it-works)).

Mosh is good prior art for automatic reconnect. It is not command-surface prior
art for persistent shared sessions.

## Lessons from the Herdr and Luvus clones

The inspected Herdr clone is commit
`2290257acb2085ce6842ba5c7e3ca50c3ba64f02`. The inspected Luvus clone is
commit `d1013d16f48cdd724b8df40c7c4c83dc306dc5d6`. `H:` and `L:` below refer to
their read-only roots at `/home/nethum/Projects/_research/herdr` and
`/home/nethum/Projects/_research/luvus`.

Both tools expose the same clear lifecycle family:

```text
herdr session list [--json]
herdr session attach <name>
herdr session stop <name> [--json]
herdr session delete <name> [--json]

luvus session list [--json]
luvus session attach <name>
luvus session stop <name> [--json]
luvus session delete <name> [--json]
```

Herdr defines these verbs directly in `H:src/cli.rs:418-525`. Luvus prints the
four-command family as focused help in `L:src/cli.rs:918-926`. In both tools,
`session attach <name>` is processed before normal command routing and becomes
the ordinary start-or-attach path (`H:src/session.rs:29-51`,
`L:src/session.rs:40-64`).

Useful lessons:

- Plain `list`, `attach`, `stop`, and `delete` verbs are easier to explain than
  one-letter flag combinations.
- Attach-or-create is a good default when the named object belongs only to the
  caller. It avoids a separate create command.
- `stop` and `delete` are correctly separate. A destructive disk action must
  not hide behind detach or stop.
- Machine output is optional. `--json` does not burden the human path.
- Herdr distinguishes terminal `control` from `observe`. That supports a clear
  product distinction between `attach` and `peek`, but the four-word Herdr
  form `terminal session observe <target>` is too long for the common human
  path (`H:src/cli.rs:43-45`, `H:src/cli/spec.rs:708-735`).

Parts not to copy:

- A Herdr or Luvus named session is an independent same-account server
  namespace. It is not one human user inside the shared broker. The existing
  lifecycle research already establishes this distinction
  (`docs/research/06-herdr-server-and-persistence.md:52-68`,
  `docs/research/07-luvus-server-and-persistence.md:77-99`).
- A second `--session <name>` selection form duplicates the attach surface and
  makes option position important.
- Socket paths and session directories are useful diagnostic output. They
  must not be required user input.
- Attach-or-create is unsafe for another person's name. `peek alice` must
  never create an `alice` user or silently change to a writable attachment.

## Proposed minimal command set

### Vocabulary

Use these terms consistently:

- A server is the one installed broker and its supervised user runtimes.
- A seat is permission for one new human to join that server.
- A person has one stable internal ID and one unique display name on a server.
- An attachment is one client connected to that person's own runtime.
- A peek is a read-only attachment to another person's workspace tree.
- Detach ends one client attachment. It does not stop a runtime or a pane.

Do not call a person, runtime, workspace tree, and client connection a
"session". That overloaded word causes much of the surveyed confusion.

### Commands

| Command | Common-path contract |
| --- | --- |
| `seer start` | On the Linux host, initialize the one server if absent and start it as a background service. If it is already running, report that state and succeed. Never create a second unnamed server. On first use, ask for the owner's display name and the published address if they are not configured. |
| `seer invite` | Create one single-use seat token for the current server. Print one copyable invitation capsule that contains the server endpoint, server identity fingerprint, and high-entropy token. Default expiry is one hour. Do not attach the token to a person name. |
| `seer join` | Ask for the invitation capsule with hidden input. Verify the server identity, claim the unused seat, then ask for the person's display name. Save a new device credential with owner-only permissions. Invalidate the seat token and perform the first attach. |
| `seer list` | Show saved servers, the caller's identity on each, server reachability, attachment state, and people available to peek. Never print credentials or invitation tokens. With one current server, show it first. |
| `seer attach` | Attach to the caller's own tree on the current server. With one saved server, select it. With several and no current server, open a picker. Never create a person, seat, or server. Never detach another client. |
| `seer detach` | Detach only the calling client. From the TUI command line, the caller is exact. From a separate shell, detach the caller's only active client or show a client picker. Keep the runtime, PTYs, and pane processes alive. |
| `seer peek <person>` | Open the named person's tree in read-only mode. Default to that person's active workspace and show a picker when no workspace is active. Show a permanent `PEEK: <person> - READ ONLY` banner. Never forward input or resize that person's PTYs. |

The commands have no required flags on the common path. A positional person
name in `peek` is the object of the action, not an option. Advanced automation
can add optional output and selection flags later without changing these seven
forms.

### Safe defaults and error behavior

- `start` is idempotent. A second call cannot create a name collision or a
  second broker.
- The server has a generated stable ID. A client can give it a local alias,
  but the alias is not its security identity.
- A seat token works once and expires after one hour by default. The server
  stores only a verifier. A failed name choice does not consume the seat.
- `join` reads the invitation through a hidden prompt. The common command does
  not place a bearer token in shell history or the process list.
- Display names are case-insensitively unique on one server. On collision,
  `join` says that the name is in use and asks again. It does not append a
  silent number and does not attach to the existing person.
- The first successful join creates a stable internal user ID. A later display
  name change must not change ownership or access records.
- A claimed seat becomes a device credential. The invitation token is not a
  permanent login token and cannot be used again from another device.
- `attach` fails with "run seer join first" when no identity is saved. It never
  falls back to create.
- `attach` opens another client when one is already attached. It does not use
  tmux-style `-d` takeover behavior.
- `detach` affects one client only. It never means stop, kill, delete, sign out,
  or revoke.
- `peek` is a separate verb, not `attach --read-only`. The server enforces the
  mode. Removing the banner or sending input is not a client-side choice.
- If a person name is not an exact match, `peek` shows close names but does not
  choose one automatically.
- Destructive server stop, person revocation, and data deletion are
  administrator operations. They are outside this seven-command common
  surface and must use separate explicit commands with scope and confirmation.

### Why join and attach are different

`join` changes durable identity state. It consumes a seat, creates a person,
and saves a device credential. It can happen only once per invitation.

`attach` changes only connection state. It uses an existing identity and can
happen many times. It must not create anything when the server or identity is
missing.

The first `join` ends with an attach because this copies tmate's useful rule:
the guest's first command reaches the shared product. The different verb still
makes the durable identity change explicit.

## Worked example: two people, one server, one peek

Alice creates the server on `team.example.com`. The first run asks only for
values that are not already configured.

```text
alice@team:~$ seer start
Your name [alice]: alice
Published address [team.example.com:7321]:
Server started at team.example.com:7321.
You are alice.
```

Alice creates one seat for Bob. The example token is shortened and is not a
real token.

```text
alice@team:~$ seer invite
Seat ready. It works once and expires in 1 hour.
Send this invitation through a private channel:

MXS1-team.example.com-7321-A7K4...Q9P2
```

Bob runs one command on macOS. The token prompt does not echo. Bob chooses his
name at this first join. A successful join saves his device credential and
opens his own tree.

```text
bob@mac:~$ seer join
Invitation: [hidden]
Server: team.example.com:7321
Server identity: SHA256:4f:91:...:2c
Name: bob
Joined as bob. Attaching...
```

Bob detaches. His server-side panes continue to run.

```text
bob@mac:~$ seer detach
Detached from team.example.com. Your panes are still running.
```

Bob checks state and attaches again. No address, user name, token, socket path,
or flag is required.

```text
bob@mac:~$ seer list
SERVER            YOU   STATE      PEOPLE
team.example.com  bob   detached   alice, bob

bob@mac:~$ seer attach
Attached to team.example.com as bob.
```

Alice opens one read-only view of Bob's active workspace. The view has a fixed
banner and cannot send input.

```text
alice@team:~$ seer peek bob
PEEK: bob - READ ONLY
Workspace: bob/current
```

This flow has one server, two durable people, two private trees, and one
temporary read-only peek. It does not create a named multiplexer server per
person. It does not expose a socket path or use a read-only flag.

## Recommendation

Adopt the seven-command surface as the design target. Keep the exact words in
help text, the TUI command palette, diagnostics, and protocol audit events.

The most important rules are:

1. `join` creates identity; `attach` only connects.
2. `attach` never means create and never steals another client.
3. `peek` is visibly and server-enforced read-only.
4. `detach` affects one client and preserves server-side work.
5. Invitations are one-use seats, not permanent person credentials.
6. Socket paths, runtime IDs, and process IDs stay in diagnostic output.
7. Destructive administration stays outside the common session surface.
