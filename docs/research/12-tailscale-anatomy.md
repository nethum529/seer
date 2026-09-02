# Tailscale anatomy and the parts Seer needs

Research date: 2026-09-02.

## Scope

This note explains the Tailscale connection path from login to application traffic. It separates NAT reachability from machine network
access. It then identifies the smaller set that Seer needs for a built-in connection.

The current Seer design has one Linux broker behind a home NAT. A friend uses macOS. The friend must reach only the Seer service. The
friend must not join a VPN or get general network access to the Linux machine ([current install and join design](../adr/0001-one-line-install-and-join.md)).

## Short answer

Tailscale is not one tunnel. It is a client agent plus several services:

| Component | Main result | Does Seer need an equivalent? |
| --- | --- | --- |
| Machine and node identity | Authenticates a device and its WireGuard key | Yes, but only for Seer identities |
| Coordination service | Exchanges keys, endpoints, routes, DNS, and policy | Yes, but only keys and connection candidates |
| WireGuard data plane | Creates an encrypted IP tunnel | No. Seer needs an encrypted application transport instead |
| NAT traversal | Finds and tests direct UDP paths | Yes |
| DERP | Starts connections and relays when direct UDP fails | Yes, as an application relay |
| Virtual IP and MagicDNS | Makes each device a named network host | No |
| Tailnet sharing and ACLs | Grants network access to machines and ports | No. Seer already owns application authorization |
| Tailscale SSH | Adds identity-based SSH on the host | No |

The NAT traversal and DERP parts provide seamless reachability. The virtual network interface, routes, addresses, names, and network policy
provide machine network access. Seer needs the first group, not the second group.

## Identity

### Machine keys

The client creates one X25519 machine key pair when Tailscale is installed. The private key stays on the device. The machine public key
identifies that installation to the coordination service. The machine key protects and authenticates control communication. It does not
encrypt peer data. Machine keys do not expire and cannot rotate. A new installation gets a new machine key ([node
keys](https://tailscale.com/docs/concepts/node-keys), [Tailscale identity](https://tailscale.com/docs/concepts/tailscale-identity)).

Seer needs a stable device credential after a friend joins. It does not need a second machine-key layer if one Seer device key can
authenticate the client to both coordination and the broker.

### Node keys

At login, the client creates a separate X25519 node key pair. The node public key goes to the coordination service. The node private key
stays on the device. The service binds the node public key to the machine key, the signed-in user, and one tailnet. Other allowed nodes
receive that public key and use it as the WireGuard peer identity ([node key flow](https://tailscale.com/docs/concepts/node-keys)).

Seer needs an end-to-end key for the owner broker and each client. This key must identify a Seer endpoint, not an operating system network
host.

### Login and key exchange

The client proves control of its machine private key to the coordination service. The service returns a login URL for a new node key. The
user completes an OAuth 2.0, OpenID Connect, SAML, or passkey flow. The service then binds the authenticated identity to the node and
returns its authorized network state ([node keys](https://tailscale.com/docs/concepts/node-keys), [control
plane](https://tailscale.com/docs/concepts/control-data-planes)).

Current control traffic uses a custom Noise IK protocol with X25519. It can run on plain TCP or inside TLS. This channel carries key
exchange and device coordination, not user data ([Tailscale encryption](https://tailscale.com/docs/concepts/tailscale-encryption)).

Seer does not need an external identity provider. Its existing one-use capsule can authorize the first join. The capsule must also
authenticate the expected owner broker, or securely bind a broker public key during the first exchange ([current capsule](../../crates/seer/src/capsule.rs)).

### Expiry and rotation

Node keys can rotate. Reauthentication creates a new node key pair and sends the new public key to the coordination service. New tailnets
use a 180-day expiry by default. An administrator can set 1 to 180 days or disable expiry for a device. An expired node cannot send or
receive tailnet traffic until it reauthenticates ([key expiry](https://tailscale.com/docs/features/access-control/key-expiry), [SSH key
rotation](https://tailscale.com/docs/features/tailscale-ssh#rotate-keys)).

WireGuard also creates and rotates short-lived session keys inside each peer connection. That is separate from Tailscale node key expiry.
The node key is the stable peer identity used by the WireGuard handshake ([WireGuard protocol](https://www.wireguard.com/protocol/)).

Seer needs rotation for compromised or replaced device credentials. It does not need periodic browser login. The owner must be able to
revoke a friend or device, and active connections must stop after revocation.

## Control plane

### What the coordination service does

The service keeps a control connection to each active node. It authenticates nodes, stores public keys and device state, calculates access,
assigns overlay addresses, and sends connection state to clients. It is not normally in the peer data path ([control and data
planes](https://tailscale.com/docs/concepts/control-data-planes)).

The main client input is a network map, or netmap. A streamed map poll sends a complete first response and then incremental changes. The
map contains the local node, visible peers, DNS settings, packet filters, and other state
([MapResponse](https://pkg.go.dev/tailscale.com/tailcfg#MapResponse)).

For each visible peer, the map can include:

- The WireGuard node public key.
- Tailscale addresses and allowed routes.
- Direct endpoint candidates from local interfaces and STUN.
- The peer's home DERP region.
- Names, key expiry, online state, and capabilities.

These fields are in the open source control protocol types ([Node](https://github.com/tailscale/tailscale/blob/main/tailcfg/tailcfg.go),
[NetworkMap](https://pkg.go.dev/tailscale.com/types/netmap#NetworkMap)).

The service also sends a DERP map, DNS configuration, and an inbound packet filter compiled from tailnet policy. Policy changes and peer
key revocations reach connected clients through the same stream ([control protocol types](https://github.com/tailscale/tailscale/blob/main/tailcfg/tailcfg.go)).

### What runs in the client

The client agent, normally `tailscaled`, owns the live work:

- It stores private keys and maintains the control connection.
- It reports local and public endpoint candidates.
- It converts netmap peers into WireGuard peer configuration.
- It configures the TUN interface, operating system routes, and DNS.
- It enforces the incoming packet filter on the destination device.
- It probes paths, keeps a DERP home, and moves traffic between paths.

The open source engine connects the TUN device, router, DNS manager, `wireguard-go`, and `magicsock` path manager ([userspace
engine](https://github.com/tailscale/tailscale/blob/main/wgengine/userspace.go),
[magicsock](https://github.com/tailscale/tailscale/blob/main/wgengine/magicsock/magicsock.go)).

Seer needs a small rendezvous service. It must know only the owner broker key, friend client key, current endpoint candidates, relay
location, revocation state, and enough state to prevent replay. It does not need a tailnet netmap, routes, DNS, or general packet filters.

## Data plane

WireGuard authenticates peer node keys and encrypts IP packets end to end. The coordination service and DERP cannot decrypt these packets.
Each client uses the netmap to select allowed peers and routes ([WireGuard white paper](https://www.wireguard.com/papers/wireguard.pdf),
[Tailscale encryption](https://tailscale.com/docs/concepts/tailscale-encryption)).

Tailscale uses its fork of `wireguard-go` on supported platforms. This is the userspace implementation of the WireGuard protocol. On Linux,
it exchanges plain IP packets with the kernel through a TUN device, encrypts them in the Go process, and sends encrypted packets through
UDP or DERP. Tailscale does not replace `wireguard-go` with the Linux WireGuard kernel module ([Tailscale
WireGuard](https://tailscale.com/docs/concepts/wireguard), [userspace data path](https://github.com/tailscale/tailscale/blob/main/wgengine/userspace.go)).

The word "kernel mode" in Tailscale subnet-router documentation refers to kernel packet forwarding. It does not mean that Tailscale uses
the kernel WireGuard implementation. Non-Linux devices and non-root Linux modes can also forward with the userspace netstack ([kernel and
userspace routing](https://tailscale.com/docs/reference/kernel-vs-userspace-routers)).

Seer does not need to carry arbitrary IP packets. It needs one authenticated, encrypted stream for the Seer protocol. QUIC, TLS, or a
Noise-based transport can provide this without a TUN device, route changes, or a VPN permission.

## NAT traversal

### Endpoint discovery

The client uses the same UDP socket for discovery and WireGuard traffic. This is necessary because a NAT can assign a different public
mapping to each socket. The client gathers local interface addresses, configured endpoints, port mappings, and public endpoints learned
through STUN ([magicsock](https://github.com/tailscale/tailscale/blob/main/wgengine/magicsock/magicsock.go),
[endpoint selection](https://github.com/tailscale/tailscale/blob/main/wgengine/magicsock/endpoint.go)).

STUN sends a request to a public server. The response gives the source IP and port that the server observed. Tailscale's DERP nodes also
serve STUN. The client sends probes to several servers so it can see whether the mapping changes with the destination
([STUN](https://tailscale.com/docs/reference/stun-protocol), [RFC 8489](https://www.rfc-editor.org/rfc/rfc8489.html)).

The client can also request an explicit mapping with PCP, NAT-PMP, or UPnP. These methods can make a hard NAT reachable, but the router can
omit or disable them. They are useful candidates, not required assumptions ([firewall ports and mapping
detection](https://tailscale.com/docs/reference/faq/firewall-ports)).

### UDP hole punching

The peers exchange candidate lists through the control and DERP side channel. Each side sends authenticated discovery probes to the other
side's candidates. These simultaneous outbound packets create NAT mappings and firewall state. The clients measure working paths and use
the best one. They keep checking so they can upgrade from relay to direct or recover after a network change ([connection
sequence](https://tailscale.com/docs/reference/connection-types), [ICE candidate model](https://www.rfc-editor.org/rfc/rfc8445.html)).

### NAT types that fail

An easy NAT uses endpoint-independent mapping. One local socket normally keeps the same public mapping for different destinations. A hard
NAT uses endpoint-dependent mapping and can also use unpredictable public ports. The RFC separates mapping behavior from
endpoint-independent, address-dependent, and address-and-port-dependent filtering ([RFC
4787](https://www.rfc-editor.org/rfc/rfc4787.html)).

Direct UDP normally works for two easy NATs. A public endpoint can also reach a hard NAT after the hard side sends first. Tailscale
documents relay use for easy-to-hard and hard-to-hard pairs. Direct traffic also fails when a network blocks UDP. Port mapping can change
these results ([device connectivity matrix](https://tailscale.com/docs/reference/device-connectivity)).

### How netcheck detects the condition

`tailscale netcheck` sends STUN probes to DERP regions. It reports whether UDP, IPv4, and IPv6 work, the observed public endpoints, the
nearest DERP, and DERP latencies. It also reports whether the IPv4 mapping varies by destination and whether UPnP, NAT-PMP, or PCP is
present ([netcheck source](https://github.com/tailscale/tailscale/blob/main/net/netcheck/netcheck.go), [netcheck
CLI](https://github.com/tailscale/tailscale/blob/main/cmd/tailscale/cli/netcheck.go)).

`MappingVariesByDestIP: true` is evidence of a hard mapping. It is not a full classification of every NAT and firewall behavior. End-to-end
path probes are still the final test.

Seer needs this whole reachability loop, but only for its application socket. It needs STUN, candidate exchange, simultaneous probes, path
selection, keepalive, network-change detection, and relay fallback.

## DERP relays

DERP is a relay protocol where clients are addressed by WireGuard public key. It carries authenticated discovery messages and already
encrypted WireGuard packets. A relay can see keys, timing, and packet sizes, but it cannot decrypt the tunneled IP traffic ([DERP
design](https://github.com/tailscale/tailscale/blob/main/derp/README.md)).

A client selects a low-latency home DERP and keeps that connection open. Peers learn the home region through coordination. A new peer
connection starts over DERP while both clients test direct UDP. It moves to a direct path when one works. If UDP is blocked or NAT
traversal fails, it stays on DERP. It can test again and change paths later ([connection
types](https://tailscale.com/docs/reference/connection-types)).

A self-hosted Tailscale DERP needs:

- A maintained `derper` build and stable service host.
- A valid TLS setup and direct TCP service, normally on port 443.
- UDP port 3478 for STUN and TCP port 80 when certificate or probe flows need it.
- Preferably static IPv4 and IPv6 addresses in the DERP map.
- Client DERP-map configuration, monitoring with `derpprobe`, and regular updates.
- No ordinary HTTP reverse proxy or global load balancer in front of it.

These are current requirements in the upstream operator guide ([custom DERP
guide](https://github.com/tailscale/tailscale/blob/main/cmd/derper/README.md), [derper
flags](https://github.com/tailscale/tailscale/blob/main/cmd/derper/derper.go)).

Seer needs a reliable relay, but it does not need the DERP protocol. The relay can carry only encrypted Seer frames. The coordination and
relay roles can run in one public service for the first version, if end-to-end keys keep that service outside the trust boundary for
terminal contents.

## MagicDNS and the 100.64.0.0/10 range

Tailscale assigns each node a stable virtual IPv4 address from `100.64.0.0/10`. RFC 6598 reserves this range as shared address space. It is
not the public endpoint used for NAT traversal. The client installs routes so traffic to a peer's virtual address enters the Tailscale data
plane ([reserved addresses](https://tailscale.com/docs/reference/reserved-ip-addresses), [RFC
6598](https://www.rfc-editor.org/rfc/rfc6598.html)).

MagicDNS maps device names and tailnet names to these virtual addresses. Each client has a local resolver at `100.100.100.100`. The control
plane sends the DNS data, and the client configures the operating system to use it. A search domain permits short names such as `home-pc`
([MagicDNS](https://tailscale.com/docs/features/magicdns), [Quad100](https://tailscale.com/docs/reference/faq/magicdns)).

Seer needs neither feature. A capsule can hold an opaque broker identity and the rendezvous location. The saved server list can hold a
human alias. No application requirement needs a virtual IP, operating system route, or DNS change.

## Sharing model and machine access

### Invite a user

A tailnet user invite adds a person to the network. The user signs in with an identity provider or passkey and adds devices. An invited
member can reach the tailnet resources allowed by policy. With the default policy, the user can reach all devices and services in the
tailnet ([user invites](https://tailscale.com/docs/features/sharing/how-to/invite-any-user), [invite compared with
share](https://tailscale.com/docs/reference/inviting-vs-sharing)).

### Share one node

Node sharing keeps the recipient in a separate tailnet and exposes one shared machine. The recipient still gets network access to that
machine, subject to both tailnets' policies. The share is machine-scoped, not application-scoped ([node
sharing](https://tailscale.com/docs/features/sharing)).

### ACLs and grants

ACLs and grants define who can connect to which destination ports and protocols. The control plane compiles policy and pushes a packet
filter to clients. The destination client enforces it locally. An absent custom ACL gets the default allow-all policy ([ACL
behavior](https://tailscale.com/docs/features/access-control/acls), [grant network
capabilities](https://tailscale.com/docs/features/access-control/grants)).

An owner can restrict a friend to TCP port 7321 on one machine. That removes most general access. The current default or wildcard policy
does not. It lets the friend address every listening service on the allowed machine, including a normal SSH server, web admin service, or
development port.

### Tailscale SSH

Tailscale SSH is optional. The destination opts in. `tailscaled` then claims port 22 on the Tailscale address and authenticates the SSH
client from tailnet identity and SSH policy. Normal SSH outside Tailscale is unchanged ([Tailscale
SSH](https://tailscale.com/docs/features/tailscale-ssh)).

This is still whole-machine shell access. It is not needed for Seer. Seer must authenticate a friend to one broker function and then
enforce its own private workspace and read-only cross-user view rules.

## Current macOS onboarding count

This count uses the recommended standalone client on macOS 15 or later. It starts when the friend receives a user invite and ends when
Tailscale is ready. It counts documented user actions. It does not count every Continue button in Apple's package installer.

1. Open the owner's Tailscale invite URL in a browser.
2. Sign in with an identity provider or passkey to accept the invite.
3. Select the option to download the Tailscale client.
4. Download the recommended standalone macOS package.
5. Open the package and complete the Apple installer.
6. Launch Tailscale and start its onboarding flow.
7. Open System Settings and select General.
8. Open Login Items & Extensions, then open Network Extensions.
9. Turn on Tailscale Network Extension and approve with Touch ID or an administrator password.
10. Select Done and select Allow when macOS asks to add the VPN configuration.
11. Open the Tailscale menu item and select login.
12. Complete the browser login with the same invited identity.

The official invite guide defines the first three actions. The macOS install and system-extension guides define the remaining actions
([accept an invite](https://tailscale.com/docs/features/sharing/how-to/invite-any-user#accept-an-invite), [install on
macOS](https://tailscale.com/docs/install/mac), [authorize the extension](https://tailscale.com/docs/concepts/macos-sysext)).

The current Seer flow then adds two actions: paste the Seer install-and-join line, and press Enter to accept or submit the display name
([current install and join design](../adr/0001-one-line-install-and-join.md)). This is why the Tailscale prerequisite is not the target
one-line friend experience.

## Reachability compared with access

| Goal | Components that provide it |
| --- | --- |
| Seamless connect through NAT | Outbound control connection, endpoint exchange, STUN, optional port mapping, UDP hole punching, path probes, and DERP fallback |
| Encrypted peer transport | WireGuard node keys and session handshakes, whether the path is direct or relayed |
| Network access to the machine | TUN interface, virtual addresses, operating system routes, netmap peers, and packet filters |
| Human-friendly machine access | MagicDNS names and tailnet search domains |
| Shell access to the machine | A reachable normal SSH server, or optional Tailscale SSH and its SSH policy |

WireGuard encryption is not what grants broad access. Broad access appears when the client exposes a general IP interface and policy allows
traffic to machine ports. NAT traversal can exist without that interface, as shown by any application that applies the same candidate and
relay method to one protocol.

## Minimum components for an application

An application that connects two home machines through NAT without a VPN needs:

1. An endpoint identity and an authenticated application transport. Private keys stay on endpoints.
2. A public rendezvous service that both endpoints reach with outbound connections.
3. A way to authorize the pair. For Seer, this can be a one-use invitation capability.
4. STUN on the same UDP socket as application traffic, with more than one observation point.
5. Candidate exchange for local, public, and optional port-mapped endpoints.
6. Authenticated simultaneous probes, path selection, keepalive, and path-change handling.
7. An always-available relay over a widely allowed transport such as TLS on TCP 443.
8. End-to-end encryption on both direct and relayed paths.

The rendezvous and relay can be one deployed service. STUN can run beside it. The endpoint still needs separate logic for discovery,
probing, encryption, and path changes. The relay is required for reliable operation because hard NATs and UDP-blocked networks cannot
always form a direct path.

The application does not need WireGuard, a TUN interface, a virtual address range, system routes, MagicDNS, tailnet membership, network
ACLs, or SSH. Those parts turn connectivity into a private IP network. They are outside Seer's service-only boundary.

## What this means for Seer

- Keep the one-use Seer capsule as the friend authorization method. Do not add an account requirement.
- Bind the capsule to the expected owner broker key so the rendezvous service cannot replace the broker.
- Put NAT discovery and encrypted Seer traffic on the same UDP socket.
- Add a small public rendezvous service for keys, candidates, presence, and revocation.
- Start through an end-to-end encrypted relay, then upgrade to direct UDP when probes succeed.
- Keep the relay available for hard NAT, blocked UDP, and path failure.
- Carry only the Seer protocol. Do not create a TUN interface or change system routes and DNS.
- Keep workspace authorization in the Seer broker. Network reachability must not become user authorization.
- Treat direct and relayed paths as the same secure session so a path change does not change permissions.

## Open questions

- Which encrypted transport best fits the existing Seer protocol and path migration requirements?
- How does the capsule authenticate the owner key while staying short enough for one pasted line?
- How does a broker renew rendezvous presence after restart without making an old capsule reusable?
- Does one relay region meet the latency and availability target for the first release?
- What relay admission rule prevents abuse without requiring a friend account?
- Which Linux home routers, carrier NATs, and macOS network changes form the acceptance test matrix?
- How quickly must an active direct or relayed session stop after device revocation?
