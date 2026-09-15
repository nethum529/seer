#!/usr/bin/env bash
# Address lookup runs for issue 391. The method is in
# docs/research/20-address-lookup.md. It uses the probe and the helpers of
# scripts/perf/transport.sh and writes the same CSV format.
#
# Usage: scripts/perf/lookup.sh [samples] [output-dir]
set -euo pipefail

# shellcheck source=scripts/perf/transport.sh
source "$(dirname "${BASH_SOURCE[0]}")/transport.sh" "${1:-10}" \
    "${2:-target/perf/391-$(date -u +%Y%m%dT%H%M%SZ)}"

# The Pkarr record TTL is 30 s and resolved serves no stale data, so a gap of
# 35 s makes every DNS query a cache miss without a cache flush.
dns_gap_seconds=35

# N fresh processes, one cold sample each, with a gap before each process.
spaced_cold() {
    local workload=$1 gap=$2
    shift 2
    for _ in $(seq "$samples"); do
        sleep "$gap"
        run_probe "$workload" "$@" --samples 1
    done
}

other_relay() {
    if [[ $1 == *use1-1* ]]; then
        echo "https://euc1-1.relay.n0.iroh.link./"
    else
        echo "https://use1-1.relay.n0.iroh.link./"
    fi
}

measure_dns_cold() {
    start_serve --api iroh --condition default
    spaced_cold iroh_dial "$dns_gap_seconds" dial --api iroh \
        --condition discovery_dnscold --remote "$(serve_field id)"
    stop_serve

    start_serve --api seer --condition default
    spaced_cold seer_dial "$dns_gap_seconds" dial --api seer \
        --condition discovery_dnscold --remote "$(serve_field id)"
    stop_serve
}

measure_hints() {
    start_serve --api iroh --condition default
    local id relay
    id=$(serve_field id)
    relay=$(serve_field relay)
    echo "$relay" >"$work/relay.txt"
    cold_and_warm iroh_dial dial --api iroh --condition relayhint --remote "$id" \
        --relay "$relay"
    cold_and_warm iroh_dial dial --api iroh --condition staleip --remote "$id" \
        --ip 192.0.2.1:9
    cold_and_warm iroh_dial dial --api iroh --condition stalerelay --remote "$id" \
        --relay "$(other_relay "$relay")"
    stop_serve
}

measure_peer_down() {
    start_serve --api iroh --condition default
    local id
    id=$(serve_field id)
    stop_serve
    spaced_cold iroh_dial 0 dial --api iroh --condition peerdown --remote "$id"
}

write_environment
echo "rev,workload,condition,cache,sample,boundary,ms,status" >"$raw"
: >"$errors"
measure_dns_cold
measure_hints
measure_peer_down
echo "relay: $(cat "$work/relay.txt")" >>"$out/environment.txt"
echo "dns_gap_seconds: $dns_gap_seconds" >>"$out/environment.txt"
scripts/perf/summarize.sh "$raw" >"$out/summary.csv"
echo "wrote $out"
