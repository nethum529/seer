# Service-only connection security

Research date: 2026-09-02.

## Scope

This note designs the security boundary for a Seer friend connection. It covers
identity, trust setup, transport, authorization, public exposure, and relay
trust. It does not design shell isolation, NAT traversal, or a production relay.

The current broker uses plain TCP. It makes a 32-byte random seat token, stores
its SHA-256 verifier, expires it after one hour, and accepts it once. The current
runtime starts the configured shell without changing the operating-system user.
These facts come from the [seat registry](../../crates/seer-broker/src/registry.rs),
[broker server](../../crates/seer-broker/src/server.rs), and
[runtime manager](../../crates/seer-broker/src/runtime.rs).

## Short answer

Use Noise over the one Seer connection. Give the broker one stable X25519 static
key. Put its 32-byte public key in every invite capsule. Make one X25519 client
key on the friend's first join. Use `Noise_XK_25519_ChaChaPoly_BLAKE2s` for the
first join and later connections. The XK pattern lets the client know the
server key before the handshake and sends the client static key under
encryption. The Noise specification defines this exact pattern and its payload
properties ([Noise XK](https://noiseprotocol.org/noise.html#interactive-handshake-patterns-fundamental)).

Keep the current single-use seat. Send it only in an encrypted Noise payload.
After a successful join, use the client public key as the durable device
identity. Do not keep a permanent bearer credential.

## Threat model

### Assets

- The owner's files, credentials, processes, network access, and home network.
- The broker server private key and the client device private keys.
- Seat tokens, client public-key records, revocation state, and audit records.
- Terminal input, terminal output, pane state, and command history.
- The integrity and availability of the broker, runtimes, and shell processes.
- Connection metadata, such as peer addresses, time, duration, and byte count.

### Attackers and their abilities

| Attacker | What the attacker can do | What the design must prevent |
| --- | --- | --- |
| Someone on the internet | Scan the address, open connections, send malformed handshake bytes, replay captured bytes, and consume network or pre-auth resources. | They cannot learn terminal data, authenticate, create a runtime, or use Seer as a route to another service. |
| Someone who finds an old invite | Learn the endpoint and server public key. Try the token again. Trigger the public handshake and rate limits. | A used or expired seat cannot create a client identity. Editing the capsule expiry cannot extend the server record. |
| The relay operator | See both relay connections, the server routing identity, timing, and traffic size. Drop, delay, duplicate, or reorder traffic. Refuse service. | The relay cannot read or change authenticated Seer data, learn the seat token from traffic, or impersonate either Noise peer. |
| A malicious friend | Authenticate with a valid client key. Exercise all operations that their Seer role permits. Send hostile post-auth protocol data and shell input. Consume shell, disk, CPU, process, and network resources. | Revocation must stop new connections and close current ones. Cross-user view rules must remain server-enforced. Transport security cannot make the granted shell safe. |

### The shell is intentional authority

The friend gets an interactive shell on the owner PC by design. This is not a
side effect. A service-only connection limits network entry to the Seer
protocol. It does not limit what the shell can do after Seer starts it.

Today the runtime and shell inherit the broker's operating-system identity
([runtime spawn](../../crates/seer-broker/src/runtime.rs#L76-L98)).
Therefore a friend can access each file and process that this identity can
access. This can include source code, SSH keys, browser data, cloud tokens,
other Seer users' state, and the Seer server key. The friend can also install
persistence, use the owner's network position, or try a local privilege
escalation. Mode `0600` does not protect a key from a shell that has the same
Unix user ID.

Revocation stops Seer access only if the friend cannot modify the broker state
or steal another valid key. It cannot undo commands that already ran. This note
does not claim that a malicious friend is contained after shell access.

## Identity and local key storage

### Server identity

The server identity is one stable X25519 public key. X25519 inputs and outputs
are 32-byte strings ([RFC 7748](https://datatracker.ietf.org/doc/html/rfc7748#section-5)).
The broker proves possession of the matching private key during every Noise
handshake. A host name and IP address are routing data, not identity.

Display the fingerprint as `SHA256:` followed by the unpadded base64url encoding
of SHA-256 over the raw 32-byte server public key. For the fake public key
`00 01 ... 1f`, the fingerprint is:

`SHA256:Yw3NKWbEM2aRElRIu7JbT_QSpJxzLbLIq8G4WBvXEN0`

### Client identity

On the first join, the client makes one X25519 static key pair. The final XK
handshake message proves possession of the private key and sends the public key
under encryption. The server binds that public key to the new Seer person after
the seat check succeeds. A later connection proves the same key again.

### Files and permissions

| Side | File | Mode | Contents |
| --- | --- | --- | --- |
| Owner | `~/.config/seer/server.key` | `0600` | Raw or encoded 32-byte server private key. |
| Owner | `~/.config/seer/broker.toml` | `0600` | Listen address, published route, and state path. No private key text. |
| Owner | `~/.local/state/seer/registry.json` | `0600` | People, client public keys, roles, revocation state, and seat records. One file permits an atomic join update. |
| Friend | `~/.config/seer/device.key` | `0600` | Raw or encoded 32-byte client private key. Made at first join. |
| Friend | `~/.config/seer/servers.toml` | `0600` | Endpoint, server public key, fingerprint, person ID, and current-server flag. No seat token. |

Use mode `0700` for both Seer directories. OpenSSH similarly keeps user identity
keys under `~/.ssh`, recommends that directory be inaccessible to other users,
and rejects private keys that are accessible by others
([OpenSSH paths and permissions](https://github.com/openssh/openssh-portable/blob/master/ssh.1),
[OpenSSH permission check](https://github.com/openssh/openssh-portable/blob/master/authfile.c)).

## Trust setup

### Invite pinning and TOFU

The capsule carries the server public key. The client configures that key as the
known remote static key before Noise starts. This is stronger than silent trust
on first use because an active network attacker cannot replace only the key
during the connection.

The capsule delivery channel is still part of trust setup. An attacker who can
replace the complete capsule can send the friend to an attacker-controlled
server. The owner should send it through an existing private channel.

Show the fingerprint after the server proves the key and before the client asks
for a name. Save it in `servers.toml`. On all later connections, a different
key is a hard failure. Do not offer a one-keypress bypass.

For a manual connection without a capsule, use explicit trust on first use.
Show the full fingerprint and require the user to compare it with the owner
through another channel. OpenSSH stores host keys in `~/.ssh/known_hosts`, adds
new keys, and warns when an identity changes
([OpenSSH host-key behavior](https://github.com/openssh/openssh-portable/blob/master/ssh.1)).
The one-line friend flow must use capsule pinning, not manual TOFU.

### Exact capsule contents

Use `SEER2.` followed by unpadded base64url. RFC 4648 defines the URL-safe
alphabet and permits omitted padding when the data length is known
([RFC 4648](https://datatracker.ietf.org/doc/html/rfc4648#section-5)).

The decoded bytes are, in order:

| Field | Size | Rule |
| --- | ---: | --- |
| Version | 1 byte | Value `2`. |
| Route kind | 1 byte | `0` is direct TCP. `1` is a relay route. |
| Endpoint length | 1 byte | Unsigned length `N`, from 1 through 255. |
| Endpoint | `N` bytes | ASCII `host:port`. The route kind defines how it is used. |
| Server public key | 32 bytes | Raw X25519 public key. |
| Seat token | 32 bytes | Cryptographically random, single-use secret. |
| Expiry | 8 bytes | Unsigned Unix seconds, network byte order. |

The decoded size is `75 + N` bytes. The printed size is
`6 + ceil(4 * (75 + N) / 3)` characters. Thus a 16-byte endpoint makes a
128-character capsule. A 21-byte endpoint makes a 134-character capsule.

These examples use the fake key `00 01 ... 1f`, fake token `20 21 ... 3f`, and
fake expiry `1800000000`. They must never be used as real invites.

Direct, `203.0.113.7:7321`, 128 characters:

`SEER2.AgAQMjAzLjAuMTEzLjc6NzMyMQABAgMEBQYHCAkKCwwNDg8QERITFBUWFxgZGhscHR4fICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8AAAAAa0nSAA`

Relay, `relay.example.com:443`, 134 characters:

`SEER2.AgEVcmVsYXkuZXhhbXBsZS5jb206NDQzAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-PwAAAABrSdIA`

The capsule expiry permits an offline rejection, but the server expiry is
authoritative. Do not save the seat after join. It can remain in shell history,
so short expiry and single use are required.

## Transport options

| Option | Security and identity | Service-only fit | Cost and risk |
| --- | --- | --- | --- |
| TLS 1.3 with a pinned self-signed certificate, using rustls | TLS 1.3 gives server authentication, optional client authentication, confidentiality, integrity, and forward secrecy ([RFC 8446](https://datatracker.ietf.org/doc/html/rfc8446#section-1)). The application pins the certificate key. | Good. Only the application behind the listener is reachable. | A self-signed end certificate is not accepted by the rustls default verifier. Pinning needs a carefully audited custom verifier, certificate parsing, and signature checks. Rustls puts this API under `danger` and says its default verifier does not use one certificate as both CA and end entity ([rustls verifier](https://docs.rs/rustls/latest/rustls/client/danger/trait.ServerCertVerifier.html), [rustls non-features](https://rustls.dev/docs/rustls/manual/_04_features/index.html#non-features)). Certificate lifetime and stable-key rotation add policy. |
| Noise with snow | A selected pattern combines static identities, ephemeral key exchange, authenticated encryption, and transcript binding. Snow provides handshake and reliable-stream transport states ([Noise specification](https://noiseprotocol.org/noise.html), [snow documentation](https://docs.rs/snow/latest/snow/)). | Best minimal fit. One pinned 32-byte key maps directly to one Seer service. | Seer must select one pattern, frame records, set size limits, handle rekey or reconnect, and keep protocol versioning outside ambiguous payloads. Noise is a framework, so a wrong pattern or state transition is an application defect. |
| QUIC with a pinned key | QUIC uses TLS 1.3 for authentication and protects application packets. It supplies reliable streams, flow control, loss recovery, and connection migration ([RFC 9000](https://datatracker.ietf.org/doc/html/rfc9000), [RFC 9001](https://datatracker.ietf.org/doc/html/rfc9001)). | Good when Seer needs independent streams or roaming. A pinned endpoint key can name the one service. | It adds UDP, QUIC state, certificate or raw-key verification, and more pre-auth packet processing. Join and other state changes must not use replayable 0-RTT data ([RFC 9001 replay warning](https://datatracker.ietf.org/doc/html/rfc9001#section-9.2)). This is more transport than the first Seer service needs. |
| WireGuard | Peers exchange static public keys in advance. The protocol gives authenticated key exchange, forward secrecy, identity hiding, and denial-of-service cookies ([WireGuard paper](https://www.wireguard.com/papers/wireguard.pdf)). | Poor default fit. It creates an IP interface, not one application capability. `AllowedIPs` controls peer addresses and routes, not destination service ports ([wg manual](https://git.zx2c4.com/wireguard-tools/about/src/man/wg.8)). A firewall is still needed for one-port access. | The friend needs VPN setup and usually operating-system integration. A policy error can expose other IP services. This repeats the product friction that Seer is removing. |

Use Noise for the minimal design. Do not layer it over TLS.

## Authorization

### Seats and device keys

- Keep a 32-byte random seat token and a one-hour default lifetime.
- Store only a token verifier, expiry, and used flag on the server.
- Check token, expiry, requested name, and name collision before consumption.
- Commit the new person, client public key, and used seat in one atomic update.
- Return one generic `invalid seat` result for unknown, expired, and used seats.
- Bind every later request to the person found by the authenticated client key.
- Enforce owner, own-tree control, and cross-user read-only rules on the server.

### Revocation

Revoking a friend marks all of that person's client keys revoked, closes their
active connections, and rejects later handshakes. Device-only revocation marks
one key. Keep the record and `revoked_at` value for audit instead of deleting
it. Revocation does not delete panes or undo shell actions. If the same-UID
shell can read or change broker state, revocation is not a security boundary.

Tailscale similarly revokes a removed device's node key
([Tailscale node keys](https://tailscale.com/docs/concepts/node-keys)). SSH
public-key access is revoked by removing or marking the authorized key, but SSH
also needs explicit policy for shells, forwarding, and subsystems
([RFC 4252 public-key authentication](https://datatracker.ietf.org/doc/html/rfc4252#section-7),
[sshd configuration](https://man.openbsd.org/sshd_config)).

### Listener limits

Start with a 5-second handshake deadline, a 4 KiB maximum handshake frame, four
unauthenticated connections per direct source address, 32 unauthenticated
connections in total, and a token bucket of two new handshakes per second per
source with a burst of five. Add a separate global bucket. A relay circuit must
have its own limit because all circuits can share one network source address.

Reject before runtime creation. Use bounded buffers and close on the first
invalid frame. Measure normal joins and tune these values. SSH permits a delay
after failed authentication but warns about self-denial of service
([RFC 4251](https://datatracker.ietf.org/doc/html/rfc4251#section-9.4)).

## Public exposure

A direct open port exposes the kernel TCP path, the small Seer record header,
the Noise handshake implementation, connection bookkeeping, and rate-limit
state. Before Noise completes, it must not expose the Seer message codec, user
lookup details, seat status, runtime manager, PTY code, or shell creation.

After authentication, the peer reaches only Seer. Seer must not offer network
forwarding, file transfer, or a generic command RPC. The shell can still make
connections with its operating-system identity.

An internet-facing SSH port exposes SSH version and algorithm negotiation, key
exchange, and user authentication before login. After login, the SSH connection
protocol can provide shells, command execution, subsystems, TCP forwarding,
X11 forwarding, and Unix-socket forwarding. These are standard SSH functions
([RFC 4251 architecture](https://datatracker.ietf.org/doc/html/rfc4251),
[OpenSSH client](https://github.com/openssh/openssh-portable/blob/master/ssh.1)).
OpenSSH can disable or restrict them, but that is a separate configuration
surface.

Seer has a narrower semantic surface than a general SSH service. It is also a
new implementation with less review. A smaller protocol is not proof of secure
code. Fuzzing, bounded parsing, dependency review, and independent review are
still required before a public listener is recommended.

### Rendezvous value

A rendezvous design lets the owner keep one outbound connection to a relay. The
home router needs no forwarded inbound port. The relay opens a circuit only for
an enrolled client key or a one-use routing capability linked to a seat. The
Noise handshake still runs end to end inside that circuit.

This removes routine internet scanning from the owner broker and hides the home
address from the friend when all traffic stays relayed. It does not remove
denial of service. The relay can observe metadata, block service, and flood the
owner's outbound circuit. Direct hole punching can also reveal addresses.

Iroh is useful prior art: its endpoint ID is a public key, QUIC authenticates
the remote endpoint, the application decides which endpoint may connect, and a
relay forwards encrypted traffic by endpoint ID
([iroh encryption and relays](https://docs.rs/iroh/latest/iroh/),
[iroh endpoint ID](https://docs.rs/iroh/latest/iroh/type.EndpointId.html)).
The same split is suitable for Seer: routing finds a key; Noise authenticates
it; Seer authorizes it.

## Comparison with existing systems

| System | Identity and trust | Authorization and revocation | Exposure and relay model |
| --- | --- | --- | --- |
| Tailscale | A device makes machine and node keys. The control plane binds a node key to a user and distributes allowed peer public keys ([Tailscale identity](https://tailscale.com/docs/concepts/tailscale-identity)). | Grants can restrict a source to one destination port. The initial tailnet policy can allow all device-to-device traffic until it is changed ([grant syntax](https://tailscale.com/docs/reference/syntax/grants), [ACL default](https://tailscale.com/docs/features/access-control/acls)). Removing a device revokes its node key. | WireGuard carries IP traffic. Direct links are preferred. DERP forwards already encrypted traffic and cannot decrypt it ([DERP servers](https://tailscale.com/docs/reference/derp-servers)). This is strong networking, but the friend must install a VPN, use an account, and receive network-layer access. |
| SSH | The server has host keys. The client records them in `known_hosts`. A user can authenticate with a public key that signs the session-bound request ([RFC 4252](https://datatracker.ietf.org/doc/html/rfc4252#section-7)). | Unix accounts and `authorized_keys` select users. Keys can be removed or restricted. Server configuration controls commands, PTYs, forwarding, and subsystems. | One TCP port exposes the SSH protocol. Successful authentication can grant a shell and several tunnel types. There is no standard rendezvous service. ProxyJump forwards through another SSH server. |
| Mosh | Mosh uses SSH to authenticate and start `mosh-server`. The server returns a UDP port and AES-128 session key through SSH ([Mosh repository](https://github.com/mobile-shell/mosh#how-it-works)). | Mosh has no durable user identity or invite revocation layer of its own. The SSH login supplies user authority. The session key authorizes one session. | The session uses one high UDP port and authenticated encryption. Mosh has no TCP, X11, or agent forwarding, but it still needs the SSH bootstrap and normally a UDP port in 60000 through 61000 ([Mosh repository](https://github.com/mobile-shell/mosh)). |
| iroh | Each endpoint ID is a public key. A connection authenticates both endpoint IDs over QUIC/TLS ([iroh documentation](https://docs.rs/iroh/latest/iroh/)). | The application must decide if the authenticated peer is allowed. Iroh does not define Seer person, seat, tree, or revocation policy. | Direct paths, NAT traversal, and encrypted relay fallback are built around the endpoint key. This is the closest model for key-addressed rendezvous, but it is a larger transport choice than Noise over the current stream. |

## Recommended minimal security design

### Protocol choice

Use `Noise_XK_25519_ChaChaPoly_BLAKE2s` through snow over one length-framed TCP
stream. Use the ASCII prologue `seer/2`. Disable zero-RTT behavior. Allow only
this one Noise protocol name. After the handshake, use Noise transport messages
for every Seer frame and reconnect before the Noise nonce space can be
exhausted. Snow documents separate handshake and transport states for reliable
streams ([snow documentation](https://docs.rs/snow/latest/snow/)).

### First-join handshake

1. The owner makes the stable server key before the first invite.
2. The owner makes a 32-byte seat token, stores its verifier and server expiry,
   and writes the exact capsule format above.
3. The client decodes the capsule, rejects bad length or an expired client-side
   expiry, and makes `device.key` if it does not exist.
4. The client configures the capsule server key as the expected XK responder
   static key. It does not accept a key learned from the network.
5. Client to server sends XK message 1: `e, es`. No Seer payload is present.
6. Server to client sends XK message 2: `e, ee`. No identity result is present.
7. Client to server sends XK message 3: `s, se`. Its encrypted payload contains
   protocol version, operation `join`, seat token, requested name, and a fresh
   client request ID.
8. The server completes Noise, checks all field bounds, and atomically validates
   the seat, binds the client key to the new person, and marks the seat used.
9. After that durable update, the server sends an encrypted result with person
   ID, accepted name, server fingerprint, and the client request ID.
10. The client verifies the request ID, writes the server public data to
    `servers.toml`, deletes the seat from memory, and starts the first attach.

The XK message sequence is `-> e, es`, `<- e, ee`, `-> s, se`, with the
responder static key known before the exchange
([Noise XK pattern](https://noiseprotocol.org/noise.html#interactive-handshake-patterns-fundamental)).

### Later handshake

1. The client loads `device.key` and the pinned server key from `servers.toml`.
2. Both sides run the same XK exchange with the same prologue.
3. Message 3 contains encrypted operation `authenticate`, person ID, protocol
   version, and a fresh request ID. It contains no seat or bearer credential.
4. The server maps the authenticated client static key to the person, checks
   revocation and role state, and returns one encrypted result.
5. Only after success can the connection reach the Seer message codec and
   runtime routing.

Using one pattern for join and reconnect keeps the state machine small. The
seat authorizes the first key binding. The client key authorizes later use.

## What this means for Seer

- Service-only means one Seer protocol path, not a safe or restricted shell.
- Use one stable 32-byte server public key as the server identity.
- Put that key, one 32-byte seat, route data, and expiry in `SEER2` capsules.
- Make and save one client device key at first join. Do not save the seat.
- Use pinned Noise XK over the direct stream and inside any future relay path.
- Reject all Seer application messages until Noise and authorization complete.
- Revoke client keys, close live connections, and keep bounded pre-auth state.
- Keep private files at `0600`, but do not treat this as same-UID shell isolation.
- Prefer an outbound rendezvous path when seamless NAT traversal is required.

## Open questions

- What operating-system boundary will stop a friend shell from reading broker
  keys and owner files when both currently use the same Unix user ID?
- Must the first built-in connection use a direct forwarded port, a hosted
  relay, or both?
- How will server-key backup, loss, rotation, and out-of-band recovery work?
- Should one person have several device keys in the first release, or require a
  new person seat for each device until device management exists?
- Which relay routing capability lets an unused seat reach the owner without
  revealing the seat token to the relay?
- What measured limits replace the proposed starting rate and concurrency
  values on the supported home-server hardware?
