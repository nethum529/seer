# Terminal sharing prior art

Date: 2026-09-02

## Scope

Seer needs to replace Tailscale for one narrow case:

- The owner runs Seer on one Linux machine behind home NAT.
- A friend uses macOS and can paste one line.
- The friend must not create an account or install a VPN.
- The connection must expose Seer, not the full owner network.
- The design must be secure and simple for the owner.

This study uses product documentation, source repositories, protocol documents, and published design papers. It does not use marketing pages.

## How friend steps are counted

- A step is one friend action after the owner sends an invite.
- Opening a URL, running a command, installing a tool, and signing in each count.
- A browser and macOS OpenSSH are prerequisites, not install steps.
- An owner approval does not count as a friend step.
- The count uses the shortest documented secure path.
- Lengths are exact only when the format is fixed. Variable formats are marked as such.

## tmate

**Invite shape:** The normal invite is `ssh <token>@nyc1.tmate.io`. The current server source defines a 25-character token from a 32-character alphabet. This gives 125 bits before protocol limits. A read-only token adds `ro-`. The command is about 43 characters on the public host. A web invite is `https://tmate.io/t/<token>`, about 44 characters. The token is a session locator and bearer capability. [Token source](https://github.com/tmate-io/tmate-ssh-server/blob/master/tmate-ssh-daemon.c), [constant](https://github.com/tmate-io/tmate-ssh-server/blob/master/tmate.h), [web source](https://github.com/tmate-io/tmate-websocket/blob/master/lib/tmate/session.ex)

**Friend join:**

1. Run the SSH invite, or open the web invite.

The SSH path needs OpenSSH. The web path needs a browser. Neither needs an account. [tmate README](https://github.com/tmate-io/tmate)

**Connectivity:** The host makes an outbound SSH connection to a tmate server. The friend connects to that server by SSH or WebSocket. The public service works through NAT. The owner can run `tmate-ssh-server`. [Architecture paper](https://viennot.com/tmate.pdf), [self-hosted server](https://github.com/tmate-io/tmate-ssh-server)

**Security, trust, and reach:** The host-server and friend-server links are separate SSH connections. The server hosts the tmux session, so it can observe terminal content. The bearer token is the guest authority. Separate read-write and read-only tokens limit control. A friend reaches one session, not a private network. A writer controls the shell as the owner user. [Architecture paper](https://viennot.com/tmate.pdf)

**License and maintenance:** Permissive BSD and ISC-style licenses. The repository is active, but the latest tagged client release is 2.4.0 from 2019. [Repository](https://github.com/tmate-io/tmate), [2.4.0 release](https://github.com/tmate-io/tmate/releases/tag/2.4.0), [commits](https://github.com/tmate-io/tmate/commits/master/)

## Upterm

**Invite shape:** `upterm host` prints `ssh <session>@uptermd.upterm.dev`. The session starts with a random 20-character base62 identifier, about 119 bits, but the public SSH username can also carry route data. The full invite is variable and normally more than 50 characters. [Session source](https://github.com/owenthereal/upterm/blob/master/cmd/upterm/command/session.go), [identifier source](https://github.com/owenthereal/upterm/blob/master/utils/utils.go)

**Friend join:**

1. Run the SSH invite.

The friend needs OpenSSH and no Upterm install or account. [Host guide](https://github.com/owenthereal/upterm/blob/master/docs/upterm_host.md)

**Connectivity:** The host runs a local SSH server and opens a reverse SSH tunnel to `uptermd`. The friend reaches that server through the tunnel. Upterm provides a hosted relay and documents complete self-hosting. [Upterm README](https://github.com/owenthereal/upterm)

**Security, trust, and reach:** The friend-to-host SSH session is carried inside the reverse tunnel. From this process split, the relay routes encrypted SSH and does not have the host session key. This is an inference, not an explicit guarantee. The host can allow SSH keys or GitHub and GitLab users. Otherwise, it approves each key unless `--accept` is set. `--read-only` removes input. `--no-sftp` removes default file transfer. TCP forwarding is opt-in. A normal shell and SFTP have owner-user reach. [Host guide](https://github.com/owenthereal/upterm/blob/master/docs/upterm_host.md)

**License and maintenance:** Apache-2.0. Version 0.24.0 was released in May 2026, and development is active. [Repository](https://github.com/owenthereal/upterm), [0.24.0 release](https://github.com/owenthereal/upterm/releases/tag/v0.24.0), [commits](https://github.com/owenthereal/upterm/commits/master/)

## sshx

**Invite shape:** The hosted URL is `https://sshx.io/s/<10-character-session>#<14-character-key>`. It is 43 characters with the default host. The fragment key has about 83 bits with base62. The browser does not send the fragment to the HTTP server. With `--enable-readers`, the short URL is read-only and a writer URL adds another 14-character secret, for about 58 characters. [Controller source](https://github.com/ekzhang/sshx/blob/main/crates/sshx/src/controller.rs), [encryption source](https://github.com/ekzhang/sshx/blob/main/crates/sshx/src/encrypt.rs)

**Friend join:**

1. Open the URL.

The friend needs a browser and no account or install. [sshx README](https://github.com/ekzhang/sshx)

**Connectivity:** The host and browser connect outbound to the hosted sshx server and Redis mesh. The repository documents a development deployment, but says production self-hosting is not supported. [sshx README](https://github.com/ekzhang/sshx)

**Security, trust, and reach:** Terminal messages use Argon2id-derived AES-128-CTR keys. The fragment secret stays at the endpoints, so the server cannot decrypt content. Anyone with a writer URL can control the session. The optional reader URL has lower authority. A reader sees the canvas. A writer can create and control shells with host-user rights. [Encryption source](https://github.com/ekzhang/sshx/blob/main/crates/sshx/src/encrypt.rs), [server source](https://github.com/ekzhang/sshx/blob/main/crates/sshx-server/src/grpc.rs)

**License and maintenance:** MIT. Version 0.4.1 was released in February 2025. The last repository activity was in 2025, and it is not archived. [Repository](https://github.com/ekzhang/sshx), [0.4.1 release](https://github.com/ekzhang/sshx/releases/tag/v0.4.1), [commits](https://github.com/ekzhang/sshx/commits/main/)

## tty-share

**Invite shape:** `tty-share --public` prints a long HTTPS capability URL. The documented example is more than 100 characters. Its opaque token is a session locator and bearer secret. The format is variable and is for copy and paste. [tty-share README](https://github.com/elisescu/tty-share)

**Friend join:**

1. Open the HTTPS invite in a browser.

The browser path needs no account or install. The terminal path needs tty-share.

**Connectivity:** Public mode sends both endpoints through `on.tty-share.com`. Local mode exposes the host HTTP server directly. The proxy is open source and can be self-hosted. [tty-share README](https://github.com/elisescu/tty-share)

**Security, trust, and reach:** TLS protects each connection, but tty-share states that the public proxy can read session data. The URL is the guest authority. `--readonly` removes input. The default reach is one PTY. Optional TCP forwarding can expand reach. [Security notes](https://github.com/elisescu/tty-share#security)

**License and maintenance:** MIT. Version 2.4.1 was released in January 2025. The repository had activity in 2025 and is not archived. [Repository](https://github.com/elisescu/tty-share), [2.4.1 release](https://github.com/elisescu/tty-share/releases/tag/v2.4.1), [commits](https://github.com/elisescu/tty-share/commits/master/)

## ttyd

**Invite shape:** The URL is `http[s]://<host>:7681[/base]`. It has no generated key or code. Its length depends on the public host and path. The owner must make it reachable. [ttyd README](https://github.com/tsl0922/ttyd)

**Friend join:**

1. Open the URL.
2. Enter Basic authentication credentials when enabled.

Anonymous mode removes step 2 but is not suitable for an Internet shell. The friend only needs a browser.

**Connectivity:** The browser connects directly to the host. ttyd has no rendezvous, NAT traversal, hosted relay, or full sharing service. It is self-hosted by design. A separate port forward or tunnel is needed behind NAT.

**Security, trust, and reach:** TLS is optional. ttyd supports Basic auth, a proxy auth header, origin checks, and mutual TLS. Read-only is the default; `-W` enables input. The owner selects one process. A writable shell has owner-user rights. URL arguments and file transfer can increase reach. [ttyd options](https://github.com/tsl0922/ttyd#command-line-options)

**License and maintenance:** MIT. The repository is active. The latest tagged release is 1.7.7 from March 2024. [Repository](https://github.com/tsl0922/ttyd), [1.7.7 release](https://github.com/tsl0922/ttyd/releases/tag/1.7.7), [commits](https://github.com/tsl0922/ttyd/commits/main/)

## Warp Agent Session Sharing

**Invite shape:** The owner copies a variable Warp HTTPS URL. Warp does not document a fixed token format or entropy. It is a hosted session reference, not a client-held encryption key. [Session sharing guide](https://docs.warp.dev/agents/local-agents/session-sharing/)

**Friend join:**

1. Open the shared link in a browser.
2. Sign in to a Warp account.

The browser needs no Warp install, but even an anyone-with-link viewer must sign in. [Session sharing guide](https://docs.warp.dev/agents/local-agents/session-sharing/)

**Connectivity:** Warp sends the host session to its hosted service. The browser connects to that service. There is no direct path and no complete self-host option.

**Security, trust, and reach:** The owner can invite named users, a team, or anyone with the link. View and edit are separate. A viewer can request edit access and the owner approves it. Warp uploads live output and scrollback, so its service can process plaintext. Data stays available for about one week after sharing stops. A viewer sees one agent session. An editor can send agent queries and execute commands. [Session sharing guide](https://docs.warp.dev/agents/local-agents/session-sharing/)

**License and maintenance:** The active Warp client repository is AGPL-3.0. The hosted sharing service is not supplied as a self-hosted open-source product. [Client repository](https://github.com/warpdotdev/warp), [sharing protocol](https://github.com/warpdotdev/session-sharing-protocol)

## Visual Studio Live Share

**Invite shape:** Live Share creates a variable unique HTTPS session URL. The web form is `https://vscode.dev/editor/liveshare/<session-id>`. The opaque ID is not an end-to-end encryption key. [Browser join guide](https://learn.microsoft.com/en-us/visualstudio/liveshare/quickstart/browser-join), [URL schema](https://code.visualstudio.com/docs/remote/vscode-web)

**Friend join:**

1. Open the invite and select the web client if asked.
2. Sign in with GitHub or Microsoft when the host requires identity.

The host can allow anonymous guests, but may need to approve them. A browser guest needs no IDE. A desktop guest needs VS Code and Live Share. [Security guide](https://learn.microsoft.com/en-us/visualstudio/liveshare/reference/security)

**Connectivity:** Live Share tries peer-to-peer transport and uses an Azure relay when direct routing fails. Microsoft runs authentication, discovery, and relay services. There is no complete self-host mode. [Connectivity guide](https://learn.microsoft.com/en-us/visualstudio/liveshare/reference/connectivity)

**Security, trust, and reach:** The endpoints use end-to-end SSH, also through the relay. The relay cannot decrypt workspace data. Guests use signed session claims. The host can require approval, limit identity domains, exclude files, and make the session read-only. Shared terminals start read-only. The host separately shares servers and terminals. A read-write terminal gives host-user shell rights. [Security guide](https://learn.microsoft.com/en-us/visualstudio/liveshare/reference/security)

**License and maintenance:** The extension and service use Microsoft software terms, not an open-source license. The active documentation and issue repository is CC-BY-4.0. [Extension license](https://marketplace.visualstudio.com/items/MS-vsliveshare.vsliveshare/license), [repository](https://github.com/microsoft/live-share)

## Microsoft dev tunnels and VS Code Remote Tunnels

**Invite shape:** A public dev tunnel port can be `https://l3rs99qw-3000.usw2.devtunnels.ms/`, 42 characters in this example. The variable URL has a tunnel ID but no secret. Private access uses identity or a separate connection token that normally expires after 24 hours. VS Code uses `https://vscode.dev/tunnel/<machine>/<folder>`. [CLI commands](https://learn.microsoft.com/en-us/azure/developer/dev-tunnels/cli-commands), [Remote Tunnels guide](https://code.visualstudio.com/docs/remote/tunnels)

**Friend join:**

1. Open the web service or VS Code tunnel URL.
2. Sign in when the tunnel is private.

An anonymous web port removes step 2. A non-HTTP service needs a tunnel client. [Start guide](https://learn.microsoft.com/en-us/azure/developer/dev-tunnels/get-started)

**Connectivity:** The host makes an outbound connection to Microsoft's Azure relay. There is no direct path and no self-host option for the full service. [FAQ](https://learn.microsoft.com/en-us/azure/developer/dev-tunnels/faq)

**Security, trust, and reach:** A tunnel is private to its creator by default. The owner can allow a tenant, organization, anonymous user, or bearer token. Tokens are scoped to one tunnel. The web forwarder terminates HTTPS and offers traffic inspection, so it can read HTTP unless the application adds end-to-end encryption. A generic tunnel exposes selected local ports. A VS Code tunnel exposes a larger development environment. [Security guide](https://learn.microsoft.com/en-us/azure/developer/dev-tunnels/security), [CLI commands](https://learn.microsoft.com/en-us/azure/developer/dev-tunnels/cli-commands)

**License and maintenance:** The active dev tunnels SDK and VS Code CLI source are MIT. The hosted service and remote extensions use Microsoft terms. [SDK](https://github.com/microsoft/dev-tunnels), [VS Code source](https://github.com/microsoft/vscode/blob/main/cli/src/tunnels/dev_tunnels.rs)

## Mosh

**Invite shape:** Mosh has no capability invite. The owner sends `user@host`, then the friend runs `mosh user@host`. SSH identity and the public address give authority and location. [Mosh README](https://github.com/mobile-shell/mosh)

**Friend join:**

1. Install mosh.
2. Run `mosh user@host`.
3. Complete SSH authentication and any first-use host key check.

The server needs `mosh-server`, SSH, and reachable UDP ports.

**Connectivity:** SSH starts `mosh-server` and returns a UDP port and session key. The client then connects directly over UDP, normally on ports 60000 to 61000. There is no relay or NAT rendezvous. A host behind NAT needs a port forward. [Mosh README](https://github.com/mobile-shell/mosh)

**Security, trust, and reach:** SSH supplies server identity and user authentication. Mosh uses an AES-128 session key sent through SSH. It has no guest capability or read-only role. Each connection gives one shell as a Unix account. Mosh does not share an existing terminal or forward ports.

**License and maintenance:** GPL-3.0. The repository is active. Release 1.4.0 is from October 2022. [Repository](https://github.com/mobile-shell/mosh), [release](https://github.com/mobile-shell/mosh/releases/tag/mosh-1.4.0), [commits](https://github.com/mobile-shell/mosh/commits/master/)

## Magic Wormhole

**Invite shape:** The sender gives a short single-use code such as `4-purple-sausages`, 17 characters. The default uses a numeric nameplate and two words, with about 16 bits of password entropy. The code is the rendezvous input and PAKE secret. [API](https://github.com/magic-wormhole/magic-wormhole/blob/master/docs/api.rst), [security analysis](https://github.com/magic-wormhole/magic-wormhole-protocols/blob/main/security.md)

**Friend join:**

1. Install Magic Wormhole.
2. Run `wormhole receive <code>`.

**Connectivity:** A mailbox server lets peers meet. They try direct TCP and fall back to a transit relay. Public services are available. Mailbox and transit services can be self-hosted. [Transit protocol](https://github.com/magic-wormhole/magic-wormhole-protocols/blob/main/transit.md), [relay operation](https://github.com/magic-wormhole/magic-wormhole-transit-relay/blob/master/docs/running.md)

**Security, trust, and reach:** SPAKE2 limits an attacker to one online guess per code. Derived keys protect the record pipe. Transit uses authenticated encryption, so relays see metadata but not content. The friend trusts whoever supplied the code. The friend receives offered files, directories, or text, not a shell or network. [Security analysis](https://github.com/magic-wormhole/magic-wormhole-protocols/blob/main/security.md)

**License and maintenance:** MIT and active. [Repository](https://github.com/magic-wormhole/magic-wormhole), [commits](https://github.com/magic-wormhole/magic-wormhole/commits/master/)

## croc

**Invite shape:** croc normally creates a variable three-word code. A QR or web invite can use `https://getcroc.com/?code=<code>`. The code is the PAKE secret and rendezvous value, not a public host address. [croc README](https://github.com/schollz/croc), [web client](https://github.com/schollz/croc/blob/main/web/README.md)

**Friend join:**

1. Open the QR or web invite.

The native path needs an install and one receive command, so it has two steps.

**Connectivity:** Current native croc uses PAKE-bound Tailcat identities and userspace WireGuard. It starts through a DERP relay and promotes to direct UDP when possible. Browser, older, and fallback paths use a croc relay. The relay can be self-hosted. [croc README](https://github.com/schollz/croc)

**Security, trust, and reach:** PAKE derives end-to-end keys from the code. Relays cannot read content. A person with the code can join. A stored transfer is client-encrypted and keeps its key in the URL fragment. The friend receives files, folders, or text, not a shell, service, or network.

**License and maintenance:** MIT. Version 11.3.6 was released in August 2026, and the project is active. [Repository](https://github.com/schollz/croc), [release](https://github.com/schollz/croc/releases/tag/v11.3.6), [commits](https://github.com/schollz/croc/commits/main/)

## Eternal Terminal

**Invite shape:** Eternal Terminal has no capability invite. The friend runs `et user@hostname[:port]`. Existing SSH identity supplies authorization. [README](https://github.com/MisterTea/EternalTerminal)

**Friend join:**

1. Install Eternal Terminal.
2. Run the `et` command.
3. Complete SSH authentication and any first-use host key check.

The owner needs `etserver`, SSH, and reachable TCP port 2022.

**Connectivity:** SSH starts the remote process. The client then makes a direct TCP connection to `etserver`. There is no relay or NAT rendezvous. A host behind NAT needs a port forward. [Protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)

**Security, trust, and reach:** SSH authenticates the host and user. The server creates a client ID and passkey for encrypted terminal packets. There is no guest capability or read-only role. The friend gets a Unix account shell. Local and reverse port forwarding can expose more. [Protocol](https://github.com/MisterTea/EternalTerminal/blob/master/docs/protocol.md)

**License and maintenance:** Apache-2.0. Version 7.0.0 was released in July 2026, and development is active. [Repository](https://github.com/MisterTea/EternalTerminal), [release](https://github.com/MisterTea/EternalTerminal/releases/tag/et-v7.0.0), [commits](https://github.com/MisterTea/EternalTerminal/commits/master/)

## iroh sendme

**Invite shape:** `sendme send` prints a long `blob...` ticket. Size is variable because it serializes the content hash, 256-bit endpoint identity, and route data. It is normally hundreds of characters and is for copy and paste. It contains public identity and location data, not a private key. [README](https://github.com/n0-computer/sendme), [iroh endpoint docs](https://docs.rs/iroh/latest/iroh/index.html)

**Friend join:**

1. Install sendme.
2. Run `sendme receive <ticket>`.

**Connectivity:** iroh tries direct QUIC with NAT traversal and uses an encrypted relay when direct connection fails. N0 runs public relays and an owner can self-host a relay. [Relay docs](https://docs.iroh.computer/concepts/relays)

**Security, trust, and reach:** QUIC TLS authenticates the endpoint public key and encrypts content end to end. A relay cannot decode it. sendme does not identify receivers, so anyone with the ticket can fetch while the sender serves it. The content hash verifies data. The ticket grants only the named file or directory. [iroh endpoint docs](https://docs.rs/iroh/latest/iroh/index.html)

**License and maintenance:** Apache-2.0 OR MIT. Version 0.36.0 was released in June 2026, and the project is active. [Repository](https://github.com/n0-computer/sendme), [release](https://github.com/n0-computer/sendme/releases/tag/v0.36.0), [commits](https://github.com/n0-computer/sendme/commits/main/)

## iroh dumbpipe

**Invite shape:** `dumbpipe listen` prints an `endpoint...` ticket. The README example is 100 characters, but tickets can grow with route data. The ticket has the endpoint's public identity and addresses, not a private key. [README](https://github.com/n0-computer/dumbpipe), [source](https://github.com/n0-computer/dumbpipe/blob/main/src/main.rs)

**Friend join:**

1. Install dumbpipe.
2. Run `dumbpipe connect <ticket>`.

**Connectivity:** It uses iroh direct QUIC, hole punching, and relay fallback. Public and self-hosted relays are possible. [Relay docs](https://docs.iroh.computer/concepts/relays)

**Security, trust, and reach:** QUIC TLS gives endpoint authentication and end-to-end encryption, so the relay is blind. The sample listener accepts any peer with the correct application protocol. The ticket is a hard-to-guess locator, but dumbpipe adds no client allowlist or single-use secret. The forwarded service must authenticate the friend. One mode forwards one standard stream. Other modes expose one configured TCP or Unix service. [iroh endpoint docs](https://docs.rs/iroh/latest/iroh/index.html), [source](https://github.com/n0-computer/dumbpipe/blob/main/src/main.rs)

**License and maintenance:** MIT OR Apache-2.0. Version 0.39.0 was released in June 2026, and the project is active. [Repository](https://github.com/n0-computer/dumbpipe), [release](https://github.com/n0-computer/dumbpipe/releases/tag/v0.39.0), [commits](https://github.com/n0-computer/dumbpipe/commits/main/)

## Join UX ranking

This table includes required installs and sign-in. It assumes the friend has a browser and macOS OpenSSH. Ties favor fewer prerequisites, then narrower default reach. Unsafe anonymous variants are labeled.

| Rank | Product and path | Steps | Prerequisites | Main compromise |
| ---: | --- | ---: | --- | --- |
| 1 | sshx browser | 1 | Browser | No person identity; hosted production service only |
| 2 | tmate web or SSH | 1 | Browser or OpenSSH | Hosted relay can read the terminal |
| 3 | Upterm SSH | 1 | OpenSSH | Opaque command; normal mode gives a shell |
| 4 | tty-share browser | 1 | Browser | Hosted proxy can read the terminal |
| 5 | croc web receive | 1 | Browser | File and text transfer only |
| 6 | ttyd anonymous | 1 | Browser and owner port exposure | Unsafe for an Internet shell without added auth |
| 7 | dev tunnel anonymous web | 1 | Browser and owner Microsoft setup | Microsoft can inspect HTTP |
| 8 | Live Share anonymous web | 1 to 2 | Browser and host approval policy | Large collaboration surface |
| 9 | Warp browser | 2 | Browser and Warp account | Account required; service stores scrollback |
| 10 | Live Share identified web | 2 | Browser and GitHub or Microsoft account | Large collaboration surface |
| 11 | Private dev tunnel web | 2 | Browser and accepted Microsoft identity | Hosted relay terminates HTTPS |
| 12 | Magic Wormhole | 2 | Install and receive command | File and text transfer only |
| 13 | croc native | 2 | Install and receive command | File and text transfer only |
| 14 | iroh sendme | 2 | Install and receive command | File only; very long ticket |
| 15 | iroh dumbpipe | 2 | Install and connect command | Long ticket; app needs client auth |
| 16 | ttyd with Basic auth | 2 | Browser and owner port exposure | No NAT solution; password is separate |
| 17 | Mosh | 3 | Install, SSH account, direct UDP | No relay, read-only role, or shared session |
| 18 | Eternal Terminal | 3 | Install, SSH account, direct TCP | No relay or read-only role; broad shell |

## Three designs closest to Seer

### 1. sshx

sshx has the best guest entry. A browser opens one 43-character URL. There is no account or install. The URL fragment holds the end-to-end secret, the relay is blind, and reader and writer capabilities are separate.

Seer should copy the split between a short routing ID and a client-held secret. It should also copy separate view and control invites. It should not copy the browser terminal or unsupported production self-hosting.

### 2. Upterm

Upterm is the closest terminal transport. The owner makes one outbound reverse connection. The friend pastes one SSH command with tools already on macOS. The relay can be self-hosted, the host can approve a key, and a forced command can limit reach to Seer.

Seer should copy the ready SSH invite, outbound-only owner connection, and optional host approval. It should copy `--no-sftp` in principle: unrelated SSH features must be absent, not only undocumented.

### 3. iroh dumbpipe

dumbpipe has the strongest connectivity model for a new Seer transport. A public-key endpoint tries direct QUIC and falls back to a relay that cannot read application data. It forwards one stream or service, not a network. Public and self-hosted relays are possible.

Seer should copy authenticated endpoint identity, direct-to-relay migration, and service-only forwarding. It must not copy the open listener policy or long raw ticket. Seer needs a short, single-use friend capability and host authorization above iroh.

## What this means for Seer

- Keep one pasted line as the primary join path. It is a proven threshold for terminal sharing.
- Put a high-entropy, single-session capability in the invite. Do not use a reusable static token.
- Separate routing data from an end-to-end key, and keep the key hidden from the relay.
- Use one outbound owner connection, direct QUIC when possible, and blind relay fallback.
- Bind the capability to one Seer service and one owner session. Do not create IP routes.
- Give view and control different capabilities. Make view the safer default for cross-user access.
- Add an owner approval option for first use, with a clear friend identity or key fingerprint.
- Package install and connect in the current one-line installer, but verify the downloaded client before it runs.
- Keep transfer, port forwarding, SFTP, and general SSH login out of the first protocol.

## Open questions

- Should the first release use iroh, a small custom QUIC relay, or SSH over a reverse tunnel?
- Can the invite stay short enough for chat while it carries relay location, endpoint identity, and a secret?
- Should a capability be single-use, time-limited, owner-revocable, or all three?
- How does a friend verify the owner before the first encrypted connection?
- Does read-only mean terminal output only, or can the friend also navigate Seer metadata?
- Which relay metadata can Seer retain, and what retention period is acceptable?
- Must the project operate a default relay, or can an owner select a community or self-hosted relay?
