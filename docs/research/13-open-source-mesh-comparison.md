# Open source mesh comparison for Seer

Research date: 2026-09-02.

## Scope

This note compares Headscale with Tailscale clients, NetBird, Netmaker, Nebula, ZeroTier, and innernet for the Seer friend flow.

The required result is strict:

- The owner runs Seer on a Linux PC behind home NAT.
- A non-technical friend uses macOS.
- The friend pastes one line, creates no account, and gets a Seer shell.
- The friend must not get access to SSH or other services on the owner PC.
- Seamless friend access is first, security is second, and owner effort is third.

## Method

- Friend steps count product-specific actions after the owner sends the join material. The count does not include the final Seer attach.
- An install command and a join command are two steps, even if Seer could put both commands in one script.
- A macOS password or VPN approval dialog can add one visible confirmation.
- Scores use 5 for best and 1 for worst.
- The maintenance snapshot uses the latest GitHub release on 2026-09-02.
- There is no public, consistent maintainer list for all six projects. "Active maintainers" is therefore a proxy: distinct human users who merged a pull request from 2026-06-04 through 2026-09-02. Bots are excluded.
- "Critical issues" means open issues with a public label named critical, blocker, release-blocker, P0, or sev-0. A repository without such a label reports zero labeled issues and says that the label is absent. This is not a security audit and does not include private advisories.

## Short answer

NetBird is the best fit if Seer accepts a small Go transport component. Its official embed package runs in userspace, needs no separate NetBird install, and gives the application dial and listen functions. A setup key removes the friend account step. Its main cost is a larger owner-side service stack. The source describes both the userspace mode and the lack of a separate client install ([NetBird embed source](https://github.com/netbirdio/netbird/blob/a1415dbc05759e7871f61f5746f5c329b22fa914/client/embed/doc.go)).

Headscale is a close second. Tailscale's tsnet library also provides a userspace network node inside one Go program, and its ControlURL field can select Headscale. Headscale pre-authentication keys give non-interactive join. The integration is not native Rust, but it can stay behind a Seer helper binary ([tsnet source](https://github.com/tailscale/tailscale/blob/91d10d38ae58b8aa88db9e951342a00b76cbb625/tsnet/tsnet.go), [Headscale registration](https://headscale.net/stable/ref/registration/)).

No unchanged desktop VPN client meets the full requirement. Each normal client creates a network interface or virtual network on the friend's Mac. Policy can reduce reachable destinations, but the friend still installs and joins another network product.

## Ranked comparison

| Rank | Product | Seamless for friend | Security | Owner effort | Embeddable in Seer | Result |
| ---: | --- | ---: | ---: | ---: | ---: | --- |
| 1 | NetBird | 5 | 5 | 2 | 4 | Best hidden-client path. Official userspace Go package and port policies. |
| 2 | Headscale and Tailscale | 5 | 5 | 3 | 4 | Mature userspace library and exact-port grants. Headscale has a small maintainer group. |
| 3 | Nebula | 2 | 5 | 3 | 2 | Strong host firewall, but manual PKI and a system tunnel remain visible. |
| 4 | ZeroTier | 4 | 4 | 2 | 3 | Good userspace SDK, but the self-hosted controller is not open source. |
| 5 | innernet | 2 | 2 | 4 | 4 | Native Rust library, but no relay and no port-level policy. |
| 6 | Netmaker | 2 | 2 | 1 | 2 | Heavy owner stack. Community ACLs select peers, not one service. |

Owner effort scores include the public control and relay infrastructure. The embeddable score includes both API fit and license fit. Development cost is not a primary score.

## Headscale with Tailscale clients

### Architecture

- Headscale is a self-hosted implementation of the Tailscale control server. It stores users, nodes, routes, keys, and policy. It is designed for one small tailnet ([Headscale overview](https://headscale.net/)).
- Tailscale clients form direct peer-to-peer tunnels with WireGuard. The control server distributes public keys, endpoints, routes, and packet filters. User traffic does not pass through the control server ([control and data planes](https://tailscale.com/docs/concepts/control-data-planes)).
- DERP relays encrypted WireGuard packets when a direct path is not possible. Headscale has an optional embedded DERP server. It can also publish other DERP servers ([Headscale DERP](https://headscale.net/stable/ref/derp/)).
- A device has machine and node key pairs. Headscale assigns personal nodes to a Headscale user and can assign service nodes to tags. Private keys stay on the node ([Tailscale identity](https://tailscale.com/docs/concepts/tailscale-identity), [Headscale registration](https://headscale.net/stable/ref/registration/)).

### License and embedding

- The Headscale server and Tailscale client use BSD-3-Clause ([Headscale license](https://github.com/juanfont/headscale/blob/cbe30304dce74fa98733ef744b2ec21ac91dd933/LICENSE), [Tailscale license](https://github.com/tailscale/tailscale/blob/91d10d38ae58b8aa88db9e951342a00b76cbb625/LICENSE)).
- The licenses permit binary redistribution with their notice conditions.
- tsnet is a supported Go library that embeds a userspace Tailscale node. It can listen only for Seer and does not need a system-wide TUN interface. Its ControlURL and AuthKey fields support Headscale and non-interactive join ([tsnet API](https://tailscale.com/docs/reference/tsnet-server-api), [tsnet source](https://github.com/tailscale/tailscale/blob/91d10d38ae58b8aa88db9e951342a00b76cbb625/tsnet/tsnet.go)).
- Seer is Rust, so tsnet cannot be linked as a normal Rust crate. Seer would need a Go sidecar, a C bridge, or a separate Go transport executable.

### Friend onboarding: 2 standard steps

1. Install the official Tailscale macOS client.
2. Run `tailscale up` with the Headscale URL and the one-use pre-authentication key.

The owner creates the Headscale user and key. The friend creates no account. Headscale documents the complete non-interactive command ([registration methods](https://headscale.net/stable/ref/registration/)). A normal macOS install can also show a system VPN approval.

### Access and owner cost

- Without a policy file, Headscale allows all tailnet traffic. A normal client can therefore reach every listening port on an allowed overlay host. A subnet is reachable only if a node advertises that route ([Headscale policy](https://headscale.net/stable/ref/acls/)).
- A grant can select the friend, the owner node, TCP, and only the Seer port. Grant syntax supports exact protocol and port values ([grant syntax](https://tailscale.com/docs/reference/syntax/grants)).
- With tsnet on both ends, Seer can expose only its own listener. This is stronger than a host-wide network interface.
- Headscale requires a Linux or BSD server with a public IP, HTTPS on TCP 443, and usually a domain. Embedded DERP also needs public TCP 443 and UDP 3478. A small VPS is the normal answer. A stable home public IP with port forwarding can replace it ([Headscale requirements](https://headscale.net/stable/setup/requirements/)).

## NetBird

### Architecture

- NetBird has client, Management, Signal, and Relay components. Management stores peer WireGuard public keys, network state, identity, groups, and policies. Signal exchanges encrypted connection candidates and carries no user traffic ([NetBird architecture](https://docs.netbird.io/about-netbird/how-netbird-works)).
- Peers use direct WireGuard tunnels after ICE-style path discovery. Relay forwards encrypted WireGuard traffic when direct connection fails ([NetBird architecture](https://docs.netbird.io/about-netbird/how-netbird-works), [relay metrics](https://docs.netbird.io/selfhosted/observability/relay)).
- A peer generates its WireGuard key locally. It registers through an identity provider or a setup key. The private key stays on the peer ([NetBird architecture](https://docs.netbird.io/about-netbird/how-netbird-works)).
- New self-host installs can combine Management, Signal, Relay, STUN, and an embedded identity provider in one server container ([configuration files](https://docs.netbird.io/selfhosted/maintenance/configuration-files)).

### License and embedding

- Client code is BSD-3-Clause. The management, signal, relay, and combined server directories are AGPL-3.0 ([license map](https://github.com/netbirdio/netbird/blob/a1415dbc05759e7871f61f5746f5c329b22fa914/LICENSE), [management license](https://github.com/netbirdio/netbird/blob/a1415dbc05759e7871f61f5746f5c329b22fa914/management/LICENSE)).
- The client license permits redistribution with notice conditions.
- The official `client/embed` Go package runs in userspace and does not require a separate NetBird client install. It gives the host program Dial, ListenTCP, and ListenUDP operations ([embed package](https://github.com/netbirdio/netbird/blob/a1415dbc05759e7871f61f5746f5c329b22fa914/client/embed/doc.go)).
- Seer still needs a Go sidecar or bridge because this is not a Rust crate.

### Friend onboarding: 2 standard steps

1. Install the NetBird macOS client.
2. Run `netbird up` with a setup key and the self-hosted Management URL.

The friend needs no NetBird account when the owner supplies a setup key. The official install guide documents setup-key registration and the self-hosted URL option ([NetBird install](https://docs.netbird.io/get-started/install)). A normal macOS install can also show a system VPN approval.

### Access and owner cost

- The default policy permits all peers to communicate on all protocols. A normal client therefore gets host-level overlay access until the owner removes that policy ([NetBird access policy](https://docs.netbird.io/manage/access-control/manage-network-access)).
- Policies select source and destination groups, direction, protocol, and ports. The owner can allow the friend group to reach the owner peer only on the Seer TCP port ([NetBird access policy](https://docs.netbird.io/manage/access-control/manage-network-access)).
- The embed package can avoid a system interface and give Seer only a userspace listener. This is the preferred form for Seer.
- Self-hosting needs a Linux VM with 1 CPU and 2 GB RAM, a public domain and IP, Docker Compose, public TCP 80 and 443, and UDP 3478. The owner maintains the combined server, dashboard, data volume, TLS, and backups ([self-host quickstart](https://docs.netbird.io/selfhosted/selfhosted-quickstart)).

## Netmaker

### Architecture

- The Netmaker server is the control API. It stores network and node state and publishes changes through an MQ broker. Netclient configures WireGuard and subscribes to those updates ([Netmaker architecture](https://docs.netmaker.io/docs/about/architecture)).
- Data normally goes directly between WireGuard peers. A gateway or relay can forward traffic when selected. Current automatic relay and failover features are listed as Pro features ([Netmaker networking functions](https://docs.netmaker.io/docs/operations-guide/giving-the-netclient-networking-functions), [Netmaker features](https://docs.netmaker.io/docs/features)).
- Netclient generates WireGuard keys. An enrollment key tells the server which networks the new host can join ([deploying netclient](https://docs.netmaker.io/docs/operations-guide/deploying-the-netclient)).

### License and embedding

- Community server code is Apache-2.0. Code under `pro/` has a separate license. Netclient is Apache-2.0 ([server license](https://github.com/gravitl/netmaker/blob/9b0f07fd351511a89ad5cdbf67b84c4dc0285bb4/LICENSE.md), [client license](https://github.com/gravitl/netclient/blob/8e6ae583d1b16fd9bb9585b26eeb869f10b6c179/LICENSE.txt)).
- Seer can redistribute netclient with the Apache notices. Netclient is a privileged system daemon, not a documented application socket library. It would remain a separate process ([netclient commands](https://github.com/gravitl/netclient/blob/8e6ae583d1b16fd9bb9585b26eeb869f10b6c179/README.md)).

### Friend onboarding: 2 standard steps

1. Install netclient and its daemon on macOS.
2. Run `netclient join -t <token>` with the owner's enrollment token.

The friend creates no account. The enrollment token contains the server and network join data ([advanced installation](https://docs.netmaker.io/docs/client-installation/advanced-netclient-installation)).

### Access and owner cost

- Community ACLs allow or deny communication between pairs of hosts. They do not select a port. The friend gets access to all services on each allowed overlay host unless the owner also configures the host firewall ([community ACLs](https://docs.netmaker.io/docs/features/acls), [nmctl ACLs](https://docs.netmaker.io/docs/references/nmctl)).
- The owner must run the server, UI, MQ broker, proxy, database, and a netclient on the owner host. The quick install requires Ubuntu, a public static IP, DNS, and public TCP 80 and 443 plus WireGuard UDP ports. A VPS is the normal deployment ([quick install](https://docs.netmaker.io/docs/server-installation/quick-install)).

## Nebula

### Architecture

- Nebula is a custom layer 3 overlay based on the Noise Protocol Framework. It uses direct encrypted UDP tunnels and optional NAT hole punching. It is not WireGuard ([Nebula technical details](https://nebula.defined.net/docs/)).
- A lighthouse is a discovery server. It tracks underlay addresses but does not carry normal data. Any Nebula host can also act as a relay for peers that cannot connect directly ([Nebula quick start](https://nebula.defined.net/docs/guides/quick-start/), [Nebula relay](https://nebula.defined.net/docs/config/relay/)).
- An offline CA signs each host certificate. A certificate fixes the host's overlay IP, name, and groups. Each host keeps its own private key ([Nebula PKI](https://nebula.defined.net/docs/config/pki/)).

### License and embedding

- Client, lighthouse, relay, and certificate tool are one MIT-licensed code base ([Nebula license](https://github.com/slackhq/nebula/blob/dd8f660c0ac37903ec4080ca4d3c861ba9342ceb/LICENSE)).
- Seer can ship the binary. Nebula has no documented userspace socket API for application embedding. The normal client creates a TUN interface, reads a config and three PKI files, and runs as a service ([Nebula introduction](https://nebula.defined.net/docs/), [example config](https://github.com/slackhq/nebula/blob/dd8f660c0ac37903ec4080ca4d3c861ba9342ceb/examples/config.yml)).

### Friend onboarding: 3 standard steps

1. Install Nebula on macOS.
2. Receive the CA certificate, host certificate, private key, and config from the owner as one protected bundle.
3. Put the bundle in the configured paths and start the Nebula service.

There is no account. The official flow requires the owner to sign the host certificate and copy the files to the host ([Nebula quick start](https://nebula.defined.net/docs/guides/quick-start/)).

### Access and owner cost

- The Nebula host firewall is deny by default. Rules select certificate name, group, CA, CIDR, protocol, and port. The owner can allow the friend's group to reach only the Seer TCP port ([Nebula firewall](https://nebula.defined.net/docs/config/firewall/)).
- A normal client still has a host overlay interface. The Nebula firewall, not application embedding, supplies the service boundary.
- At least one lighthouse needs a stable, public UDP address. A relay also needs a public IP and inbound UDP. One small VPS can fill both roles. A home host can work with stable addressing and UDP port forwarding. The owner also maintains the offline CA, host certificates, configs, and revocation data ([Nebula quick start](https://nebula.defined.net/docs/guides/quick-start/), [Nebula relay](https://nebula.defined.net/docs/config/relay/)).

## ZeroTier

### Architecture

- ZeroTier has an encrypted peer-to-peer VL1 network and a VL2 virtual Ethernet layer. Most traffic is direct. ZeroTier-operated roots help peers find paths and can relay slow fallback traffic ([ZeroTier repository](https://github.com/zerotier/ZeroTierOne/blob/899352e38405968516bb12a770f0ac02f6058fa8/README.md), [protocol](https://docs.zerotier.com/protocol/)).
- Each virtual network has a controller that admits members, issues membership certificates, assigns addresses and routes, and distributes rules ([controller](https://docs.zerotier.com/what-is-a-controller/)).
- Each node has a cryptographic identity. Its short public identity is the node address that the controller authorizes ([ZeroTier Sockets identities](https://docs.zerotier.com/sockets/#identities)).

### License and embedding

- The agent and core are MPL-2.0. The bundled self-host controller under `nonfree/` uses the ZeroTier Source-Available License. That license says the controller is not open source and requires a commercial license when it is incorporated into a product or used commercially ([license map](https://github.com/zerotier/ZeroTierOne/blob/899352e38405968516bb12a770f0ac02f6058fa8/LICENSE.txt), [controller license](https://github.com/zerotier/ZeroTierOne/blob/899352e38405968516bb12a770f0ac02f6058fa8/nonfree/LICENSE.md)).
- ZeroTier Sockets embeds a userspace node and socket stack. It has C, Rust, Python, C#, and Java bindings. This is a strong technical fit for a Rust client, subject to license review ([ZeroTier Sockets](https://docs.zerotier.com/sockets/)).

### Friend onboarding: 2 standard steps

1. Install the ZeroTier macOS client.
2. Enter the 16-digit network ID and join.

The owner then authorizes the node on the private controller. The friend needs no account for a self-hosted controller ([client configuration](https://docs.zerotier.com/config/), [join flow](https://docs.zerotier.com/start/)). A normal macOS install can also show a system VPN approval.

### Access and owner cost

- A joined node gets a virtual Ethernet interface. Broad default rules give it network access, not service-only access. Distributed rules can match source and destination node, IP protocol, and destination port. The owner can allow only the Seer TCP port ([rules engine](https://docs.zerotier.com/rules/)).
- ZeroTier Sockets can instead expose only sockets used by Seer.
- A self-hosted controller runs with Internet access over UDP 9993. If the owner continues to use ZeroTier roots and fallback relay, the controller can be one small process. A fully independent deployment also needs public root and relay infrastructure. The controller license remains an owner and distribution concern ([controller deployment](https://docs.zerotier.com/what-is-a-controller/), [ZeroTier repository](https://github.com/zerotier/ZeroTierOne/blob/899352e38405968516bb12a770f0ac02f6058fa8/README.md)).

## innernet

### Architecture

- innernet has one coordination server. It assigns peers, records endpoints, and distributes peer configuration. Data travels directly between peers through WireGuard ([innernet README](https://github.com/tonarino/innernet/blob/1ba6154b6ebacd68dfe79c3a4f6273fd3e8dea35/README.md)).
- There is no relay. The server uses the endpoint that it observes, and an administrator can override an endpoint when discovery fails ([innernet README](https://github.com/tonarino/innernet/blob/1ba6154b6ebacd68dfe79c3a4f6273fd3e8dea35/README.md)).
- A single-use invitation lets a peer contact the server. The new peer makes a WireGuard key pair and replaces the invitation key. The server assigns the peer to one CIDR ([innernet README](https://github.com/tonarino/innernet/blob/1ba6154b6ebacd68dfe79c3a4f6273fd3e8dea35/README.md)).

### License and embedding

- Client and server use MIT ([innernet license](https://github.com/tonarino/innernet/blob/1ba6154b6ebacd68dfe79c3a4f6273fd3e8dea35/LICENSE)).
- Version 2.0 publishes `innernet-client-core` as a Rust library. It can compile into Seer and manage an innernet network interface. It still depends on the operating system WireGuard interface and is not an application-only socket stack ([client-core manifest](https://github.com/tonarino/innernet/blob/1ba6154b6ebacd68dfe79c3a4f6273fd3e8dea35/client-core/Cargo.toml), [v2.0.0 release](https://github.com/tonarino/innernet/releases/tag/v2.0.0)).

### Friend onboarding: 3 standard steps

1. Install innernet and its WireGuard runtime on macOS.
2. Receive the one-use invitation file from the owner.
3. Run `sudo innernet install <invitation-file>`.

There is no account. The official macOS package uses Homebrew, and the install command needs the invitation file ([innernet installation](https://github.com/tonarino/innernet/blob/1ba6154b6ebacd68dfe79c3a4f6273fd3e8dea35/README.md#installation)).

### Access and owner cost

- Access control is by CIDR membership and CIDR-to-CIDR association. Peers in one CIDR can reach each other and the infrastructure CIDR by default. There is no protocol or port rule. The owner needs a host firewall to expose only Seer ([innernet associations](https://github.com/tonarino/innernet/blob/1ba6154b6ebacd68dfe79c3a4f6273fd3e8dea35/README.md#adding-associations-between-cidrs)).
- The owner runs one coordination server and forwards its WireGuard listen port through the home router. A VPS is optional. Hard NAT between peers can still prevent data transfer because there is no relay ([innernet server setup](https://github.com/tonarino/innernet/blob/1ba6154b6ebacd68dfe79c3a4f6273fd3e8dea35/README.md#server-creation)).

## Maintenance state in 2026

| Product | Last release | Active maintainers | Open critical issues |
| --- | --- | ---: | --- |
| Headscale and Tailscale | [Headscale v0.29.3, 2026-07-29](https://github.com/juanfont/headscale/releases/tag/v0.29.3); [client v1.102.3, 2026-08-20](https://github.com/tailscale/tailscale/releases/tag/v1.102.3) | [2 control](https://github.com/juanfont/headscale/pulls?q=is%3Apr+is%3Amerged+merged%3A%3E%3D2026-06-04); [46 client](https://github.com/tailscale/tailscale/pulls?q=is%3Apr+is%3Amerged+merged%3A%3E%3D2026-06-04) | [0 client release-blockers](https://github.com/tailscale/tailscale/issues?q=is%3Aissue+state%3Aopen+label%3Arelease-blocker); Headscale has [no critical-class label](https://github.com/juanfont/headscale/labels) |
| NetBird | [v0.77.1, 2026-08-21](https://github.com/netbirdio/netbird/releases/tag/v0.77.1) | [13](https://github.com/netbirdio/netbird/pulls?q=is%3Apr+is%3Amerged+merged%3A%3E%3D2026-06-04) | [0 P0](https://github.com/netbirdio/netbird/issues?q=is%3Aissue+state%3Aopen+label%3AP0) |
| Netmaker | [server v1.7.0, 2026-08-31](https://github.com/gravitl/netmaker/releases/tag/v1.7.0); [client v1.7.0, 2026-08-31](https://github.com/gravitl/netclient/releases/tag/v1.7.0) | [2 across server](https://github.com/gravitl/netmaker/pulls?q=is%3Apr+is%3Amerged+merged%3A%3E%3D2026-06-04) [and client](https://github.com/gravitl/netclient/pulls?q=is%3Apr+is%3Amerged+merged%3A%3E%3D2026-06-04) | 0 labeled; [server](https://github.com/gravitl/netmaker/labels) and [client](https://github.com/gravitl/netclient/labels) have no critical-class label |
| Nebula | [v1.11.1, 2026-08-21](https://github.com/slackhq/nebula/releases/tag/v1.11.1) | [5](https://github.com/slackhq/nebula/pulls?q=is%3Apr+is%3Amerged+merged%3A%3E%3D2026-06-04) | 0 labeled; [no critical-class label](https://github.com/slackhq/nebula/labels) |
| ZeroTier | [1.16.2, 2026-05-28](https://github.com/zerotier/ZeroTierOne/releases/tag/1.16.2) | [1](https://github.com/zerotier/ZeroTierOne/pulls?q=is%3Apr+is%3Amerged+merged%3A%3E%3D2026-06-04) | [0 P0](https://github.com/zerotier/ZeroTierOne/issues?q=is%3Aissue+state%3Aopen+label%3AP0) |
| innernet | [v2.0.0, 2026-07-02](https://github.com/tonarino/innernet/releases/tag/v2.0.0) | [4](https://github.com/tonarino/innernet/pulls?q=is%3Apr+is%3Amerged+merged%3A%3E%3D2026-06-04) | 0 labeled; [no critical-class label](https://github.com/tonarino/innernet/labels) |

All six projects had a release or merged work in 2026. Release recency alone does not remove the architecture, policy, or license limits above.

## Can Seer hide the mesh product completely?

Yes, for two open-source choices, with an integration change:

- NetBird can be hidden through its official userspace Go embed package. The Seer line can download one Seer bundle, use a one-use setup key, and open only Seer's userspace listener. The friend needs no account, separate app, VPN menu, or system network interface.
- Headscale can be hidden through Tailscale tsnet. The Seer line can use a one-use Headscale pre-authentication key and a fixed Headscale ControlURL. The friend needs no Tailscale account or separate client.
- ZeroTier Sockets can also hide a client and has Rust bindings. It is not an open-source end-to-end answer because the self-hosted controller is under a source-available license with commercial restrictions.
- innernet can hide its name inside the Seer binary, but it cannot hide the WireGuard interface, elevated network changes, or lack of relay.
- Nebula and Netmaker can be bundled, but their supported clients remain separate system tunnel processes.

The hidden paths do not mean zero owner infrastructure. NetBird still needs its public combined server. Headscale still needs its public control server and a reliable DERP option.

## What this means for Seer

- Prototype NetBird `client/embed` and Headscale plus tsnet before designing a new transport protocol.
- Use a userspace listener on the owner and a userspace dialer on the friend. Do not create a host-wide VPN interface.
- Keep the Seer capsule as the only friend-visible credential.
- Exchange the capsule for a short-lived, single-use mesh enrollment key. Do not put a reusable setup key in the install line.
- Bind the enrollment identity to one Seer invitation and revoke it after claim.
- Permit only the Seer service port even when the embedded data plane already limits socket use. This gives defense in depth.
- Plan for one small public VPS. NAT traversal without a reliable relay does not meet the friend success goal.
- Reject Netmaker and innernet for the current goal because their community policy models do not enforce one-service access by themselves.
- Do not select ZeroTier without a license decision for its controller.

## Open questions

- Does NetBird guarantee compatibility for `client/embed`, or is its public Go API still allowed to change without notice?
- Does tsnet with Headscale pass all required macOS arm64 and x86_64 cases for the Tailscale client versions that Headscale supports?
- Can either Go option be built as a stable static helper for Seer's three release targets without dynamic library dependencies?
- How will the broker create, deliver, consume, and revoke one-use setup keys without exposing a reusable control-plane credential?
- Can the owner run control and relay on one low-cost VPS without making that VPS a single point that prevents new joins after an outage?
- What recovery text should Seer show when direct transport and relay both fail?
