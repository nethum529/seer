# Multi-user terminal server prior art

Research date: 2026-08-30.

This note covers macOS and Linux. It does not cover Windows or a desktop GUI.
It uses outside sources only.

## Short answer

Use a JupyterHub-shaped design. Run one public broker and one supervised runtime
per human user. The broker authenticates the client, maps it to one stable user
ID, starts or finds that user's runtime, and routes the connection to it. A user
runtime owns that user's workspaces, tabs, panes, PTYs, and child processes. This
keeps one public server while limiting a runtime crash to one user. JupyterHub
uses the same Hub, Authenticator, Spawner, proxy, and single-user-server split
for a different interactive workload
([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html),
[Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html)).

For a small team, start with OS users and OpenSSH public keys. OpenSSH already
maps an authenticated connection to a Unix account, starts a process for the
connection, and reads per-user authorized key files
([sshd](https://man.openbsd.org/sshd.8),
[sshd_config](https://man.openbsd.org/sshd_config)). Add OIDC later if centralized
login and offboarding are worth the added browser flow, claim validation, and
account mapping work
([OpenID Connect Core](https://openid.net/specs/openid-connect-core-1_0.html),
[OAuth device flow](https://datatracker.ietf.org/doc/html/rfc8628)).

Persist declarative state. Save the workspace tree, tab and pane layout, current
directories, and restart commands. Do not promise to restore live process
images. Zellij is the best direct model. It writes a human-readable layout and
commands to the cache, but it asks before it reruns restored commands
([Zellij resurrection](https://zellij.dev/documentation/session-resurrection.html)).

## Comparison

"Disconnect" below means a client transport disconnect. It does not mean a host
reboot or a server-process crash.

| System | Server model | User identity | Per user and shared state | Disconnect behavior | Durable disk state | Fit |
| --- | --- | --- | --- | --- | --- | --- |
| tmux | One background server manages all sessions on one socket. Separate socket names create separate servers ([tmux manual](https://github.com/tmux/tmux/blob/master/tmux.1), [advanced use](https://github.com/tmux/tmux/wiki/Advanced-Use)). | The default socket directory contains the numeric UID. File permissions and the server ACL admit other local users ([tmux manual](https://github.com/tmux/tmux/blob/master/tmux.1), [tmux 3.3 changes](https://github.com/tmux/tmux/blob/master/CHANGES?plain=1)). | Sessions, windows, panes, buffers, and options are shared inside one server. A guest admitted to the socket must be fully trusted ([tmux FAQ](https://github.com/tmux/tmux/wiki/FAQ)). | The server and child programs stay alive. A new client attaches to the session ([getting started](https://github.com/tmux/tmux/wiki/Getting-Started)). | `MISSING`: no durable live-session image is documented. Live state is in the main server process ([getting started](https://github.com/tmux/tmux/wiki/Getting-Started)). | Good PTY model. Bad cross-user security boundary. |
| GNU Screen | A Screen process owns a session. Its socket is in an owner-specific socket directory ([Screen socket directory](https://www.gnu.org/software/screen/manual/html_node/Socket-Directory.html)). | Normal mode is one Unix user. Multiuser mode names other Unix users and requires setuid-root for cross-user attach ([Screen manual](https://www.gnu.org/software/screen/manual/screen.html)). | Windows and commands belong to the session owner. Screen ACLs can grant read, write, and execute rights to named users ([Screen multiuser manual](https://www.gnu.org/software/screen/manual/screen.html#Multiuser-Session)). | Detach leaves the Screen process and its child programs running. `screen -r` resumes it ([Screen invocation](https://www.gnu.org/software/screen/manual/screen.html#Invoking-Screen)). | Socket entries and optional logs are files. `MISSING`: no process-death or reboot resurrection format is documented ([Screen manual](https://www.gnu.org/software/screen/manual/screen.html)). | Useful ACL prior art. Old and privileged cross-user attach path. |
| tmate | A local tmate, which is a tmux fork, opens an outbound SSH connection to a relay. The local host still owns the terminal session ([tmate README](https://github.com/tmate-io/tmate), [client source](https://github.com/tmate-io/tmate/blob/master/tmate-ssh-client.c)). | A read-write or read-only session token is used as the remote SSH name. An optional authorized-keys file can restrict guests ([session source](https://github.com/tmate-io/tmate-websocket/blob/master/lib/tmate/session.ex), [host source](https://github.com/tmate-io/tmate/blob/master/tmate-session.c)). | The host owns one local session. Guests share that session. The relay shares transport and session tokens, not a workspace tree ([session source](https://github.com/tmate-io/tmate-websocket/blob/master/lib/tmate/session.ex)). | The host reconnects to the relay and sends reconnection state. The relay reports when the host is disconnected ([host reconnect source](https://github.com/tmate-io/tmate/blob/master/tmate-session.c), [relay source](https://github.com/tmate-io/tmate-websocket/blob/master/lib/tmate/session.ex)). | `MISSING`: the public source does not define durable relay restoration of the host's child processes. | Good capability-link sharing pattern. Not a multi-user workspace owner. |
| Zellij | A named live session has a server. CLI actions connect to the Zellij server, send a message, and disconnect ([programmatic control](https://zellij.dev/documentation/programmatic-control.html), [commands](https://zellij.dev/documentation/commands.html)). | Local sessions follow the invoking OS user. The optional web server uses revocable hashed login tokens and can issue read-only tokens ([web client](https://zellij.dev/documentation/web-client.html)). | Tabs, panes, clients, and plugins are session state. The web server can expose existing sessions, but the public docs do not define separate workspace trees per authenticated token ([web client](https://zellij.dev/documentation/web-client.html)). | A forced client close detaches by default. A client can attach to a named live session ([options](https://zellij.dev/documentation/options.html), [commands](https://zellij.dev/documentation/commands.html)). | Layout, pane commands, and optional viewport and scrollback are serialized to the user cache. Commands are not run until the user confirms ([resurrection](https://zellij.dev/documentation/session-resurrection.html)). | Best direct persistence model. `UNKNOWN`: token-to-user isolation is not documented. |
| mosh | SSH starts one unprivileged `mosh-server` for one connection. The client then uses encrypted UDP state synchronization ([Mosh site](https://mosh.org/)). | SSH authenticates the OS user. Mosh itself does not listen for logins or authenticate users ([Mosh site](https://mosh.org/)). | One client and one remote shell state belong to one OS user. There is no shared workspace model ([Mosh README](https://github.com/mobile-shell/mosh)). | The server and client synchronize the latest screen state. The client can roam across IP changes and temporary network loss ([Mosh site](https://mosh.org/)). | `MISSING`: Mosh documents an ordinary process that lasts for the connection, not a reboot-restorable session ([Mosh site](https://mosh.org/)). | Good reconnect protocol ideas. Not a multiplexer or hub. |
| Eternal Terminal | A system `etserver` routes connections to an `etterminal` process that runs as the user and owns the PTY ([protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)). | SSH performs the initial authentication. A generated client ID and passkey authenticate reconnects to `etserver` ([protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)). | `etserver` is shared. Each `etterminal`, PTY, client ID, and passkey are per connection and OS user ([protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)). | The terminal process stays alive and proxies terminal data when the client reconnects ([protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)). | Runtime FIFOs can be under `/var/run` or a per-user runtime directory. `MISSING`: no durable child-process restore format is documented ([protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)). | A useful shared-router plus per-user-process example. Not a workspace tree. |
| OpenSSH sshd | One public daemon listens and forks a daemon for each incoming connection ([sshd](https://man.openbsd.org/sshd.8)). | The requested Unix username plus password, public key, keyboard-interactive, or other configured methods identify the login ([sshd_config](https://man.openbsd.org/sshd_config)). | Host keys and policy are shared. UID, home directory, file permissions, environment, shell, and processes are per OS user ([sshd](https://man.openbsd.org/sshd.8), [sshd_config](https://man.openbsd.org/sshd_config)). | Plain SSH has no reattach contract. A separate tmux, Screen, Mosh, or ET layer keeps interactive state ([tmux getting started](https://github.com/tmux/tmux/wiki/Getting-Started), [Mosh site](https://mosh.org/)). | User files and OpenSSH configuration persist. `MISSING`: sshd does not persist terminal topology or live commands ([sshd](https://man.openbsd.org/sshd.8)). | Lowest-cost identity and permission base for a small Unix-only team. |
| Coder | A control plane provisions workspaces. An agent inside each workspace dials out and serves SSH, terminal, IDE, and port connections ([architecture](https://coder.com/docs/admin/infrastructure/architecture)). | Coder has users, sessions, API tokens, GitHub OAuth, and OIDC login ([sessions and tokens](https://coder.com/docs/admin/users/sessions-tokens), [OIDC](https://coder.com/docs/admin/users/oidc-auth)). | Workspaces have owners and ACL-based sharing. Compute and storage resources can be persistent or ephemeral ([workspace sharing](https://coder.com/docs/user-guides/shared-workspaces), [workspace lifecycle](https://coder.com/docs/user-guides/workspace-lifecycle)). | A connection can end while the workspace remains running. A stopped workspace can be started again ([workspace lifecycle](https://coder.com/docs/user-guides/workspace-lifecycle)). | The control-plane database stores metadata. Template resources decide which workspace data survives stop. Delete destroys all resources ([workspace lifecycle](https://coder.com/docs/user-guides/workspace-lifecycle)). | Strong control-plane prior art. Much more infrastructure than this project needs. |
| Gitpod | Current Gitpod creates standardized environments in the user's infrastructure. Gitpod Classic used managed ephemeral workspace containers ([current overview](https://www.gitpod.io/docs), [Classic lifecycle](https://www.gitpod.io/docs/classic/user/configure/workspaces/workspace-lifecycle)). | Gitpod Classic Enterprise used OIDC. Classic GitHub login used OAuth and created a Gitpod user ([Classic SSO](https://www.gitpod.io/docs/enterprise/setup-gitpod/configure-sso), [Classic GitHub auth](https://www.gitpod.io/docs/classic/payg/authentication/github)). | Environments are tied to users and organizations. Classic also allowed a running workspace to be shared with multiple users ([configuration overview](https://www.gitpod.io/docs/classic/user/configure/overview), [Classic collaboration](https://www.gitpod.io/docs/configure/workspaces/collaboration)). | Classic workspaces stopped after inactivity. Restart created a new container and restored saved workspace files ([Classic lifecycle](https://www.gitpod.io/docs/classic/user/configure/workspaces/workspace-lifecycle)). | The documented Classic contract saved only `/workspace`. It did not save the old container or live processes ([Classic lifecycle](https://www.gitpod.io/docs/classic/user/configure/workspaces/workspace-lifecycle)). | Good file-persistence boundary. Version-specific architecture makes it poor direct code prior art. |
| VS Code Server | A VS Code client installs and manages a server under the remote login user. Remote Tunnels can expose it through an outbound tunnel ([remote FAQ](https://code.visualstudio.com/docs/remote/faq), [remote tunnels](https://code.visualstudio.com/docs/remote/tunnels)). | Remote SSH uses SSH identity. Remote Tunnels require the same GitHub or Microsoft account at both ends ([remote FAQ](https://code.visualstudio.com/docs/remote/faq), [remote tunnels](https://code.visualstudio.com/docs/remote/tunnels)). | Remote files, extensions, and server processes use the remote login user's permissions. One server instance is designed for one user or client at a time ([remote FAQ](https://code.visualstudio.com/docs/remote/faq), [remote tunnels](https://code.visualstudio.com/docs/remote/tunnels)). | The VS Code client manages server start and stop. A tunnel can be reconnected while the remote tunnel process remains active ([remote FAQ](https://code.visualstudio.com/docs/remote/faq), [remote tunnels](https://code.visualstudio.com/docs/remote/tunnels)). | Source files and server installation remain on the remote filesystem. `MISSING`: no terminal-process checkpoint is documented ([remote FAQ](https://code.visualstudio.com/docs/remote/faq)). | Confirms the single-user remote-agent pattern. It is not a multi-client terminal server. |
| JupyterHub | One public proxy fronts a Hub and one spawned single-user server per user ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)). | An Authenticator returns a username. The default uses PAM. OAuthenticator supports external OAuth providers ([authenticators](https://jupyterhub.readthedocs.io/en/stable/reference/authenticators.html)). | The Hub database, proxy, policy, and routing are shared. A Spawner instance and single-user server are per user. Named servers allow more than one server name per user in the API ([Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html), [REST API](https://jupyterhub.readthedocs.io/en/stable/reference/rest-api.html)). | Closing the browser does not require the single-user server to stop. The Spawner polls it and the Hub removes its route only after it stops ([Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html)). | The Hub database and cookie secret persist control state. User files persist according to the selected local, container, or cluster Spawner and storage design ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html), [user environments](https://jupyterhub.readthedocs.io/en/4.x/howto/configuration/config-user-env.html)). | Closest architecture match. |
| OpenHands | The community server is documented as single-user. Enterprise runs an application server plus one isolated sandbox pod per conversation ([FAQ](https://docs.openhands.dev/overview/faqs), [resource limits](https://docs.openhands.dev/enterprise/k8s-install/resource-limits)). | Community has no built-in multi-tenant auth. Enterprise advertises SAML/SSO and RBAC ([FAQ](https://docs.openhands.dev/overview/faqs), [enterprise](https://docs.openhands.dev/enterprise)). | The application server handles UI, API, and orchestration. Each enterprise conversation gets its own sandbox pod ([resource limits](https://docs.openhands.dev/enterprise/k8s-install/resource-limits)). | Remote conversations use HTTP and WebSocket connections to a sandbox. `UNKNOWN`: public enterprise docs do not define exact reconnect ownership rules ([API sandbox](https://docs.openhands.dev/sdk/guides/agent-server/api-sandbox)). | Docker runtimes support bind mounts, named volumes, and overlays. `UNKNOWN`: public enterprise docs do not define the full conversation and workspace retention contract ([runtime architecture](https://docs.openhands.dev/openhands/usage/architecture/runtime)). | Worth naming because it isolates agent execution. Public community code is not a safe multi-user base. |

## tmux and GNU Screen

### The normal model is one owner

tmux starts a server automatically for the invoking user. It puts the default
socket below `tmux-UID`, and that directory must not be accessible to the world
([tmux manual](https://github.com/tmux/tmux/blob/master/tmux.1)). Screen uses a
mode 700 socket directory, normally below the user's home or a system socket
root
([Screen socket directory](https://www.gnu.org/software/screen/manual/html_node/Socket-Directory.html)).
These defaults use the Unix UID and socket permissions as the primary boundary.

tmux keeps every session for one socket in one main server process
([tmux getting started](https://github.com/tmux/tmux/wiki/Getting-Started)). A
server exit therefore ends every tmux session on that socket. This is a direct
example of the shared-process crash blast radius
([tmux server source](https://github.com/tmux/tmux/blob/master/server.c)).

### tmux sharing today

Since tmux 3.3, `server-access` can allow a named local user and mark that user
read-only or read-write. The socket's filesystem permissions still need a
separate manual change
([tmux changes](https://github.com/tmux/tmux/blob/master/CHANGES?plain=1),
[tmux manual](https://github.com/tmux/tmux/blob/master/tmux.1)). The ACL is for
the server socket, not an independent user workspace. The upstream FAQ says any
user who can access a socket must be fully trusted. It also says read-only and
`server-access` are convenience controls, not a security boundary
([tmux FAQ](https://github.com/tmux/tmux/wiki/FAQ)).

This makes tmux sharing suitable for trusted pair work. It is not suitable for
isolating several human users inside one server process. `MISSING`: per-session
security principals, per-user workspace ownership, and a durable restore image.

### Screen sharing today

Screen starts in single-user mode. In multiuser mode, `acladd`, `aclchg`, and
`acldel` grant rights to named Unix users. Rights can cover commands and
windows. Cross-user attach with `screen -r owner/session` requires a setuid-root
Screen installation
([Screen multiuser manual](https://www.gnu.org/software/screen/manual/screen.html#Multiuser-Session),
[Screen invocation](https://www.gnu.org/software/screen/manual/screen.html#Invoking-Screen)).

Screen has finer command and window ACL vocabulary than tmux. It also has a
larger privileged attack surface because the documented cross-user path depends
on setuid-root. `MISSING`: a non-privileged broker, a separate tree per user,
and restart-after-process-death persistence.

## tmate

tmate changes who carries the connection, not who owns the PTY. The local tmate
is a tmux fork. It connects outward to the tmate SSH server and shares session
state with it
([tmate README](https://github.com/tmate-io/tmate),
[SSH client source](https://github.com/tmate-io/tmate/blob/master/tmate-ssh-client.c)).
The relay publishes separate read-write and read-only tokens. The token appears
as the SSH username in the connection string
([relay source](https://github.com/tmate-io/tmate-websocket/blob/master/lib/tmate/session.ex)).

The local host can send an authorized-keys list to limit who may join. If the
relay connection fails, the local host retries and marks the next connection as
a reconnection so it can send current state
([host source](https://github.com/tmate-io/tmate/blob/master/tmate-session.c)).
The public server container needs `SYS_ADMIN` because it creates nested
namespaces to secure sessions
([server README](https://github.com/tmate-io/tmate-ssh-server)).

Useful parts for this project are capability links, explicit read-only access,
outbound-only host connections, and relay reconnection. Do not copy token-only
identity for primary workspace ownership. A leaked bearer token grants its
holder the token's authority
([OAuth bearer token definition](https://datatracker.ietf.org/doc/rfc6750/)).

## Zellij

Zellij separates live attachment from resurrection. A client can attach to a
named live session. Independent CLI commands connect to the session server and
disconnect after the command
([commands](https://zellij.dev/documentation/commands.html),
[programmatic control](https://zellij.dev/documentation/programmatic-control.html)).
This resembles the local per-user runtime needed here.

Zellij writes a KDL layout to the system cache on a timer. It records tabs,
panes, order, current directories, and discovered commands. Viewport and
scrollback storage are optional because they increase resource and cache use
([resurrection](https://zellij.dev/documentation/session-resurrection.html),
[options](https://zellij.dev/documentation/options.html)). A resurrected command
is placed behind a confirmation banner. Zellij does not silently execute it
([resurrection](https://zellij.dev/documentation/session-resurrection.html)).

Zellij plugins are WebAssembly/WASI components. They can render UI, subscribe to
events, and request host actions. Sensitive actions are behind permissions such
as reading application state, running commands, opening files, writing to
stdin, and reading pane contents
([plugins](https://zellij.dev/documentation/plugins.html),
[plugin permissions](https://zellij.dev/documentation/plugin-api-permissions.html)).
This is useful extension prior art. It does not solve human-user isolation.

The newer Zellij web server adds hashed revocable login tokens, read-only
tokens, TLS requirements for non-loopback listeners, and remote attach
([web client](https://zellij.dev/documentation/web-client.html)). `UNKNOWN`: the
public docs do not define a stable human identity for each token or a separate
workspace tree per token. We would have to build those mappings and ACLs.

## Mosh and Eternal Terminal

Mosh exists because a TCP byte stream handles roaming and high latency poorly.
It starts `mosh-server` through SSH, closes the SSH transport, and synchronizes
screen state over encrypted UDP. A valid packet from a new address moves the
server's reply address, which permits IP roaming
([Mosh site](https://mosh.org/)). It sends the newest screen state and may skip
intermediate frames. It is a reconnect transport, not a terminal history or
workspace database
([Mosh site](https://mosh.org/)).

Eternal Terminal uses TCP and an explicit router. `etserver` is shared, while
`etterminal` runs as the logged-in user and owns the terminal. SSH bootstraps
the session. A generated client ID and passkey authenticate later connections
([ET protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)).
This is close to the recommended process split, but its per-user unit is one
terminal connection rather than a workspace tree.

Both systems preserve a live process by keeping its server-side process alive.
Neither system documents a cross-reboot child-process image
([Mosh site](https://mosh.org/),
[ET protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)).
Use their reconnect ideas below the workspace protocol. Do not treat reconnect
as persistence.

## OpenSSH and the boring answer

The boring answer is one Unix account and one multiplexer runtime per human.
One sshd still listens publicly. sshd forks for each connection and performs
authentication, account checks, environment setup, UID selection, and command
execution
([sshd](https://man.openbsd.org/sshd.8),
[sshd_config](https://man.openbsd.org/sshd_config)). The per-user multiplexer
then runs with that user's normal filesystem permissions.

Benefits:

- The kernel enforces file ownership and process ownership through the UID
  boundary. OpenSSH starts the login under the target account
  ([sshd](https://man.openbsd.org/sshd.8)).
- Public keys live in files relative to each user's home by default. Key
  revocation is a file update
  ([sshd_config](https://man.openbsd.org/sshd_config)).
- A broken user runtime does not require another user's runtime to exit. This
  follows from using separate runtime processes, unlike tmux's one main process
  per socket
  ([tmux getting started](https://github.com/tmux/tmux/wiki/Getting-Started)).

Costs:

- An administrator must create, disable, and remove OS accounts, home
  directories, groups, and keys. JupyterHub's PAM model has the same account
  requirement
  ([JupyterHub technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).
- Shared projects need explicit Unix group or ACL policy. tmux socket sharing
  shows that filesystem permissions and application ACLs are separate concerns
  ([tmux changes](https://github.com/tmux/tmux/blob/master/CHANGES?plain=1)).
- A root broker, sshd, launchd job, or systemd service must start and supervise
  the runtime with the correct UID. `MISSING`: this project still needs that
  supervisor and its macOS and Linux service definitions.
- One runtime per user uses more fixed memory than one global runtime. The
  exact amount is `UNKNOWN` until this project has a measurable prototype.

This is still the recommended first deployment model. The team is small, the
targets are Unix systems, and the product is terminal-first. It removes an
entire in-process permission system from version 1.

## Coder, Gitpod, and VS Code Server

### Coder

Coder separates its control plane from workspace compute. Terraform templates
create resources. A workspace agent dials out to the control plane and exposes
SSH, terminals, IDE access, port forwarding, and liveness
([Coder architecture](https://coder.com/docs/admin/infrastructure/architecture)).
Workspaces have an owner. Current Coder can also grant `use` or `admin` access
to other users
([workspace sharing](https://coder.com/docs/user-guides/shared-workspaces)).

Stop and delete are different. Stop can destroy ephemeral compute while leaving
persistent resources. Delete destroys all workspace resources
([workspace lifecycle](https://coder.com/docs/user-guides/workspace-lifecycle)).
This is the right lifecycle vocabulary for this project: disconnect, stop,
archive, and delete must not be aliases.

### Gitpod

Gitpod's current product and Gitpod Classic have different architecture docs.
The durable filesystem detail available in the official docs is explicitly a
Classic contract. Classic backed up `/workspace`, removed the old container,
and restored that directory into a new container on restart
([Classic lifecycle](https://www.gitpod.io/docs/classic/user/configure/workspaces/workspace-lifecycle)).
It did not preserve arbitrary files outside `/workspace` or live processes
([Classic lifecycle](https://www.gitpod.io/docs/classic/user/configure/workspaces/workspace-lifecycle)).

The lesson is to define one explicit durable root. `MISSING`: this project must
decide whether each workspace points at an existing host directory, owns a
managed directory, or can support both. It must also define archive and delete
behavior for that directory.

### VS Code Server

VS Code Server runs as the same account used to sign in to the remote machine.
The client installs, starts, stops, and updates it
([remote FAQ](https://code.visualstudio.com/docs/remote/faq)). Remote Tunnels
use outbound connections and require the same GitHub or Microsoft account on
the host and client. A server instance is designed for only one user or client
at a time
([remote tunnels](https://code.visualstudio.com/docs/remote/tunnels)).

This supports two decisions. Remote helper processes should run with user
permissions, not broker permissions. Also, multi-user collaboration should be
an explicit sharing feature, not accidental concurrent access to a single-user
backend.

## JupyterHub in depth

JupyterHub is the closest structural match.

### Components

1. The proxy is the only public component. It routes `/hub/` to the Hub and
   `/user/name/` to a single-user server
   ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).
2. The Hub owns users, authentication, policy, database state, and server
   coordination
   ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).
3. The Authenticator validates login data and returns a normalized username.
   The Spawner receives that username
   ([concepts](https://jupyterhub.readthedocs.io/en/stable/explanation/concepts.html)).
4. One Spawner instance represents each user. Its required control surface is
   `start`, `poll`, `stop`, `get_state`, and `load_state`
   ([Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html)).
5. The spawned single-user server serves the interactive application. The
   default local spawner runs it under the user's system account. Other
   spawners can use containers or remote compute
   ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).

### Request and attach flow

1. The proxy sends an unauthenticated request to the Hub
   ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).
2. The Authenticator returns the user name after successful login
   ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).
3. The Hub starts that user's server through the Spawner if needed
   ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).
4. The Hub adds a proxy route from the stable user URL to the server's current
   internal address
   ([proxy API](https://jupyterhub.readthedocs.io/en/stable/howto/proxy.html)).
5. The single-user server uses JupyterHub's internal OAuth. It does not need to
   know whether the upstream login used PAM, GitHub, or another identity
   provider
   ([JupyterHub OAuth](https://jupyterhub.readthedocs.io/en/stable/explanation/oauth.html)).

This project should copy the separation, not the HTTP details. Replace the HTTP
proxy route with an authenticated terminal connection routed to a per-user Unix
socket or other local IPC endpoint.

### Persistence and recovery

JupyterHub stores Hub state in a database and stores a cookie secret on disk.
The database lets the Hub remember users and where running servers were found.
The cookie secret prevents all login cookies from being invalidated at every
Hub restart
([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).

Spawner state is a reference to a running resource, not a checkpoint of its
memory. On Hub startup, the Spawner can load saved state and poll the user
server. If the server is not running, the Hub removes the route
([Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html)).
This is the correct distinction for this project:

- Broker restart: reconnect to still-running user runtimes.
- User runtime restart: reconstruct the saved workspace tree.
- Host reboot: reconstruct layout and commands. Do not claim the old processes
  are alive.

### Multiple workspaces and sharing

JupyterHub's API supports a default server and named servers under a user. This
maps well to a tree root containing several workspaces
([REST API](https://jupyterhub.readthedocs.io/en/stable/reference/rest-api.html)).
JupyterHub 5 also supports explicit limited sharing of a user's server. Sharing
is disabled by default and uses filtered scopes
([sharing reference](https://jupyterhub.readthedocs.io/en/latest/reference/sharing.html),
[scopes](https://jupyterhub.readthedocs.io/en/stable/rbac/scopes.html)).

For this project, workspace ownership should remain separate from workspace
sharing. `MISSING`: define whether a guest can view, type, create panes, change
layout, stop processes, or change sharing. Do not reduce these rights to one
read-write boolean.

## Newer agent prior art

OpenHands is worth naming, but its public community server is explicitly not a
multi-tenant system. Its FAQ says it lacks built-in authentication, isolation,
and scalability for several users
([OpenHands FAQ](https://docs.openhands.dev/overview/faqs)). OpenHands Enterprise
advertises SAML/SSO, multi-user RBAC, and an isolated sandbox for each agent.
Its Kubernetes docs say each conversation has its own sandbox pod
([enterprise](https://docs.openhands.dev/enterprise),
[resource limits](https://docs.openhands.dev/enterprise/k8s-install/resource-limits)).

The useful lesson is to isolate code-running agents more strongly than the
control plane. `UNKNOWN`: the public enterprise docs do not specify enough
storage, reconnect, and authorization internals to use it as the main design
reference. We would need to build and document those contracts ourselves.

## Identity and authentication options

OS users and SSH keys answer different questions. The OS user selects a UID,
home, file permissions, and process owner. The SSH key proves that a client may
log in as that user
([sshd](https://man.openbsd.org/sshd.8),
[sshd_config](https://man.openbsd.org/sshd_config)). They work best together.

The ranking below is by initial product and operating cost for a small trusted
team. It is not a ranking of maximum enterprise capability.

| Rank | Option | Initial cost | Ongoing cost | Decision |
| --- | --- | --- | --- | --- |
| 1 | OS users plus SSH keys | Low product cost. sshd already implements the flow and per-user key files ([sshd](https://man.openbsd.org/sshd.8), [sshd_config](https://man.openbsd.org/sshd_config)). | An admin manages accounts, homes, groups, and key removal. | Use for version 1. |
| 2 | Server-issued bearer tokens | Medium. Build secure generation, hashing, lookup, expiry, scope, revocation, and audit. A bearer can be used by whoever possesses it ([RFC 6750](https://datatracker.ietf.org/doc/rfc6750/)). | Easy for short-lived share links. Harder for durable human identity and incident response. | Use only for invitations, reconnect tickets, or explicit sharing. |
| 3 | OIDC | Medium to high. Build authorization code or device flow, callback or polling, signature and claim checks, subject mapping, and session refresh ([OIDC Core](https://openid.net/specs/openid-connect-core-1_0.html), [RFC 8628](https://datatracker.ietf.org/doc/html/rfc8628)). | Low account lifecycle cost when the team already has an identity provider. | Add when centralized offboarding or browser clients justify it. |
| 4 | App-managed passwords for OS users | Low code only if PAM is reused. Password reset, storage policy, brute-force defense, and second-factor policy remain operational work. JupyterHub's default PAM authenticator shows the reuse model ([JupyterHub authenticators](https://jupyterhub.readthedocs.io/en/stable/reference/authenticators.html)). | Higher support and security policy cost than SSH keys for this small terminal-only team. | Do not build a password database. |

If the application exposes bearer tokens, it must use TLS, avoid tokens in URLs,
use short lifetimes where practical, and limit audience and scope
([RFC 6750](https://datatracker.ietf.org/doc/rfc6750/)). If a terminal client
later needs OIDC without a local browser callback, OAuth device authorization is
the standard flow for a client that can display a URL and code and make outbound
HTTPS requests
([RFC 8628](https://datatracker.ietf.org/doc/html/rfc8628)).

## One shared process or one process per user

| Concern | One shared runtime process | One runtime process per user |
| --- | --- | --- |
| Isolation | Every authorization check is application code. tmux warns that socket users are inside one trust boundary ([tmux FAQ](https://github.com/tmux/tmux/wiki/FAQ)). | The OS can apply a UID boundary. A container or sandbox can add a stronger boundary ([sshd](https://man.openbsd.org/sshd.8), [OpenHands runtime](https://docs.openhands.dev/openhands/usage/architecture/runtime)). |
| Crash blast radius | One process owns every user's in-memory tree. A fatal server exit loses all live trees, as one tmux server owns all sessions on its socket ([tmux getting started](https://github.com/tmux/tmux/wiki/Getting-Started)). | One user loses live runtime state. The broker and other user runtimes can stay up. JupyterHub uses this per-user-server split ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)). |
| Memory | Lowest fixed overhead. Shared caches and event loops exist once. Exact savings are `UNKNOWN` until measured. | Repeats runtime stacks and buffers. Exact cost is `UNKNOWN` until measured. Idle runtimes can be started lazily, as JupyterHub spawns on demand ([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)). |
| Permissions | A single service UID cannot rely on normal per-user file ownership. It needs application ACLs or a stronger sandbox. | A runtime can run as the human's UID and naturally use that user's home and repository permissions ([sshd](https://man.openbsd.org/sshd.8)). |
| Operations | One process is simple to start, but upgrades and faults affect everyone. | Needs a supervisor, IPC discovery, health checks, and per-user logs. JupyterHub's Spawner API is the useful control surface ([Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html)). |

Recommendation: use a hybrid. Run one public broker and one runtime per human
user. Start the runtime lazily. Keep all of one user's workspaces in that one
runtime at first. This avoids one process per workspace while still limiting
cross-user faults. Give the broker only identity, routing, supervision, and
metadata duties. Do not let it own PTYs or agent child processes.

## Persistence model

### Layer 1: broker state

Persist these records in a small transactional database:

- Stable user ID and external identity mapping.
- Workspace IDs, names, owners, and parent relationships.
- Sharing grants and revocations.
- User-runtime status, endpoint, generation, and last successful heartbeat.
- Schema version and audit events.

This follows JupyterHub's durable Hub database and restartable Spawner state
([technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html),
[Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html)).
`MISSING`: this project must define the schema, migration rules, backup, file
permissions, and corruption recovery.

### Layer 2: workspace files

Persist repository and user files in one explicit durable root per workspace.
Do not imply that every path visible to a pane is managed storage. Gitpod
Classic's `/workspace` rule is a clear example of a narrow durable boundary
([Gitpod Classic lifecycle](https://www.gitpod.io/docs/classic/user/configure/workspaces/workspace-lifecycle)).
`MISSING`: define host-path workspaces, managed workspaces, archive, delete,
quota, ownership, and backup behavior.

### Layer 3: terminal topology

Persist a declarative snapshot with:

- Workspace, tab, and pane IDs and ordering.
- Split layout and focused item.
- Pane current directory.
- Pane title and restart command.
- Terminal size and selected safe environment keys.
- Optional viewport and bounded scrollback.
- Snapshot format version and last-write generation.

Zellij proves this is enough to reconstruct useful terminal context and shows
why viewport and scrollback should be optional
([Zellij resurrection](https://zellij.dev/documentation/session-resurrection.html)).
Write snapshots atomically. `MISSING`: define the file or database format,
write cadence, size limits, redaction rules, and upgrade policy.

### Layer 4: live processes

While a user runtime is alive, detach only the client. Keep PTYs and children
running. tmux, Screen, Mosh, and Eternal Terminal all use this rule
([tmux manual](https://github.com/tmux/tmux/blob/master/tmux.1),
[Screen manual](https://www.gnu.org/software/screen/manual/screen.html),
[Mosh site](https://mosh.org/),
[ET protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)).

After a user runtime or host dies, restore topology, directories, and command
text. Do not automatically rerun commands with side effects. Show a confirmation
step like Zellij
([Zellij resurrection](https://zellij.dev/documentation/session-resurrection.html)).

No surveyed terminal or workspace system restores the old child process memory
on both macOS and Linux. Linux has CRIU, which can checkpoint process memory,
registers, file descriptors, namespaces, and other kernel state
([CRIU overview](https://www.criu.org/Main_Page),
[CRIU design](https://criu.org/Checkpoint/Restore)). CRIU is Linux-specific and
needs kernel checkpoint features
([CRIU Linux kernel requirements](https://www.criu.org/Linux_kernel)). It also
cannot generically dump several external resources and devices
([CRIU limitations](https://criu.org/index.php?title=What_cannot_be_checkpointed)).
That does not meet this project's macOS and Linux contract.

Therefore, live-process checkpoint and restore is out of scope. `MISSING`: the
product must tell the user which panes are live, which were reconstructed, and
which commands are waiting for approval.

## Proposed server contract

The following contract is a synthesis of the cited systems:

1. One broker listens on the public endpoint. This follows sshd and JupyterHub's
   proxy model
   ([sshd](https://man.openbsd.org/sshd.8),
   [JupyterHub technical overview](https://jupyterhub.readthedocs.io/en/4.1.3/reference/technical-overview.html)).
2. Authentication returns one stable internal user ID. It does not return a
   workspace ID or a mutable display name
   ([OIDC subject identifier](https://openid.net/specs/openid-connect-core-1_0.html),
   [JupyterHub concepts](https://jupyterhub.readthedocs.io/en/stable/explanation/concepts.html)).
3. The broker resolves or starts one runtime for that user. The minimum runtime
   supervisor API is start, poll, stop, save state, and load state
   ([JupyterHub Spawner API](https://jupyterhub.readthedocs.io/en/stable/api/spawner.html)).
4. The runtime runs as the user's UID in version 1. It owns all PTYs and child
   processes for that user
   ([sshd](https://man.openbsd.org/sshd.8)).
5. Client loss removes only the client. Runtime loss marks its panes exited and
   makes the latest declarative snapshot available for reconstruction
   ([tmux manual](https://github.com/tmux/tmux/blob/master/tmux.1),
   [Zellij resurrection](https://zellij.dev/documentation/session-resurrection.html)).
6. Workspace sharing is explicit, scoped, and revocable. A share does not
   change the workspace owner
   ([JupyterHub sharing](https://jupyterhub.readthedocs.io/en/latest/reference/sharing.html)).
7. Read-only is a user experience control, not the only security boundary.
   tmux explicitly warns about this distinction
   ([tmux FAQ](https://github.com/tmux/tmux/wiki/FAQ)).

## MISSING work

The surveyed systems do not provide the complete product. We must build:

- A broker protocol that carries authenticated user identity before terminal
  attach.
- An SSH-key-to-internal-user mapping for any mode that does not use a distinct
  OS account.
- A per-user runtime supervisor for launchd and systemd.
- A secure broker-to-runtime IPC endpoint and stale-endpoint cleanup.
- Runtime generation IDs so a broker does not attach to an old process after a
  restart.
- A transactional metadata store, migrations, backups, and recovery.
- A versioned workspace, tab, pane, and restart-command snapshot format.
- Explicit live, detached, exited, reconstructed, and waiting-for-confirmation
  states.
- Workspace ownership and sharing rights beyond read-only and read-write.
- Audit records for login, attach, detach, share, revoke, start, stop, restore,
  and delete.
- Resource limits and a response to a user runtime that consumes too much CPU,
  memory, PTYs, file descriptors, or disk.
- A documented admin recovery path when the broker database and durable
  workspace directories disagree.

The exact runtime memory overhead, snapshot write cost, maximum practical pane
count, and launch time are `UNKNOWN`. Measure them in a prototype before adding
more isolation layers.
