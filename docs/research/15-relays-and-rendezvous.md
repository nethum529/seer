# Relays and rendezvous

Research date: 2026-09-02.

## Scope

This note covers how a friend finds an owner and how the connection continues
when NAT traversal fails. It compares rendezvous, relay, tunnel, port mapping,
and direct IPv6 options. It does not select a full transport library or change
the Seer protocol.

Seer has these fixed product constraints:

- The owner runs the broker on a Linux PC at home.
- The friend uses macOS or Linux and is not assumed to be a developer.
- The friend pastes one install-and-join line and does not create an account.
- Seer exposes only the Seer service. It does not give access to the machine's
  other ports.
- The current capsule contains an endpoint and a seat token
  (`crates/seer/src/capsule.rs`).
- The current start flow prefers a Tailscale address and otherwise publishes a
  host or local address (`crates/seer/src/start.rs`, function `published_host`).

## Short answer

Use a small Seer rendezvous service and a Seer relay as one combined public
entry point. The owner keeps one outbound connection to it. The invitation
capsule identifies the owner, the seat, and the service. The friend connects
to the relay first. Both endpoints then try to upgrade to a direct path. If
the upgrade fails, the same encrypted session stays on the relay.

This is the proven relay-first pattern used by libp2p DCUtR, iroh, and
Tailscale. Libp2p starts through a relay and coordinates the direct upgrade
over that connection
([libp2p hole punching](https://github.com/libp2p/specs/blob/master/connections/hole-punching.md)).
Tailscale also starts relayed and then tries a direct connection
([Tailscale connection types](https://tailscale.com/docs/reference/connection-types)).

The relay must carry Seer ciphertext, not plain terminal data. The relay can
then see connection metadata but cannot read terminal input or output. Iroh
documents the same boundary for its public relays
([iroh public relay privacy](https://docs.iroh.computer/iroh-services/relays/public)).

## Connectivity model

Address discovery and data transport are different jobs:

1. Rendezvous tells the friend where the owner can be reached now.
2. A relay gives both peers a path that works with outbound connections.
3. STUN or equivalent observation finds each NAT's public address and port.
4. Direct connection checks try host, public, mapped, and relay candidates.
5. The session moves to the best working path without changing identity.

STUN can discover a NAT mapping, but STUN alone is not a NAT traversal
solution
([RFC 8489, section 13](https://www.rfc-editor.org/rfc/rfc8489.html#section-13)).
ICE separates host, server-reflexive, and relayed candidates and gives direct
candidates a higher preference than relay candidates
([RFC 8445, section 5.1](https://www.rfc-editor.org/rfc/rfc8445.html#section-5.1)).

## Rendezvous options

| Option | Owner work | Friend work | Strength | Main limit |
| --- | --- | --- | --- | --- |
| Public rendezvous service | Seer connects and refreshes a short-lived record. | The pasted capsule selects the record. | Fast, simple, and works when addresses change. | The service sees lookup and presence metadata. |
| DNS records | The owner controls a domain and updates A, AAAA, SRV, or SVCB records. | Normal DNS lookup. | Standard tools and caching. | Domain setup, update delay, and stale records add owner work. |
| DHT | Seer bootstraps, publishes, refreshes, and queries signed records. | The client joins or queries the DHT. | No single rendezvous operator. | More network code, slower lookup, bootstrap peers, and public metadata. |
| pkarr | Seer signs DNS packets and publishes them to Mainline DHT, directly or through a pkarr relay. | Resolve by the owner's public key. | The record is self-authenticating. | NAT clients can still need a pkarr relay and periodic republishing. |
| Code word service | The owner allocates a short mailbox name and words. | The friend types or pastes the code. | Good for spoken or manually typed invitations. | It adds a mailbox service and low-entropy code protocol. |
| Address in capsule | Seer writes current direct and relay addresses into the invite. | No lookup before the first dial. | No separate lookup for stable addresses. | It becomes stale and cannot coordinate new NAT mappings. |

### Public rendezvous service

A purpose-built service is the smallest fit. It can store a signed,
short-lived record with relay choices and observed candidates. Libp2p prior
art lets a peer register only itself with a signed record and bounded lifetime
([libp2p rendezvous specification](https://github.com/libp2p/specs/blob/master/rendezvous/README.md)).
The record does not need the seat secret or terminal data.

### DNS

DNS SRV maps a service name to a host and port
([RFC 2782](https://www.rfc-editor.org/rfc/rfc2782.html)). SVCB can also bind
port and application protocol
([RFC 9460](https://www.rfc-editor.org/rfc/rfc9460.html)). DNS is useful for a
stable relay, but changing peer candidates need dynamic updates and short TTLs.

### DHT and pkarr

Libp2p Kademlia uses replicated values and iterative lookups, and it needs
known peers before bootstrap
([libp2p Kademlia specification](https://github.com/libp2p/specs/blob/master/kad-dht/README.md)).
Pkarr instead publishes a signed DNS packet of at most 1000 bytes to Mainline
DHT, directly or through a relay. Records need periodic republishing
([pkarr design](https://github.com/pubky/pkarr)). Iroh provides this type of
lookup ([iroh address lookups](https://github.com/n0-computer/iroh-address-lookups)).
Both options add bootstrap and refresh work that this MVP does not need.

### Code words

Magic Wormhole uses a nameplate and words to select a server mailbox
([mailbox protocol](https://github.com/magic-wormhole/magic-wormhole-protocols/blob/main/server-protocol.md)).
PAKE protects the exchange
([protocol introduction](https://magic-wormhole.readthedocs.io/en/latest/introduction.html)).
This helps spoken codes. Seer pastes a line, so an opaque capsule is simpler.

### Address inside the capsule

Keep bootstrap data in the capsule, but do not make it the only source of
live addresses. The capsule should contain:

- A format version.
- The owner's stable public key or server ID.
- The one-use seat token.
- One rendezvous URL.
- One or more relay hints for initial contact.

The capsule should not contain a permanent person credential. It should not
depend on a home public address staying unchanged.

## Relay options

| Relay choice | Cost | Owner requirement | Service and trust result |
| --- | --- | --- | --- |
| n0 iroh public relays | No charge for development and hobby use. | Use the iroh endpoint and relay protocol. | No SLA, latest stable release only, and rate limits. Relay sees metadata but not encrypted content. |
| libp2p circuit relays | Protocol software has no license fee. Public capacity is not guaranteed. | Use libp2p identities, reservations, discovery, and relay transport. | Reservations can expire. A relay can set duration and byte limits. |
| Tailscale DERP | Included in a Tailscale plan. Personal use can be free. | Both peers join one tailnet and run Tailscale. | Strong fallback, but it keeps the account and VPN dependency Seer must remove. |
| Self-hosted Seer relay | About USD 6 to 7 per month for one small VPS, plus excess transfer. | The project or owner operates a public server. | Full policy control. The operator still sees metadata and pays for relayed bytes. |
| No relay | No relay bill. | The owner must have a working direct, mapped, or punched path. | Some valid network pairs cannot connect at all. |

Iroh public relays have no SLA and are rate-limited
([iroh public relays](https://docs.iroh.computer/iroh-services/relays/public)).
Libp2p relays require reservations and can limit time and bytes
([Circuit Relay v2](https://github.com/libp2p/specs/blob/master/relay/circuit-v2.md)).
Tailscale DERP relays WireGuard ciphertext, but both peers need Tailscale and a
tailnet ([DERP reference](https://tailscale.com/docs/reference/derp-servers),
[free plans](https://tailscale.com/docs/account/manage-plans/free-plans-discounts)).

One small VPS can start near USD 6.49 per month before VAT, IPv4, and excess
transfer
([Hetzner prices](https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/)).
No-relay mode has no bill, but RFC 5128 requires fallback when direct setup
fails ([RFC 5128](https://www.rfc-editor.org/rfc/rfc5128.html#section-5.1)).

## Tunnel services as an alternative

| Service | What the owner must do | Cost for this use | Friend and trust effect |
| --- | --- | --- | --- |
| Cloudflare Tunnel | Create an account and domain setup, run `cloudflared`, configure a tunnel and Access policy. | A Zero Trust Free plan is available, but payment details are still required at setup. | Arbitrary TCP needs `cloudflared` on the friend device and an identity login. Cloudflare carries the traffic. |
| ngrok | Create an account, save an authtoken, add a payment method, and run the agent. | Free TCP includes 1 GB and 5,000 TCP or TLS connections per month. Paid use adds endpoint and transfer charges. | The friend can use a normal host and port. Seer still needs end-to-end encryption above ngrok. |
| zrok | Get and enable an account, then run a share, or self-host zrok and OpenZiti. | The hosted service offers a free account. Self-hosting adds VPS and operations cost. | Private TCP sharing requires zrok access on the friend side. The project states that private traffic is end-to-end encrypted. |
| bore | Run `bore local` against the public instance or a self-hosted server. | Open-source software is free. Self-hosting adds the VPS. | The friend uses a normal host and port. Bore traffic is not encrypted by default, so Seer encryption is mandatory. |
| rathole | Run a rathole client and operate a rathole server on a VPS. | Open-source software is free, plus the VPS. | The friend uses the VPS host and port. Noise or TLS protects the owner-to-server leg, not Seer data by itself. |
| frp | Run `frpc` and operate `frps` on a VPS. | Open-source software is free, plus the VPS. | The friend uses the VPS host and port. TLS protects the frp transport, but Seer still needs endpoint encryption. |

Cloudflare arbitrary TCP needs `cloudflared` on both peers, an account, a
site, and browser login
([Cloudflare TCP](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/non-http/cloudflared-authentication/arbitrary-tcp/)).
Its Free plan is not charged, but setup requests payment details
([Cloudflare setup](https://developers.cloudflare.com/cloudflare-one/setup/)).

Ngrok needs an account token and payment method for free TCP
([ngrok CLI](https://ngrok.com/docs/agent/cli), [ngrok FAQ](https://ngrok.com/docs/faq)).
Free limits include 1 GB and 5,000 TCP or TLS connections per month
([ngrok limits](https://ngrok.com/docs/pricing-limits)). Zrok hosted setup
needs a free account; private shares use OpenZiti
([zrok repository](https://github.com/openziti/zrok)).

Bore raw TCP is not encrypted by default
([bore README](https://github.com/ekzhang/bore/blob/main/README.md)). Rathole
supports TLS and Noise
([rathole security](https://github.com/rathole-org/rathole/blob/main/docs/transport.md)).
Frp supports TCP, QUIC, WebSocket, and TLS
([frp configuration](https://github.com/fatedier/frp/blob/dev/conf/frpc_full_example.toml)).
These tools validate the outbound-agent model, but add a separate control
plane or owner setup. A Seer relay keeps Seer identity and one service only.

## Owner-side direct options

| Option | Where it works | Owner work | Limits |
| --- | --- | --- | --- |
| PCP | A router or CGN that offers PCP MAP. | The app requests and renews one Seer mapping. | Provider and router support are not guaranteed. Other firewalls can still block it. |
| NAT-PMP | A local NAT gateway that implements NAT-PMP. | The app requests, renews, and removes one mapping. | It does not control an upstream CGN. PCP is its successor. |
| UPnP IGD | A home router with IGD enabled. | The app discovers the gateway and maps only the Seer port. | Users or routers can disable it. A lease must be renewed and removed. |
| Manual port forward | A user-controlled home router with a public WAN address. | The owner reserves a local address and configures one port. | High setup cost and no solution for an uncontrolled CGN. |
| IPv6 direct | Both peers have IPv6 and inbound policy permits the Seer port. | The owner permits one service through host and router firewalls. | A global IPv6 address does not imply inbound reachability. |

PCP covers residential NATs, CGNs, and IPv6 firewalls
([RFC 6887](https://www.rfc-editor.org/rfc/rfc6887.html#section-10.1)). NAT-PMP
recommends PCP first, then a NAT-PMP retry
([RFC 6886](https://www.rfc-editor.org/rfc/rfc6886.html#section-1.1)); the Rust
client supports public address and port requests
([natpmp crate](https://docs.rs/natpmp/)). UPnP IGD and the Rust `igd` crate
also support gateway mappings
([UPnP IGD](https://openconnectivity.org/developer/specifications/upnp-resources/upnp/internet-gateway-device-igd-v-2-0/), [igd crate](https://docs.rs/igd/)).

IPv6 addressability does not ensure reachability through a home firewall
([RFC 7368](https://www.rfc-editor.org/rfc/rfc7368.html#section-2.2)). Mapping
is only an optimization. Seer should map one port with a renewable lease.

## Hole punching results and failure cases

| Source and population | Transport | Reported result | Important limit |
| --- | --- | --- | --- |
| Ford, Srisuresh, and Kegel, deployed NAT products, 2005 | UDP | About 82 percent of tested NATs supported hole punching. | Product sample, not current end-user connection attempts. |
| Same study | TCP | About 64 percent supported TCP hole punching. | TCP simultaneous open had more implementation limits. |
| Protocol Labs controlled residential test, about 45 volunteers, 2022 | TCP | 86 percent of attempts succeeded. | Small controlled sample. |
| Same test | QUIC | 93 percent of attempts succeeded. | Small controlled sample. |
| IPFS DCUtR production measurement, more than 4.4 million attempts and 85,000 networks, 2026 | TCP and QUIC | 70 percent, plus or minus 7.1 percent, conditional success. | Excludes attempts where relay reservation or public address discovery failed. |

The 2005 results are in the authors' primary paper
([Peer-to-Peer Communication Across NATs](https://bford.info/pub/net/p2pnat-abs/)).
The 2022 residential results report 86 percent for TCP and 93 percent for QUIC
([Decentralized Hole Punching](https://research.protocol.ai/publications/decentralized-hole-punching/seemann2022.pdf)).
The 2026 production campaign reports a conditional rate of 70 percent, plus
or minus 7.1 percent
([DCUtR final report](https://github.com/probe-lab/dcutr-project/blob/main/docs/dcutr-final-report.md)).

These numbers are not directly comparable. They use different years,
populations, protocols, prerequisites, and definitions of an attempt. They
show that hole punching is valuable, but not reliable enough to be the only
path.

Hole punching needs reusable endpoint mappings. It fails when a NAT creates a
different mapping for each destination. RFC 5128 calls this endpoint-dependent
mapping. This behavior was also called symmetric NAT in older terminology
([RFC 5128, sections 3.3 and 5.2](https://www.rfc-editor.org/rfc/rfc5128.html#section-3.3)).

CGN places a provider NAT outside the home router. Subscribers do not receive
unique public IPv4 addresses, and some applications cannot work without extra
support
([RFC 6888, section 1](https://www.rfc-editor.org/rfc/rfc6888.html#section-1)).
A manual home-router mapping does not control that provider NAT. PCP can help
only if the provider offers and authorizes it.

Measurements on two US cellular carriers found multiple NAT layers and
carrier-specific mapping and filtering behavior
([cellular middlebox study](https://ftp.eecs.umich.edu/~zmao/Papers/netpiculet.pdf)).
Cafe and mobile policy varies, so Seer must keep a relay-capable path.

## Terminal stream requirements

A terminal session is normally latency-sensitive and low volume. Each key can
need a round trip before server output confirms it. Full-screen redraws and
commands with large output can create short bandwidth bursts. Mosh uses local
prediction on high-latency links, adjusts frame rate to avoid filling queues,
and notes that it is mostly idle when the user is not typing
([Mosh README](https://github.com/mobile-shell/mosh)).

A relay changes a path from owner-to-friend into owner-to-relay-to-friend.
The user feels the sum of both relay legs and relay processing. A nearby relay
can be acceptable for a terminal, while a distant relay can make every key
feel slow. Throughput is usually not the limit until a command emits much
output. The relay must then carry every byte and can become the bottleneck.

Seer must encrypt above the path selection layer. QUIC uses TLS for peer
authentication, confidentiality, and integrity
([RFC 9001](https://www.rfc-editor.org/rfc/rfc9001.html)). If the owner's key
from the capsule authenticates that QUIC session end to end, a rendezvous,
relay, or raw TCP tunnel cannot read terminal data. It can still observe IP
addresses, timing, duration, and byte counts.

## Decision table

Assumptions for this table:

- Public IPv4 means the owner host is reachable on the Seer port.
- Home NAT means the owner controls the router but no mapping is assumed.
- CGNAT means the owner cannot configure the provider NAT.
- Cafe WiFi may block UDP but permits outbound TCP on common web ports.
- Mobile may use CGN and may change addresses.
- A Seer relay is reachable through an outbound connection.

| Owner network | Friend network | First direct path to try | Reliable fallback | Who runs what |
| --- | --- | --- | --- | --- |
| Public IPv4 | Home NAT | Friend makes an outbound direct connection to owner. | Seer relay if local policy blocks direct. | Owner runs Seer. Friend pastes the one line. Project runs rendezvous and relay. |
| Public IPv4 | Cafe WiFi | Direct TCP or QUIC to owner, with TCP useful when UDP is blocked. | Seer relay on a commonly allowed outbound port. | Same as above. |
| Public IPv4 | Mobile | Direct to owner after current mobile address selection. | Seer relay during hard NAT or address change. | Same as above. |
| Home NAT | Home NAT | PCP, NAT-PMP, or UPnP mapped path; otherwise coordinated hole punch. | Seer relay. | Seer manages the owner's mapping and punch. Friend only pastes the line. |
| Home NAT | Cafe WiFi | Mapped owner TCP path; otherwise punch if policy permits. | Seer relay. | Same as above. |
| Home NAT | Mobile | Mapped owner path; otherwise coordinated punch. | Seer relay. | Same as above. |
| CGNAT | Home NAT | Coordinated hole punch can work with reusable mappings. | Seer relay. | Owner and friend run Seer. Project runs the public relay. Home port forwarding is not sufficient. |
| CGNAT | Cafe WiFi | Try direct candidates, but restrictive policy makes success uncertain. | Seer relay. | Same as above. |
| CGNAT | Mobile | Try direct candidates, but two carrier or restrictive NAT paths are the hardest case. | Seer relay. | Same as above. |

IPv6 is an additional direct candidate in every row where both peers have
working IPv6 and inbound policy permits Seer. It does not replace the relay
because either peer can be IPv4-only or filtered.

## Recommended default and fallback

Use this default sequence:

1. `seer start` creates or loads one stable server key.
2. Seer opens an outbound encrypted connection to a nearby Seer relay.
3. Seer publishes a signed, short-lived presence record to rendezvous.
4. `seer invite` puts the server ID, one-use seat token, rendezvous URL, and
   relay hints in the capsule.
5. The friend's existing install line installs Seer and passes the capsule to
   `seer join`.
6. The friend resolves the signed presence and starts the encrypted Seer
   session through the relay.
7. Both peers try IPv6, public, mapped, and punched candidates in parallel.
8. If a direct path succeeds, Seer moves the session to it.
9. If all direct checks fail, the session stays on the relay with no new
   friend action.

This makes the reliable path the default connection state and makes direct
transport an optimization. It also keeps the friend contract at one pasted
line. The owner runs only Seer. The project operates at least one rendezvous
and relay service, with a self-hosted relay URL as an advanced override.

Tunnel services are useful for a prototype or emergency operator override.
They should not be the normal friend flow. Public project relays without an
SLA should not be the only production fallback.

## What this means for Seer

- Add a stable server public key to the invitation identity model.
- Put rendezvous and relay bootstrap data in the capsule, not only one direct
  host and port.
- Start through an outbound relay connection, then upgrade to direct when a
  candidate works.
- Keep the friend flow at one pasted install-and-join line with no account.
- Try PCP, NAT-PMP, UPnP IGD, and IPv6 only as direct-path optimizations.
- Keep a relay available for symmetric NAT, CGNAT, mobile, and restrictive
  WiFi cases.
- Encrypt and authenticate terminal data end to end above every relay or
  tunnel path.
- Limit the relay to Seer endpoint identities and Seer traffic. Do not expose
  the machine or a subnet.
- Measure direct rate, relay rate, setup time, RTT, bytes, and failure reason
  without logging seat tokens or terminal content.

## Open questions

- Will the first service use an iroh relay implementation, a smaller custom
  relay, or another audited transport component?
- Which outbound ports and transports must the relay support for restrictive
  cafe and enterprise networks?
- Will Seer operate one relay region for the MVP or require two regions for a
  latency and outage fallback?
- What short lifetime and refresh interval should a presence record use?
- Which server identity bytes fit in the capsule while keeping the invite easy
  to paste?
- Should an advanced owner be able to select a self-hosted relay in
  `broker.toml`?
- What metadata retention policy applies to rendezvous and relay logs?
