#!/usr/bin/env bash
# Measurement driver for issue 417, the screen data a guest receives. The
# method is in docs/research/23-screen-data.md.
#
# Usage: flock /tmp/claude-1000/perf-run.lock scripts/perf/screen.sh [repeats] [output-dir] [workloads]
# workloads is a list from: typing editor pager burst. Default is all four.
# It starts its own broker on port 47417 and its own runtime in a temp dir.
# Linux only. Needs bash 5, python3, and sha256sum.
set -euo pipefail

repeats=${1:-3}
root=$(git rev-parse --show-toplevel)
cd "$root"
out=${2:-target/perf/417-$(date -u +%Y%m%dT%H%M%SZ)}
workloads=${3:-typing editor pager burst}
mkdir -p "$out"
raw="$out/samples.csv"
windows="$out/windows.csv"
errors="$out/errors.log"
inputs=docs/research/perf-samples/417/input
work=$(mktemp -d)
port=47417
address=127.0.0.1:$port
broker=$root/target/release/seer-broker
runtime=$root/target/release/seer-runtime
probe=$root/target/release/examples/screen_probe
broker_pid=
runtime_pid=
settle_seconds=3
sizes="80x24 200x50"

cleanup() {
    for pid in $runtime_pid $broker_pid; do
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    done
    rm -rf "$work"
}
trap cleanup EXIT

cargo build --release -p seer --bin seer-broker --bin seer-runtime
cargo build --release -p seer-core --example screen_probe

rev=$(git rev-parse HEAD)
if [[ -n $(git status --porcelain) ]]; then
    rev="$rev-dirty"
fi

rate_ms() {
    case $1 in
        pager) echo 40 ;;
        *) echo 100 ;;
    esac
}

{
    echo "date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "revision: $rev"
    echo "repeats: $repeats"
    echo "workloads: $workloads"
    echo "sizes: $sizes"
    for workload in $workloads; do
        echo "$workload: input=$inputs/$workload.txt rate_ms=$(rate_ms "$workload")"
    done
    echo "shell: bash $(bash --version | head -1 | sed 's/.*version //')"
    echo "vim: $(vim --version | head -1)"
    echo "less: $(less --version | head -1)"
    echo "kernel: $(uname -srm)"
    echo "cpu: $(lscpu | sed -n 's/^Model name: *//p')"
    echo "cpu_count: $(nproc)"
    echo "rustc: $(rustc -V)"
    echo "profile: release (lto fat, codegen-units 1, panic abort)"
} >"$out/environment.txt"

hash() {
    printf %s "$1" | sha256sum | cut -d' ' -f1
}

start_room() {
    local state=$work/state home=$work/home alice=$work/alice
    mkdir -p "$state" "$home" "$alice"
    printf '[{"user_id":"alice","name":"alice","credential_hash":"%s","created_at":1,"is_owner":true},{"user_id":"bob","name":"bob","credential_hash":"%s","created_at":2,"is_owner":false}]\n' \
        "$(hash alice-secret)" "$(hash bob-secret)" >"$state/people.json"
    echo '[]' >"$state/seats.json"
    printf 'listen = "%s"\npublished_addr = "%s"\nremote = false\nstate_dir = "%s"\nowner_name = "alice"\n' \
        "$address" "$address" "$state" >"$work/broker.toml"
    # A fixed prompt with no title escape, so every run types into the same shell.
    printf "PS1='\$ '\nPROMPT_COMMAND=()\nunset HISTFILE\n" >"$home/.bashrc"
    awk 'BEGIN { for (i = 1; i <= 5000; i++) printf "line %5d: the quick brown fox jumps over the lazy dog %d times\n", i, i % 97 }' >"$home/long.txt"
    "$broker" "$work/broker.toml" >"$work/broker.log" 2>&1 &
    broker_pid=$!
    for _ in $(seq 100); do
        if (exec 3<>/dev/tcp/127.0.0.1/$port) 2>/dev/null; then
            break
        fi
        sleep 0.1
    done
    (
        cd "$alice"
        HOME=$home SEER_SNAPSHOT_DIR=$alice SEER_ROOM_ENDPOINT=$address \
            SEER_ROOM_CREDENTIAL=alice-secret SEER_ROOM_KEY=$alice/runtime.key \
            exec "$runtime" "$alice/socket" alice bash gen-alice </dev/null >"$work/runtime.log" 2>&1
    ) &
    runtime_pid=$!
    sleep "$settle_seconds"
}

run_probe() {
    local workload=$1 cols=$2 rows=$3 repeat=$4 status=0
    "$probe" --socket "$work/alice/socket" --room "$address" --host alice \
        --viewer bob --viewer-credential bob-secret --cols "$cols" --rows "$rows" \
        --input "$inputs/$workload.txt" --rate-ms "$(rate_ms "$workload")" \
        --label "$workload" >"$work/probe.out" 2>"$work/probe.err" || status=$?
    sed "s/^/$workload ${cols}x${rows} repeat $repeat: /" "$work/probe.err" >>"$out/probe.log"
    grep -v "^screen: " "$work/probe.err" | sed "s/^/$workload ${cols}x${rows} repeat $repeat: /" >>"$errors" || true
    grep '^update,' "$work/probe.out" | sed "s/^update,/$rev,$repeat,/" >>"$raw" || true
    grep '^window,' "$work/probe.out" | sed "s/^window,/$rev,$repeat,/" >>"$windows" || true
    if ((status != 0)); then
        echo "$workload ${cols}x${rows} repeat $repeat: probe exit $status" | tee -a "$errors" >&2
    fi
}

echo "rev,repeat,workload,cols,rows,seq,t_ms,bytes,frame_cols,frame_rows,changed_cells,changed_rows,row_diff_bytes,cell_diff_bytes" >"$raw"
echo "rev,repeat,workload,cols,rows,keys,last_key_ms,window_ms,updates,bytes" >"$windows"
: >"$errors"
start_room
for size in $sizes; do
    for workload in $workloads; do
        for repeat in $(seq "$repeats"); do
            run_probe "$workload" "${size%x*}" "${size#*x}" "$repeat"
        done
    done
done
scripts/perf/summarize_screen.sh "$windows" "$raw" >"$out/summary.csv"
echo "wrote $out"
