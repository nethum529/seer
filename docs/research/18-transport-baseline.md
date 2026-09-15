# Transport baseline

Date: 2026-09-13 (run at 2026-09-14T04:27Z)
Issue: 389 (R-001). Related: 370, 386.

## Scope

This file records how to measure Seer connection and terminal transport
costs, and the first baseline. It keeps the Seer adapter cost (seer-net)
apart from the iroh cost. It does not change transport behavior.

## Source revision

- Revision: 367e50cb992fba8a2eea5de6d39455c76d82d87b.
- This revision is main at 8714a51 plus the probe and the driver. The
  seer-net, broker, and runtime code is the same as 8714a51.
- iroh 1.1.0, preset N0, relay https://use1-1.relay.n0.iroh.link./.

## Machine and build

- AMD Ryzen 7 7800X3D, 16 logical CPUs, Linux 7.1.6-1-cachyos x86_64.
- rustc 1.98.0. Release profile from Cargo.toml: lto fat, codegen-units 1,
  panic abort.
- DNS goes to systemd-resolved at 127.0.0.53. The run does not flush that
  cache.
- Links up: eno1 (wired), wlan0, tailscale0.
- The full environment of each run is in environment.txt next to the raw
  samples.

## How to run

    scripts/perf/transport.sh [samples] [output-dir]

- The default is 10 samples. This baseline used 20.
- The default output directory is target/perf/389-<UTC time>/.
- The script builds the release probe and the release seer-broker first.
- It needs bash 5 (EPOCHREALTIME), Linux, internet access, and a free
  loopback port 47389.
- A full run with 20 samples takes about 10 minutes.
- To summarize any samples file: scripts/perf/summarize.sh samples.csv.

The probe is crates/seer-net/examples/transport_probe.rs. You can also run
it directly:

    cargo build --release -p seer-net --example transport_probe
    target/release/examples/transport_probe <command> --api iroh|seer \
        --condition <condition> [--samples N] [--echoes N] \
        [--remote ID] [--relay URL] [--ip ADDR]

Commands: serve, listen, dial (iroh or seer), session (seer only). serve
prints one line that starts with "ready", with id, relay, loopback, and ips
fields. It then echoes every stream until you stop it.

## Output format

One CSV line for each timed boundary:

    rev,workload,condition,cache,sample,boundary,ms,status

- rev: git revision. The suffix -dirty means uncommitted changes.
- workload: see the next section.
- condition: network condition, see below.
- cache: cold or warm, see below.
- sample: sample index in its process. Sample 0 is always cold.
- boundary: see the timing boundaries table.
- ms: milliseconds, 3 decimals.
- status: ok, or fail:<reason>.

The probe prints the same line without rev. The driver adds rev. Other
passes (issues 390 to 393) can filter these lines with grep or awk.

## Workloads

The driver runs each workload in two ways:

- Cold: N fresh processes, one sample each. Each process has a new endpoint,
  a new key, and empty in-process caches.
- Warm: one process with N+1 samples. Sample 0 is cold. Samples 1 to N are
  warm.

So each workload has N+1 cold samples and N warm samples. The meaning of
warm is different for each workload:

| Workload | Command | Warm sample means |
|---|---|---|
| broker_ready | seer-broker <tmp>/broker.toml, remote = true | New process, same state dir (key and registry on disk) |
| iroh_listen | transport_probe listen --api iroh | New endpoint in the same process |
| seer_listen | transport_probe listen --api seer | New seer_net::Listener in the same process |
| iroh_dial | transport_probe dial --api iroh | Same endpoint, new connection |
| seer_dial | transport_probe dial --api seer | New seer_net::dial call, same key, as a runtime reconnect does today |
| seer_session | transport_probe session --api seer | New stream on the existing seer_net::Session |

The dial workloads connect to a serve process of the same api on this
machine. The driver waits 5 seconds after serve prints ready, so that the
serve endpoint can publish its address.

Each dial sample sends 20 one byte echoes after the first stream opens.

## Timing boundaries

| Boundary | Starts | Stops | Timer location |
|---|---|---|---|
| process_spawn | Driver reads EPOCHREALTIME before exec | First line of probe main | scripts/perf/transport.sh:63, transport_probe.rs:105 and 157 |
| runtime_start | Before tokio multi thread runtime build | Runtime built | transport_probe.rs:182 |
| endpoint_bind | Before Endpoint::builder(N0).bind() | bind returns | transport_probe.rs:228, 297 |
| relay_online | After bind | endpoint.online() returns | transport_probe.rs:233 |
| address_lookup | Before connect_with_opts | connect_with_opts returns | transport_probe.rs:304 |
| dial_handshake | connect_with_opts returned | Connecting resolves to a Connection | transport_probe.rs:307 |
| stream_open | Stream request (open_bi, Session::open, or dial return) | First echo byte comes back | transport_probe.rs:319, 383, 400 |
| echo_rtt | One byte written | One byte read back | transport_probe.rs:326, 418 |
| seer_listen_ready | Before seer_net::Listener::bind | bind returns | transport_probe.rs:346 |
| seer_dial | Before seer_net::dial | dial returns a stream | transport_probe.rs:381 |
| seer_dial_session | Before seer_net::dial_session | dial_session returns | transport_probe.rs:393 |
| process_ready | Driver reads EPOCHREALTIME before broker exec | Broker loopback TCP port accepts | scripts/perf/transport.sh:131 |

Notes:

- The probe bind function copies bind_endpoint in crates/seer-net/src/lib.rs
  so that bind and online have separate timers.
- iroh connect_with_opts waits for address resolution. When the address has
  no known path, this is the address lookup. The handshake starts after it.
- The far side sees a new stream only after the opener writes. This is why
  stream_open includes the first echo.
- For seer_dial, seer_net::dial already opened the stream. stream_open there
  is only the first echo through both socket pair bridges.
- seer_listen_ready includes the listener thread, the runtime, bind, and the
  relay online wait (crates/seer-net/src/lib.rs:83 and 282).
- The broker binds the iroh listener before the TCP port
  (crates/seer-broker/src/lib.rs:51 and 52). So process_ready includes the
  relay online wait.
- The driver polls the broker port in a busy loop in the shell, with no
  sleep. The resolution is about one connect attempt.
- echo_rtt is the transport part of a keystroke to echo. It does not include
  the PTY, the broker forward hop, the runtime, or the TUI render.

## Network conditions

| Condition | Meaning |
|---|---|
| default | Listen only, no peer. IP and relay transports on, internet reachable. |
| loopback | Same machine. The dialer gets only the 127.0.0.1 address of the serve endpoint. |
| discovery | Same machine. The dialer gets only the endpoint ID and uses N0 address lookup. This is the path that Seer uses today, because seer_net::dial takes only an ID. |
| relay | Both sides remove the IP transports. The dialer gets the endpoint ID and the relay URL. All packets go through the relay. |
| lan | Two machines on one LAN. Supported by the probe, not measured here. |

To measure lan, run serve on the second machine, then run dial with
--condition lan --remote <id> --ip <lan ip:port> from the ips field. The seer
api cannot use loopback, lan, or relay, because seer_net::dial has no
address or transport option.

## Retention of samples and failures

- Every run writes samples.csv, summary.csv, environment.txt, and errors.log
  to its output directory.
- A pass that reports numbers must commit its run under
  docs/research/perf-samples/<issue>/. The baseline is in
  docs/research/perf-samples/389/.
- Never delete a failed sample from samples.csv. summary.csv counts failures
  in the fail column and leaves them out of median and p95.
- A failed step has status fail:<reason>. The ms value is the time until the
  failure. Commas in the reason become spaces. Later boundaries of that
  sample are not written. Format examples (not measured values):

      <rev>,iroh_dial,discovery,cold,0,dial_handshake,15000.412,fail:timeout
      <rev>,seer_dial,discovery,cold,0,seer_dial,10021.003,fail:could not connect to endpoint
      <rev>,seer_dial,-,cold,0,process_exit,0,fail:exit 2
      <rev>,broker_ready,default,cold,0,process_ready,15000.020,fail:timeout

- A probe that exits with an error adds a process_exit line. A broker that
  exits early gives fail:broker exited.
- errors.log keeps the stderr of every probe. For a failed broker sample it
  keeps the last 5 broker log lines, without lines that contain a
  credential.
- The baseline run had 0 failures and an empty errors.log.

## Baseline

20 samples for each workload. 21 cold samples (20 processes plus sample 0
of the warm process) and 20 warm samples. echo_rtt has 20 echoes for each
sample. p95 uses the nearest rank. All values are in ms. The full table is
in perf-samples/389/summary.csv.

### Listen and start

| Workload | Condition | Cache | Boundary | Median | p95 |
|---|---|---|---|---|---|
| iroh_listen | default | cold | process_spawn | 0.741 | 0.769 |
| iroh_listen | default | cold | runtime_start | 0.392 | 0.410 |
| iroh_listen | default | cold | endpoint_bind | 1.560 | 1.642 |
| iroh_listen | default | cold | relay_online | 3118.939 | 3124.925 |
| iroh_listen | default | warm | endpoint_bind | 1.169 | 1.510 |
| iroh_listen | default | warm | relay_online | 3119.389 | 3125.235 |
| iroh_listen | relay | cold | endpoint_bind | 1.562 | 1.693 |
| iroh_listen | relay | cold | relay_online | 1365.477 | 1426.115 |
| iroh_listen | relay | warm | relay_online | 1368.722 | 1410.497 |
| seer_listen | default | cold | seer_listen_ready | 3120.110 | 3126.369 |
| seer_listen | default | warm | seer_listen_ready | 3125.303 | 3127.110 |
| broker_ready | default | cold | process_ready | 3121.365 | 3127.723 |
| broker_ready | default | warm | process_ready | 3122.147 | 3127.085 |

### Dial and first stream

| Workload | Condition | Cache | Boundary | Median | p95 |
|---|---|---|---|---|---|
| iroh_dial | loopback | cold | endpoint_bind | 1.485 | 1.577 |
| iroh_dial | loopback | cold | address_lookup | 0.094 | 0.109 |
| iroh_dial | loopback | cold | dial_handshake | 0.547 | 0.597 |
| iroh_dial | loopback | cold | stream_open | 0.473 | 0.619 |
| iroh_dial | loopback | warm | dial_handshake | 0.352 | 0.421 |
| iroh_dial | loopback | warm | stream_open | 0.173 | 0.191 |
| iroh_dial | discovery | cold | endpoint_bind | 1.390 | 1.538 |
| iroh_dial | discovery | cold | address_lookup | 1.649 | 129.475 |
| iroh_dial | discovery | cold | dial_handshake | 191.470 | 200.193 |
| iroh_dial | discovery | cold | stream_open | 90.463 | 94.137 |
| iroh_dial | discovery | warm | address_lookup | 0.079 | 0.090 |
| iroh_dial | discovery | warm | dial_handshake | 0.354 | 0.398 |
| iroh_dial | discovery | warm | stream_open | 0.166 | 0.217 |
| iroh_dial | relay | cold | dial_handshake | 193.626 | 201.118 |
| iroh_dial | relay | cold | stream_open | 93.521 | 97.198 |
| iroh_dial | relay | warm | dial_handshake | 76.567 | 76.862 |
| iroh_dial | relay | warm | stream_open | 90.178 | 90.299 |
| seer_dial | discovery | cold | process_spawn | 0.703 | 0.748 |
| seer_dial | discovery | cold | seer_dial | 199.532 | 204.956 |
| seer_dial | discovery | cold | stream_open | 87.701 | 96.263 |
| seer_dial | discovery | warm | seer_dial | 196.545 | 205.171 |
| seer_dial | discovery | warm | stream_open | 93.062 | 223.780 |
| seer_session | discovery | cold | seer_dial_session | 203.994 | 204.935 |
| seer_session | discovery | cold | stream_open | 86.302 | 96.557 |
| seer_session | discovery | warm | stream_open | 0.135 | 0.149 |

### Echo round trip (transport part of keystroke to echo)

| Workload | Condition | Cache | n | Median | p95 |
|---|---|---|---|---|---|
| iroh_dial | loopback | warm | 400 | 0.032 | 0.055 |
| iroh_dial | discovery | warm | 400 | 0.032 | 0.056 |
| iroh_dial | relay | warm | 400 | 76.208 | 76.641 |
| seer_dial | discovery | warm | 400 | 0.040 | 0.061 |
| seer_session | discovery | warm | 400 | 0.042 | 0.057 |

## What the baseline shows

These points compare medians from this run. They are measured facts except
where a point says "candidate".

- Listener readiness is about 3.12 s for iroh, for seer_net::Listener, and
  for the broker process. Bind is under 2 ms. The relay online wait is
  almost all of it. The Seer adapter and the broker add no cost that is
  larger than the run to run spread (about 6 ms).
- With only the relay transport, relay_online drops to 1.37 s. So about
  1.75 s of the default wait depends on the IP transports. The default wait
  has a spread of only about 6 ms, which looks like a fixed timer and not
  like network time. Candidate cause, not proven: the iroh net report
  PROBES_TIMEOUT of 3 s (iroh 1.1.0 src/net_report/defaults.rs:23).
- A cold Seer dial takes 199.5 ms. The iroh parts of the same path (bind,
  lookup, handshake) take about 194.5 ms. So the adapter costs about 5 ms
  on a cold dial.
- The cold handshake on the discovery path (191 ms) matches the relay path
  (194 ms). On the same machine the loopback handshake is 0.55 ms.
- The first stream on a new connection takes about 90 ms on the discovery
  and relay paths and 0.47 ms on loopback. The relay echo round trip is
  76 ms.
- A warm Seer dial (196.5 ms) costs the same as a cold one, because each
  seer_net::dial builds a new runtime and endpoint. A warm iroh dial that
  reuses its endpoint takes 0.35 ms to handshake and 0.17 ms to open its
  first stream.
- A new stream on an existing seer_net::Session takes 0.135 ms. This is
  already under the 5 ms target in issue 386.
- The socket pair bridge adds about 8 microseconds to each echo round trip
  (0.040 ms against 0.032 ms).
- seer join calls seer_net::dial (crates/seer/src/commands.rs:358). This
  pass measured that call at about 0.2 s on the same machine. Issue 386
  reports about 3.8 s for a same host join. Most of that time is outside
  seer_net::dial, and this pass did not measure it.

## Not measured

- lan condition: this pass had one machine.
- The full keystroke to echo through the TUI, broker, runtime, and PTY.
- The full seer start, seer join, and runtime reconnect commands. seer start
  uses a fixed port and the user config directory, so the driver runs
  seer-broker directly with a temporary config instead.
- Thread counts for each process.
- Cold DNS: the systemd-resolved cache was not flushed between samples.
- macOS.
