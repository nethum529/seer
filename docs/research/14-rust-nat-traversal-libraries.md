# Rust NAT traversal libraries for Seer

Research date: 2026-09-02.

## Scope

This note evaluates Rust libraries that can connect two Seer peers through home NATs without a VPN. The owner runs Linux.
The friend runs macOS or Linux and pastes one command. The connection must expose only Seer.

Seer uses blocking TCP today. Its capsule carries a host, port, and seat token ([capsule source](../../crates/seer/src/capsule.rs)).
The start flow publishes a Tailscale address when one is available ([start source](../../crates/seer/src/start.rs)).
The replacement must preserve the one-line join rule ([ADR 0001](../adr/0001-one-line-install-and-join.md)).

## Short answer

Use iroh for the first Seer prototype. It gives the friend the fewest steps. It combines authenticated QUIC, NAT traversal,
a relay fallback, and address lookup behind one endpoint API ([iroh crate documentation](https://docs.rs/iroh/latest/iroh/)).
The friend can use an endpoint public key and a Seer seat token from the pasted capsule. No VPN, account, port forward, or separate network client is required.

Rust-libp2p is the safest complete NAT stack to depend on for years. Its peer identity, AutoNAT, Circuit Relay v2, and DCUtR
protocols have public specifications and implementations in more than one language ([libp2p specifications](https://github.com/libp2p/specs)).
It needs more Seer code and relay operations than iroh.

Rustls is the best transport-only choice when a public TCP port already exists. It does not solve the home NAT problem.

## Evaluation method

- A complete result must include identity, encrypted transport, address discovery, NAT traversal, and a relay fallback.
- A relay is required for reliable use. STUN alone cannot traverse every NAT ([RFC 8489, section 13](https://www.rfc-editor.org/rfc/rfc8489.html#section-13)).
- ICE tests host, server-reflexive, and relayed candidates ([RFC 8445, section 5](https://www.rfc-editor.org/rfc/rfc8445.html#section-5)).
- The API cost includes the move from blocking `std::net::TcpStream` to an async runtime, event loop, or packet interface.
- Maintenance data is a snapshot on the research date. Release pages and repository activity are the source, not star counts.

## Capability summary

| Candidate | Peer identity | Transport crypto | NAT traversal | Relay | Discovery |
| --- | --- | --- | --- | --- | --- |
| iroh 1.1.0 | Ed25519 public key as `EndpointId` | Authenticated QUIC and TLS | Built-in UDP hole punching | Built-in encrypted relay path | DNS built in; pkarr and mDNS extensions |
| rust-libp2p 0.56.0 | Public-key-derived `PeerId` | QUIC TLS or TCP with Noise | AutoNAT plus DCUtR | Circuit Relay v2 | Identify plus app-selected mDNS, DHT, or rendezvous |
| quinn 0.11.11 | TLS certificate chosen by the app | QUIC and rustls | None | None | None |
| str0m 0.23.1 | DTLS certificate fingerprint | DTLS plus SCTP data channel | ICE agent | TURN candidates supplied by the app | Signaling supplied by the app |
| webrtc-rs 0.20.4 | DTLS certificate fingerprint | DTLS plus SCTP data channel | Full ICE stack | Configured STUN or TURN service | Signaling supplied by the app |
| boringtun 0.7.1 | WireGuard static public key | WireGuard | None | None | Configured IP endpoint only |
| wireguard-rs 0.1.4 | WireGuard static public key | WireGuard | None | None | Configured IP endpoint only |
| snow 0.10.0 | Static key or PSK selected by Noise pattern | Noise handshake and records | None | None | None |
| rustls 0.23.43 | X.509 or an app verifier | TLS 1.2 and 1.3 | None | None | Normal TCP address input |

Iroh authenticates the `EndpointId` public key during QUIC setup ([iroh encryption](https://docs.rs/iroh/latest/iroh/#encryption)).
Libp2p derives the `PeerId` from the public key ([peer ID specification](https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md)).
The other entries follow their protocol APIs described below.

## Iroh

Iroh is the only candidate that supplies the full path with useful defaults. An `EndpointAddr` contains the peer public key
and can contain direct and relay addresses ([EndpointAddr API](https://docs.rs/iroh/latest/iroh/struct.EndpointAddr.html)).
The endpoint first has a relay path, coordinates simultaneous UDP sends, and keeps the relay as fallback if the direct QUIC path fails
([iroh NAT traversal](https://docs.iroh.computer/concepts/nat-traversal)).

The iroh project reports that about 9 of 10 network pairs get a direct path. This is a project estimate, not an independent Seer result.
All pairs with a working relay can continue through the relay. The relay forwards encrypted packets and does not have the endpoint keys
([iroh relay description](https://docs.rs/iroh/latest/iroh/#relay-servers)).

The default builder uses relays run by number 0. Seer can give the builder a custom relay map
([endpoint builder source](https://github.com/n0-computer/iroh/blob/main/iroh/src/endpoint.rs)).
The same repository ships the `iroh-relay` server with allowlist, denylist, shared-token, and HTTP access controls
([relay server source](https://github.com/n0-computer/iroh/blob/main/iroh-relay/src/main.rs)).
The owner can self-host it, but it needs a public server and TLS.

DNS lookup is in the main crate. It maps an endpoint ID to relay and address records
([DNS lookup API](https://docs.rs/iroh/latest/iroh/address_lookup/dns/struct.DnsAddressLookup.html)).
Pkarr and local mDNS are separate official lookup crates ([iroh address lookups](https://github.com/n0-computer/iroh-address-lookups)).
Seer can also put the full `EndpointAddr` in the capsule and avoid lookup for the first dial.

The connection exposes async QUIC bidirectional streams. Iroh uses Tokio. Seer would need one async network task per runtime,
bounded channels, and a small adapter around the existing blocking framed protocol. This is a medium move. It does not require a protocol rewrite.

Iroh uses MIT OR Apache-2.0 ([manifest](https://github.com/n0-computer/iroh/blob/main/iroh/Cargo.toml)). Version 1.1.0 is current.
Releases are frequent and the repository is active ([releases](https://github.com/n0-computer/iroh/releases)). Delta Chat lists
iroh and iroh-gossip as dependencies ([Delta Chat manifest](https://github.com/deltachat/deltachat-core-rust/blob/main/Cargo.toml)).

## Rust-libp2p

Rust-libp2p provides QUIC and TCP transports. QUIC binds the libp2p peer key to TLS
([libp2p QUIC specification](https://github.com/libp2p/specs/blob/master/quic/README.md)). TCP normally uses the libp2p Noise
handshake and a stream multiplexer ([Noise specification](https://github.com/libp2p/specs/blob/master/noise/README.md)).

Identify exchanges observed addresses. AutoNAT tests whether an address is publicly reachable
([AutoNAT v2 specification](https://github.com/libp2p/specs/blob/master/autonat/autonat-v2.md)). A private peer reserves capacity
on a Circuit Relay v2 server. DCUtR starts through that relay, exchanges addresses, and tries a synchronized direct dial
([DCUtR specification](https://github.com/libp2p/specs/blob/master/relay/DCUtR.md)).

The latest large production study measured about 70 percent conditional hole punch success. It excludes attempts that failed before DCUtR,
so this is not an end-to-end connection rate ([DCUtR report](https://github.com/probe-lab/dcutr-project/blob/main/docs/dcutr-final-report.md)).
Direct failure can stay on the relay. Circuit Relay v2 limits time and bytes, so Seer must provision capacity and renew reservations
([relay v2 specification](https://github.com/libp2p/specs/blob/master/relay/circuit-v2.md)).

There is no default public relay or discovery service for a Seer product. Seer must run relay and bootstrap nodes or select an operator.
The official Rust examples include a relay server ([relay server example](https://github.com/libp2p/rust-libp2p/tree/master/examples/relay-server)).
The owner can self-host it on a public server.

`libp2p::Stream` implements async read and write traits ([stream API](https://docs.rs/libp2p/latest/libp2p/struct.Stream.html)).
Seer must add Tokio, assemble a `Swarm`, route peer events, and bridge async streams to its blocking protocol. This is a high move.

Rust-libp2p uses MIT ([license](https://github.com/libp2p/rust-libp2p/blob/master/LICENSE)). Version 0.56.0 is the latest stable release.
The repository remains active between crate releases ([releases](https://github.com/libp2p/rust-libp2p/releases)). Polkadot SDK lists
rust-libp2p as a dependency ([Polkadot SDK manifest](https://github.com/paritytech/polkadot-sdk/blob/master/Cargo.toml)).

## Quinn with a custom NAT path

Quinn supplies QUIC, rustls integration, connections, and async streams ([Quinn repository](https://github.com/quinn-rs/quinn)).
It does not supply a peer identity policy, STUN, hole punching, relay, or discovery. Seer must define its own endpoint identity.

A complete design needs STUN, authenticated signaling, simultaneous UDP sends, candidate checks, a relay, and path migration.
RFC 8489 states that STUN is not a complete NAT traversal solution ([RFC 8489](https://www.rfc-editor.org/rfc/rfc8489.html)).
Direct success is unknown until Seer measures the design. There is no fallback until Seer builds the relay.

Quinn streams implement Tokio async IO ([receive stream API](https://docs.rs/quinn/latest/quinn/struct.RecvStream.html)).
The stream adapter is a medium move. Owning the NAT and relay protocol makes the total move very high.

Quinn uses MIT OR Apache-2.0 ([manifest](https://github.com/quinn-rs/quinn/blob/main/quinn/Cargo.toml)). Version 0.11.11 is current.
The repository has regular releases and current commits ([releases](https://github.com/quinn-rs/quinn/releases)).
The official project sources do not name a production user.

## Str0m

Str0m is a sans-IO WebRTC implementation. It provides ICE, DTLS, SCTP, and
data channels, but the caller owns sockets, time, and signaling
([str0m documentation](https://docs.rs/crate/str0m/latest)). The caller must
enumerate interfaces, create STUN and TURN sockets, exchange candidates, and
feed packets to the state machine.

ICE tries all supplied candidate pairs. A TURN candidate is the reliable
fallback when direct checks fail. Str0m does not obtain that TURN allocation
or run a relay. Seer or the owner must run a TURN service. Coturn is a
self-hosted STUN and TURN server
([coturn repository](https://github.com/coturn/coturn)).

Data channels are message based, not byte streams. Seer needs framing,
backpressure, retransmission settings, signaling, socket polling, and TURN
credentials. Sans-IO can fit a blocking loop, but the full move is very high.

Str0m uses MIT OR Apache-2.0
([manifest](https://github.com/algesten/str0m/blob/main/Cargo.toml)). Version
0.23.1 is current. The project has frequent 2026 releases and current commits
([changelog](https://github.com/algesten/str0m/blob/main/CHANGELOG.md)). Its
official project sources do not name a production user.

## WebRTC-rs

WebRTC-rs supplies a full ICE, DTLS, SCTP, and data-channel stack. The main
crate is an async layer over a sans-IO core. It supports Tokio and smol
runtimes
([architecture](https://github.com/webrtc-rs/webrtc)). The application still
provides offer and answer signaling. It also selects STUN and TURN servers.

ICE gives direct paths priority and uses a TURN relayed candidate when direct
checks fail
([RFC 8445](https://www.rfc-editor.org/rfc/rfc8445.html),
[RFC 8656](https://www.rfc-editor.org/rfc/rfc8656.html)). No public relay is
selected by default. Seer or the owner can self-host coturn.

The data channel API sends and receives messages
([data-channel example](https://github.com/webrtc-rs/webrtc/blob/master/examples/data-channels/data-channels.rs)).
Seer needs an async runtime, signaling, TURN credential handling, and a byte
stream adapter. This is a high move. It includes media protocols that Seer
does not need.

WebRTC-rs uses MIT OR Apache-2.0
([manifest](https://github.com/webrtc-rs/webrtc/blob/master/Cargo.toml)).
Version 0.20.4 is the current stable release, with 0.21 release candidates in
active development
([releases](https://github.com/webrtc-rs/webrtc/releases)). The official
project sources do not name a production user.

## BoringTun

BoringTun implements the WireGuard protocol. Its library is packet based.
The command-line program adds a TUN interface and platform network setup
([BoringTun repository](https://github.com/cloudflare/boringtun)). It does not
provide address discovery, hole punching, or relay service. A configured
public endpoint or another control plane is required.

WireGuard peers use static public keys and encrypted UDP tunnels
([WireGuard protocol](https://www.wireguard.com/protocol/)). Persistent
keepalive can keep an existing NAT mapping open. It does not create a path
between two unreachable peers. There is no library fallback.

This is a packet and virtual-network abstraction, not a Seer byte stream. A
full integration needs TUN privileges, IP routing, platform setup, endpoint
coordination, NAT traversal, and a relay. It also risks exposing more than the
Seer service. The move is very high and does not fit the product boundary.

BoringTun uses BSD-3-Clause
([license](https://github.com/cloudflare/boringtun/blob/master/LICENSE.md)).
Version 0.7.1 is current and the repository has current commits
([crate releases](https://docs.rs/crate/boringtun/latest)). Cloudflare states
that it deployed BoringTun on its mobile clients and servers
([project README](https://github.com/cloudflare/boringtun/blob/master/README.md)).

## Wireguard-rs

Wireguard-rs is an incomplete WireGuard implementation. Its own project page
says not to use it and says it has no users
([project warning](https://git.zx2c4.com/wireguard-rs/about/)). Its README
describes Linux support and incomplete support for other systems
([README](https://git.zx2c4.com/wireguard-rs/tree/README.md)).

It has the same missing NAT traversal, relay, and discovery functions as
BoringTun. It is also packet based. Version 0.1.4 has not had active
development since 2021
([commit log](https://github.com/WireGuard/wireguard-rs/commits/master/)). The license is MIT
([manifest](https://github.com/WireGuard/wireguard-rs/blob/master/Cargo.toml)). Reject it.

## Snow over plain TCP

Snow implements Noise handshakes and transport states. The application picks
a Noise pattern, provides keys, frames messages, and moves bytes over its own
socket
([snow documentation](https://docs.rs/snow/latest/snow/)). It supplies crypto
only. It has no NAT traversal, relay, discovery, or public-key trust policy.

Snow can fit Seer's blocking TCP loop. Seer must add record framing, key
storage, identity binding, replay rules, and protocol negotiation. The move is
medium for a public port, but it does not solve the target home NAT. Direct
failure has no fallback.

Snow uses MIT OR Apache-2.0
([manifest](https://github.com/mcginty/snow/blob/main/Cargo.toml)). Version
0.10.0 is current. Releases are less frequent than iroh or rustls, and the
repository remains active
([releases](https://github.com/mcginty/snow/releases)). The project states
that it has not had a formal security audit. Its official sources do not name
a production user
([project README](https://github.com/mcginty/snow/blob/main/README.md)).

## Rustls over plain TCP

Rustls implements TLS 1.2 and TLS 1.3. It supports server certificates,
client certificates, and custom verification
([rustls documentation](https://docs.rs/rustls/latest/rustls/)). It does not
open sockets or provide NAT traversal, relay, or discovery.

`rustls::Stream` implements blocking `Read` and `Write` over an existing
socket
([stream API](https://docs.rs/rustls/latest/rustls/struct.Stream.html)). This
is the lowest API move. Seer can pin an owner certificate or public key in the
capsule. The owner must still have a public TCP port, port forwarding, or a
separate relay. Direct failure has no library fallback.

Rustls uses Apache-2.0, ISC, or MIT
([license](https://github.com/rustls/rustls/blob/main/LICENSE)). Version
0.23.43 is current. It has frequent patch releases and current development
([releases](https://github.com/rustls/rustls/releases)). The repository says
that many organizations use rustls in production, but it does not give a
verified user list
([project README](https://github.com/rustls/rustls/blob/main/README.md)).

## Build and binary size results

The probe used Rust 1.98.0, Zig 0.16.0, and cargo-zigbuild 0.23.3 ([cargo-zigbuild source](https://github.com/rust-cross/cargo-zigbuild)).
It built release binaries with thin LTO, one codegen unit, abort-on-panic, and symbol stripping. Added size is the stripped probe size
minus an empty Rust binary for the same target. These values compare small API probes. They are not final Seer sizes.

| Candidate probe | Linux x86_64 | macOS arm64 | macOS x86_64 | Added size, Linux / arm64 / x86_64 |
| --- | --- | --- | --- | --- |
| iroh endpoint with default services | Pass | Fail: SDK frameworks absent | Fail: SDK frameworks absent | 12.53 MiB / not measured / not measured |
| rust-libp2p QUIC, TCP, Noise, relay, DCUtR, identify | Pass | Fail: SDK frameworks absent | Fail: SDK frameworks absent | 5.25 MiB / not measured / not measured |
| quinn endpoint with rustls-ring | Pass | Pass | Pass | 1.46 MiB / 1.11 MiB / 1.31 MiB |
| str0m `Rtc`, rust-crypto feature | Pass | Pass | Pass | 3.56 MiB / 2.34 MiB / 3.33 MiB |
| webrtc-rs peer connection | Pass | Pass | Pass | 3.67 MiB / 2.93 MiB / 3.37 MiB |
| BoringTun crypto core | Pass | Pass | Pass | 0.07 MiB / 0.05 MiB / 0.06 MiB |
| wireguard-rs command | Pass | Fail: old ring assembly | Fail: missing platform code | 1.39 MiB / not measured / not measured |
| snow handshake builder | Pass | Pass | Pass | 0.15 MiB / 0.08 MiB / 0.14 MiB |
| rustls client config with ring | Pass | Pass | Pass | 0.75 MiB / 0.54 MiB / 0.69 MiB |

The macOS host did not have an Apple SDK. Iroh and rust-libp2p reached links
to `SystemConfiguration` and `CoreFoundation`, so cargo-zigbuild could not
finish those two probes. This is a build environment blocker, not proof that
the libraries do not support macOS. The other successful probes did not need
those frameworks. Wireguard-rs failed in its old target-specific code, so its
failures are library failures.

All probes pulled native cryptography build code. Iroh, rust-libp2p, Quinn,
webrtc-rs, BoringTun, snow, and the selected rustls provider pulled `ring`.
Ring compiles bundled C and assembly with a C compiler
([ring build notes](https://github.com/briansmith/ring/blob/main/BUILDING.md)).
Str0m pulled `aws-lc-sys`, which uses CMake and a C toolchain
([aws-lc-rs build notes](https://github.com/aws/aws-lc-rs/blob/main/aws-lc-sys/README.md)).
Wireguard-rs pulled an old ring release. No probe needed a separately
installed runtime C library.

The BoringTun probe measured only its crypto core. A complete TUN device, route manager, and platform service would be larger.
Quinn does not include the custom STUN, signaling, and relay code that a complete result needs.

## Ranked result for Seer

| Rank | Choice | Friend steps after paste | Reliable NAT fallback | Seer move | Long-term result |
| ---: | --- | --- | --- | --- | --- |
| 1 | iroh | None | Automatic relay | Medium | Best product fit; integrated but newer stack |
| 2 | rust-libp2p | None if Seer runs infrastructure | App-operated relay | High | Safest complete stack for years |
| 3 | webrtc-rs | None if Seer runs signaling and TURN | TURN | High | Mature standards, excess media scope |
| 4 | str0m | None if Seer builds signaling, sockets, and TURN | TURN | Very high | Small core with large app responsibility |
| 5 | Quinn plus custom NAT code | None after Seer builds all services | Seer must build it | Very high | Good transport, highest protocol ownership |
| 6 | rustls over public TCP | Owner must make a public port | None | Low | Safe crypto, incomplete connection result |
| 7 | snow over public TCP | Owner must make a public port | None | Medium | More crypto protocol risk than rustls |
| 8 | BoringTun | Extra control plane or network setup | None | Very high | Wrong service boundary |
| 9 | wireguard-rs | Extra control plane or network setup | None | Very high | Incomplete and inactive; reject |

Iroh gives the friend a working connection with the fewest steps. The invite can hold the endpoint public key, relay hint, and Seer seat token.
The client can connect through the relay first and try a direct path without user action.

Rust-libp2p is the safest complete dependency for years because its key
protocols are open specifications with several implementations. This does not
make it the best Seer choice. Seer would own more assembly, testing, bootstrap
service, and relay policy.

## What this means for Seer

- Prototype iroh first. Keep the current Seer session protocol above one QUIC bidirectional stream.
- Put the owner's iroh endpoint public key in the capsule. Keep the seat token as the Seer authorization credential.
- Start through a relay, then let iroh upgrade to a direct path. Do not make
  direct traversal a join requirement.
- Run iroh on one Tokio network task and keep the rest of the runtime blocking
  until a broader async move has a separate reason.
- Decide whether Seer can use number 0 relays for an early release. Operate a
  Seer relay before production if their service policy is not sufficient.
- Keep relay traffic end-to-end encrypted and authorize the friend inside
  Seer. Do not create a TUN interface or a host-wide network.
- Do not build STUN, ICE, or a custom QUIC relay for the MVP. That work would
  duplicate complete stacks without improving the friend flow.
- Keep rust-libp2p as the fallback design if long-term protocol independence
  becomes more important than the small integration surface.
- Fix the macOS SDK path in the release builder and repeat the iroh size probe
  before implementation approval.

## Open questions

- Does number 0 permit Seer's expected production traffic, retention, and
  support needs, or must the project operate relays from the first release?
- Which relay regions are required for acceptable terminal latency?
- Should the capsule carry a full `EndpointAddr`, only an endpoint ID, or both
  an endpoint ID and a short-lived Seer rendezvous record?
- How should Seer rotate the iroh endpoint key without invalidating active
  invite capsules?
- Can one async network task provide correct backpressure for every runtime
  without moving PTY and protocol code to Tokio?
- What direct and relayed success rates does iroh achieve on the actual Linux
  owner and macOS friend network sample?
- Does the final cargo-zigbuild release environment link iroh for both macOS
  targets after the Apple SDK is present?
