#!/usr/bin/env bash
# Prints one row per workload and size from the windows and samples files of
# scripts/perf/screen.sh. Per update values pool every repeat. Rates divide
# the total bytes and updates by the total window time. The after_keys
# columns only count updates that arrive after the last key of a repeat,
# over the time from that key to the last update. The p95 uses the nearest
# rank method.
set -euo pipefail

windows=${1:?usage: scripts/perf/summarize_screen.sh windows.csv samples.csv}
samples=${2:?usage: scripts/perf/summarize_screen.sh windows.csv samples.csv}
echo "workload,cols,rows,repeats,keys,window_s,updates,updates_per_s,bytes_per_s,bytes_median,bytes_p95,bytes_max,cells_median,cells_p95,rows_median,row_diff_median,row_diff_p95,row_diff_per_s,cell_diff_median,cell_diff_p95,cell_diff_per_s,after_keys_updates_per_s,after_keys_bytes_per_s"
awk -F, -v OFS=, '
function pct(values, n, p,   rank) {
    rank = int(p * n)
    if (rank < p * n) rank++
    if (rank < 1) rank = 1
    return values[rank]
}
function median(values, n) {
    return (n % 2) ? values[(n + 1) / 2] : (values[n / 2] + values[n / 2 + 1]) / 2
}
function sorted(key, column, n, out,   i) {
    delete out
    for (i = 1; i <= n; i++) out[i] = value[key, column, i]
    asort(out)
}
function rate(amount, ms) {
    return ms > 0 ? amount / (ms / 1000) : 0
}
FNR == 1 { next }
FILENAME == ARGV[1] {
    k = $3 "," $4 "," $5
    reps[k]++
    keys[k] += $6
    lastkey[$1, $2, k] = $7
    window[k] += $8
    updates[k] += $9
    bytes[k] += $10
    next
}
{
    k = $3 "," $4 "," $5
    n[k]++
    for (c = 8; c <= 14; c++) value[k, c, n[k]] = $c
    if ($7 > lastkey[$1, $2, k]) {
        tail_updates[k]++
        tail_bytes[k] += $8
        span = $7 - lastkey[$1, $2, k]
        if (span > tail_span[$1, $2, k]) tail_span[$1, $2, k] = span
    }
}
END {
    for (k in reps) {
        s = window[k] / 1000
        tail_ms = 0
        for (key in tail_span) if (index(key, SUBSEP k) > 0) tail_ms += tail_span[key]
        if (n[k] == 0) { print k, reps[k], keys[k], s, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0; continue }
        sorted(k, 8, n[k], b); sorted(k, 11, n[k], cells); sorted(k, 12, n[k], rows)
        sorted(k, 13, n[k], rd); sorted(k, 14, n[k], cd)
        rdsum = 0; cdsum = 0
        for (i = 1; i <= n[k]; i++) { rdsum += rd[i]; cdsum += cd[i] }
        printf "%s,%d,%d,%.1f,%d,%.1f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.1f,%.0f\n", k, reps[k], keys[k], s, n[k],
            n[k] / s, bytes[k] / s, median(b, n[k]), pct(b, n[k], 0.95), b[n[k]],
            median(cells, n[k]), pct(cells, n[k], 0.95), median(rows, n[k]),
            median(rd, n[k]), pct(rd, n[k], 0.95), rdsum / s,
            median(cd, n[k]), pct(cd, n[k], 0.95), cdsum / s,
            rate(tail_updates[k], tail_ms), rate(tail_bytes[k], tail_ms)
    }
}' "$windows" "$samples" | LC_ALL=C sort -t, -k2,2n -k1,1
