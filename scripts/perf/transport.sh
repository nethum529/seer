#!/usr/bin/env bash
# Measurement driver for issue 389. The method is in
# docs/research/18-transport-baseline.md.
#
# Usage: scripts/perf/transport.sh [samples] [output-dir] [all|reuse]
# The reuse set is for issue 390. It measures endpoint reuse against
# connection reuse on the relay and discovery paths.
# Needs bash 5 or later for EPOCHREALTIME. Linux only.
set -euo pipefail

samples=${1:-10}
root=$(git rev-parse --show-toplevel)
cd "$root"
out=${2:-target/perf/389-$(date -u +%Y%m%dT%H%M%SZ)}
set_name=${3:-all}
mkdir -p "$out"
raw="$out/samples.csv"
errors="$out/errors.log"
work=$(mktemp -d)
probe=target/release/examples/transport_probe
broker=target/release/seer-broker
broker_port=47389
serve_pid=
settle_seconds=5

cleanup() {
    if [[ -n $serve_pid ]]; then
        kill "$serve_pid" 2>/dev/null || true
    fi
    rm -rf "$work"
}
trap cleanup EXIT

cargo build --release -p seer-net --example transport_probe
cargo build --release -p seer --bin seer-broker

rev=$(git rev-parse HEAD)
if [[ -n $(git status --porcelain) ]]; then
    rev="$rev-dirty"
fi

write_environment() {
    {
        echo "date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "revision: $rev"
        echo "samples_per_workload: $samples"
        echo "workload_set: $set_name"
        echo "kernel: $(uname -srm)"
        echo "cpu: $(lscpu | sed -n 's/^Model name: *//p')"
        echo "cpu_count: $(nproc)"
        echo "rustc: $(rustc -V)"
        echo "profile: release (lto fat, codegen-units 1, panic abort)"
        echo "iroh: $(grep -A1 '^name = "iroh"$' Cargo.lock | sed -n 's/^version = //p')"
        echo "nameservers: $(sed -n 's/^nameserver //p' /etc/resolv.conf | tr '\n' ' ')"
        echo "links: $(ip -br link | awk '{print $1 "=" $2}' | tr '\n' ' ')"
    } >"$out/environment.txt"
}

ms() {
    awk -v micros="$1" 'BEGIN { printf "%.3f", micros / 1000 }'
}

# One probe process. Sample 0 of every process is a cold sample.
run_probe() {
    local workload=$1 status=0
    shift
    SEER_PROBE_SPAWN_US=${EPOCHREALTIME/./} "$probe" "$@" \
        >"$work/probe.out" 2>>"$errors" || status=$?
    sed "s/^/$rev,/" "$work/probe.out" >>"$raw"
    if ((status != 0)); then
        echo "$rev,$workload,-,cold,0,process_exit,0,fail:exit $status" >>"$raw"
    fi
}

# N fresh processes give the cold samples. One process with N+1 samples gives
# one more cold sample and N warm samples.
cold_and_warm() {
    local workload=$1
    shift
    for _ in $(seq "$samples"); do
        run_probe "$workload" "$@" --samples 1
    done
    run_probe "$workload" "$@" --samples $((samples + 1))
}

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

serve_field() {
    grep '^ready ' "$work/serve.out" | sed -E "s/.* $1=([^ ]*).*/\1/"
}

stop_serve() {
    kill "$serve_pid" 2>/dev/null || true
    wait "$serve_pid" 2>/dev/null || true
    serve_pid=
}

# Time from broker spawn until its loopback TCP port accepts. The broker binds
# the iroh listener before the TCP port, so this includes the relay wait.
broker_sample() {
    local state=$1 sample=$2 cache=$3 status=ok started pid
    cat >"$state/broker.toml" <<EOF
listen = "127.0.0.1:$broker_port"
published_addr = "127.0.0.1:$broker_port"
remote = true
state_dir = "$state"
owner_name = "perf"
EOF
    started=${EPOCHREALTIME/./}
    "$broker" "$state/broker.toml" >>"$state/broker.log" 2>&1 &
    pid=$!
    until { : <>"/dev/tcp/127.0.0.1/$broker_port"; } 2>/dev/null; do
        if ! kill -0 "$pid" 2>/dev/null; then
            status="fail:broker exited"
            break
        fi
        if ((${EPOCHREALTIME/./} - started > 15000000)); then
            status="fail:timeout"
            break
        fi
    done
    local elapsed=$((${EPOCHREALTIME/./} - started))
    echo "$rev,broker_ready,default,$cache,$sample,process_ready,$(ms "$elapsed"),$status" >>"$raw"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    if [[ $status != ok ]]; then
        grep -v -i credential "$state/broker.log" | tail -n 5 >>"$errors" || true
    fi
}

measure_broker() {
    local warm_state
    for _ in $(seq "$samples"); do
        local state
        state=$(mktemp -d -p "$work")
        broker_sample "$state" 0 cold
    done
    warm_state=$(mktemp -d -p "$work")
    for sample in $(seq 0 "$samples"); do
        local cache=warm
        if ((sample == 0)); then
            cache=cold
        fi
        broker_sample "$warm_state" "$sample" "$cache"
    done
}

measure_listen() {
    cold_and_warm iroh_listen listen --api iroh --condition default
    cold_and_warm iroh_listen listen --api iroh --condition relay
    cold_and_warm seer_listen listen --api seer --condition default
}

measure_dial() {
    start_serve --api iroh --condition default
    local id loopback relay
    id=$(serve_field id)
    loopback=$(serve_field loopback)
    serve_field relay >"$work/relay.txt"
    cold_and_warm iroh_dial dial --api iroh --condition loopback --remote "$id" --ip "$loopback"
    cold_and_warm iroh_dial dial --api iroh --condition discovery --remote "$id"
    stop_serve

    start_serve --api iroh --condition relay
    id=$(serve_field id)
    relay=$(serve_field relay)
    cold_and_warm iroh_dial dial --api iroh --condition relay --remote "$id" --relay "$relay"
    stop_serve

    start_serve --api seer --condition default
    id=$(serve_field id)
    cold_and_warm seer_dial dial --api seer --condition discovery --remote "$id"
    cold_and_warm seer_session session --api seer --condition discovery --remote "$id"
    stop_serve
}

measure_reuse() {
    start_serve --api iroh --condition relay
    local id relay
    id=$(serve_field id)
    relay=$(serve_field relay)
    echo "$relay" >"$work/relay.txt"
    cold_and_warm iroh_dial dial --api iroh --condition relay --remote "$id" --relay "$relay"
    cold_and_warm iroh_session session --api iroh --condition relay --remote "$id" --relay "$relay"
    stop_serve

    start_serve --api iroh --condition default
    id=$(serve_field id)
    cold_and_warm iroh_session session --api iroh --condition discovery --remote "$id"
    stop_serve
}

if { : <>"/dev/tcp/127.0.0.1/$broker_port"; } 2>/dev/null; then
    echo "port $broker_port is in use; stop the process that uses it" >&2
    exit 1
fi

write_environment
echo "rev,workload,condition,cache,sample,boundary,ms,status" >"$raw"
: >"$errors"
case $set_name in
all)
    measure_broker
    measure_listen
    measure_dial
    ;;
reuse) measure_reuse ;;
*)
    echo "unknown workload set: $set_name" >&2
    exit 1
    ;;
esac
echo "relay: $(cat "$work/relay.txt")" >>"$out/environment.txt"
scripts/perf/summarize.sh "$raw" >"$out/summary.csv"
echo "wrote $out"
