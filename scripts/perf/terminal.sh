#!/usr/bin/env bash
# Output speed of Seer's own terminal in the desktop window against
# Alacritty, Ghostty and kitty (issue 462). The method and the results are
# in docs/research/25-terminal-speed.md.
#
# Usage: scripts/perf/terminal.sh [work-dir] [terminal[:WxH] ...]
# Default terminals: seer alacritty ghostty kitty. WxH is the window size in
# pixels; tune it so every terminal reports the same grid in io-NAME.txt.
# Linux with Hyprland only: each terminal opens as a floating window.
# Needs zsh, curl, python3, sha256sum, hyprctl, and a release build of
# seer-desktop and seer-runtime (cargo build --release -p seer -p seer-desktop).
set -euo pipefail

root=$(git rev-parse --show-toplevel)
work=${1:-$HOME/.cache/seer-bench}
shift || true
terminals=("$@")
if [ ${#terminals[@]} -eq 0 ]; then
    terminals=(seer:1170x806 alacritty:1490x1000 ghostty:1500x900 kitty:1490x973)
fi
io=$root/scripts/perf/terminal_io.zsh
corpus=$work/corpus
results=$work/results/$(date -u +%Y%m%dT%H%M%SZ)
plain_url=https://raw.githubusercontent.com/moktavizen/terminal-benchmark/main/test/shakespeare.txt
plain_sha=5d1293ba864d3f7200f8ed45c7fe75b9bfbc110726a438cdb28835b7c08662b3
mkdir -p "$corpus" "$results"

if [ ! -f "$corpus/plain.txt" ]; then
    curl -fsSL -o "$corpus/plain.txt" "$plain_url"
fi
echo "$plain_sha  $corpus/plain.txt" | sha256sum -c --quiet
if [ ! -f "$corpus/sgr.txt" ]; then
    python3 -I - "$corpus/sgr.txt" <<'EOF'
import random, sys
random.seed(7)
words = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor".split()
out, size = [], 0
while size < 11_000_000:
    line = []
    for _ in range(12):
        word = random.choice(words)
        kind = random.randrange(4)
        if kind == 0:
            line.append(f"\x1b[38;5;{random.randrange(256)}m{word}")
        elif kind == 1:
            r, g, b = (random.randrange(256) for _ in range(3))
            line.append(f"\x1b[38;2;{r};{g};{b}m{word}")
        elif kind == 2:
            line.append(f"\x1b[1m{word}\x1b[22m")
        else:
            line.append(f"\x1b[48;5;{random.randrange(256)}m{word}\x1b[49m")
    text = " ".join(line) + "\x1b[0m\n"
    out.append(text)
    size += len(text)
open(sys.argv[1], "w").write("".join(out))
EOF
fi

start_seer() {
    local state=$work/st config=$work/cfg
    rm -rf "$state" "$config"
    mkdir -p "$config/seer" "$state/seer/runtimes/bench"
    chmod 700 "$state/seer/runtimes/bench"
    printf '[[servers]]\nendpoint = "127.0.0.1:1"\nalias = "bench"\nuser_id = "bench"\nname = "bench"\ncredential = "bench"\ncurrent = true\n' \
        > "$config/seer/servers.toml"
    printf '#!/bin/sh\nexec zsh %s seer %s %s\n' "$io" "$results" "$corpus" > "$work/seer-shell"
    chmod +x "$work/seer-shell"
    env -u SEER_ROOM_ENDPOINT -u SEER_ROOM_CREDENTIAL -u SEER_ROOM_KEY -u SEER_SNAPSHOT_DIR "$root/target/release/seer-runtime" \
        "$state/seer/runtimes/bench/socket" bench "$work/seer-shell" bench \
        > "$results/runtime.log" 2>&1 &
    runtime=$!
    sleep 1
}

launch() {
    local name=$1
    case $name in
        seer) echo "env XDG_CONFIG_HOME=$work/cfg XDG_STATE_HOME=$work/st $root/target/release/seer-desktop" ;;
        alacritty)
            : > "$work/alacritty.toml"
            echo "alacritty --config-file $work/alacritty.toml -o font.size=13 -e zsh $io alacritty $results $corpus" ;;
        ghostty)
            echo "ghostty --gtk-single-instance=false --config-default-files=false --font-size=13 -e zsh $io ghostty $results $corpus" ;;
        kitty)
            echo "kitty --config NONE -o font_size=13 zsh $io kitty $results $corpus" ;;
        *) echo "unknown terminal: $name" >&2; return 1 ;;
    esac
}

summarize() {
    local name=$1
    awk -v name="$name" '
        /^grid:/ { grid = $2 "x" $3 }
        / run / {
            split($0, f, /[ :,]+/)
            if (f[3] > 1) { cat[f[1]] += f[5]; da[f[1]] += f[8]; n[f[1]]++ }
        }
        END {
            printf "%s grid %s plain cat %.0f cat+DA1 %.0f sgr cat %.0f cat+DA1 %.0f\n", name, grid,
                cat["plain"] / n["plain"], da["plain"] / n["plain"], cat["sgr"] / n["sgr"], da["sgr"] / n["sgr"]
        }' "$results/io-$name.txt"
    sort -n "$results/rtt-$name.txt" | awk '{ v[NR] = $1 } END { printf "%s DA1 p50 %.3f p99 %.3f\n", "'"$name"'", v[int(NR * 0.5)], v[int(NR * 0.99)] }'
}

for entry in "${terminals[@]}"; do
    name=${entry%%:*}
    geometry=${entry#*:}
    [ "$geometry" = "$entry" ] && geometry=1500x900
    runtime=
    [ "$name" = seer ] && start_seer
    command=$(launch "$name")
    echo "load: $(cut -d' ' -f1-3 /proc/loadavg)" > "$results/load-$name.txt"
    hyprctl dispatch exec "[float; size ${geometry%x*} ${geometry#*x}; center] $command" > /dev/null
    deadline=$((SECONDS + 300))
    until grep -qs '^done' "$results/io-$name.txt" || [ $SECONDS -ge $deadline ]; do
        sleep 1
    done
    sleep 2
    pkill -f -- "$io $name $results" || true
    [ "$name" = seer ] && pkill -f -- "$root/target/release/seer-desktop" || true
    [ -n "$runtime" ] && kill "$runtime" 2> /dev/null || true
    [ "$name" = seer ] && pkill -f -- "$work/st/seer/runtimes/bench/socket" || true
    sleep 1
    cat "$results/load-$name.txt"
    summarize "$name" | tee -a "$results/summary.txt"
done
echo "results: $results"
