#!/usr/bin/env bash
# Measurement driver for issue 393, the seer-net stream adapter. The method
# is in docs/research/22-stream-adapter.md. It uses the probe from issue 389.
#
# Usage: flock /tmp/claude-1000/perf-run.lock scripts/perf/adapter.sh [samples] [output-dir] [mode]
# mode warm (default): one process per payload, sample 0 cold, then warm dials.
# mode single: one process per sample, one dial each. No old dialer runtime
# lives in the process while the echoes run, so the counters are clean.
# Linux only. Needs python3 for the Unix socket pair capacity.
set -euo pipefail

samples=${1:-10}
root=$(git rev-parse --show-toplevel)
cd "$root"
out=${2:-target/perf/393-$(date -u +%Y%m%dT%H%M%SZ)}
mode=${3:-warm}
mkdir -p "$out"
raw="$out/samples.csv"
counters="$out/counters.csv"
errors="$out/errors.log"
work=$(mktemp -d)
probe=target/release/examples/transport_probe
serve_pid=
settle_seconds=5
echoes=100
# 1 byte is the issue 389 echo. 115 bytes is one TerminalInput key frame.
# The other sizes are Cells frames: blank 80x24, full 80x24, full 200x50.
# crates/seer-core/examples/frame_sizes.rs prints these sizes.
payloads="1 115 4278 297822 1550274"

cleanup() {
    if [[ -n $serve_pid ]]; then
        kill "$serve_pid" 2>/dev/null || true
    fi
    rm -rf "$work"
}
trap cleanup EXIT

cargo build --release -p seer-net --example transport_probe

rev=$(git rev-parse HEAD)
if [[ -n $(git status --porcelain) ]]; then
    rev="$rev-dirty"
fi

{
    echo "date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "revision: $rev"
    echo "samples_per_workload: $samples"
    echo "echoes_per_sample: $echoes"
    echo "mode: $mode"
    echo "kernel: $(uname -srm)"
    echo "cpu: $(lscpu | sed -n 's/^Model name: *//p')"
    echo "cpu_count: $(nproc)"
    echo "rustc: $(rustc -V)"
    echo "profile: release (lto fat, codegen-units 1, panic abort)"
    echo "iroh: $(grep -A1 '^name = "iroh"$' Cargo.lock | sed -n 's/^version = //p')"
    echo "net.core.wmem_default: $(cat /proc/sys/net/core/wmem_default)"
    echo "links: $(ip -br link | awk '{print $1 "=" $2}' | tr '\n' ' ')"
} >"$out/environment.txt"

start_serve() {
    "$probe" serve "$@" >"$work/serve.out" 2>>"$errors" &
    serve_pid=$!
    for _ in $(seq 150); do
        if grep -q '^ready ' "$work/serve.out"; then
            sleep "$settle_seconds"
            return 0
        fi
        sleep 0.1
    done
    echo "serve $* did not become ready" | tee -a "$errors" >&2
    return 1
}

stop_serve() {
    kill "$serve_pid" 2>/dev/null || true
    wait "$serve_pid" 2>/dev/null || true
    serve_pid=
}

dial_process() {
    local api=$1 id=$2 payload=$3 count=$4 status=0
    "$probe" dial --api "$api" --condition discovery --remote "$id" \
        --samples "$count" --echoes "$echoes" --payload "$payload" \
        --counters 1 >"$work/probe.out" 2>>"$errors" || status=$?
    grep -v '^counters,' "$work/probe.out" | sed "s/^/$rev,/" >>"$raw" || true
    grep '^counters,' "$work/probe.out" | sed "s/^counters,/$rev,/" >>"$counters" || true
    if ((status != 0)); then
        echo "$rev,${api}_dial_p$payload,-,cold,0,process_exit,0,fail:exit $status" >>"$raw"
    fi
}

measure_api() {
    local api=$1 id
    start_serve --api "$api" --condition default
    id=$(grep '^ready ' "$work/serve.out" | sed -E 's/.* id=([^ ]*).*/\1/')
    for payload in $payloads; do
        if [[ $mode == single ]]; then
            for _ in $(seq "$samples"); do
                dial_process "$api" "$id" "$payload" 1
            done
        else
            dial_process "$api" "$id" "$payload" $((samples + 1))
        fi
    done
    stop_serve
}

# Payload bytes that one side of a new Unix socket pair takes before a
# non-blocking write fails, for each write size.
measure_pair() {
    echo "sample,write_bytes,capacity_bytes" >"$out/pair_capacity.csv"
    for sample in $(seq "$samples"); do
        for chunk in 1 115 4096 65536; do
            python3 -c '
import socket, sys
chunk = int(sys.argv[1])
a, b = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
a.setblocking(False)
total = 0
try:
    while True:
        total += a.send(b"x" * chunk)
except BlockingIOError:
    pass
print(total)' "$chunk" | sed "s/^/$sample,$chunk,/" >>"$out/pair_capacity.csv"
        done
    done
}

echo "rev,workload,condition,cache,sample,boundary,ms,status" >"$raw"
echo "rev,workload,condition,sample,echoes,payload,cpu_ns,slices,vcsw,nvcsw,threads" >"$counters"
: >"$errors"
measure_pair
measure_api iroh
measure_api seer
scripts/perf/summarize.sh "$raw" >"$out/summary.csv"
echo "wrote $out"
