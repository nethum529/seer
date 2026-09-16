#!/usr/bin/env bash
# Prints one row per workload and size from the windows and samples files of
# scripts/perf/screen.sh. Per update values pool every repeat. Rates divide
# the total bytes and updates by the total window time. bytes columns are
# the full frames of today, diff_bytes the scroll diff message, sent_bytes
# the smaller of the two per update (the cap rule). shifted_share is the
# share of updates whose diff moved rows, full_share the share that the cap
# rule sends as a full frame, apply_failures the count of diffs that did not
# give the current frame. The after_keys columns only count updates that
# arrive after the last key of a repeat, over the time from that key to the
# last update. The p95 uses the nearest rank method. floor_failures counts
# updates where cells_floor was above the real Cells bytes, final_mismatches
# the repeats whose screen after the run differed from the Resync answer.
set -euo pipefail

windows=${1:?usage: scripts/perf/summarize_screen.sh windows.csv samples.csv}
samples=${2:?usage: scripts/perf/summarize_screen.sh windows.csv samples.csv}
echo "workload,cols,rows,repeats,keys,window_s,updates,updates_per_s,bytes_per_s,bytes_median,bytes_p95,bytes_max,cells_median,cells_p95,shifted_share,diff_cells_median,diff_cells_p95,diff_bytes_median,diff_bytes_p95,sent_bytes_median,sent_bytes_p95,sent_per_s,full_share,apply_failures,after_keys_updates_per_s,after_keys_bytes_per_s,after_keys_sent_per_s,floor_failures,final_mismatches"
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
    if ($11 == 0) mismatches[k]++
    next
}
{
    k = $3 "," $4 "," $5
    n[k]++
    for (c = 8; c <= 15; c++) value[k, c, n[k]] = $c
    sent[k] += $15
    if ($12 != 0) shifted[k]++
    if ($16 == "full") full[k]++
    if ($17 == 0) failures[k]++
    if ($19 == 0) floor_failures[k]++
    if ($7 > lastkey[$1, $2, k]) {
        tail_updates[k]++
        tail_bytes[k] += $8
        tail_sent[k] += $15
        span = $7 - lastkey[$1, $2, k]
        if (span > tail_span[$1, $2, k]) tail_span[$1, $2, k] = span
    }
}
END {
    for (k in reps) {
        s = window[k] / 1000
        tail_ms = 0
        for (key in tail_span) if (index(key, SUBSEP k) > 0) tail_ms += tail_span[key]
        if (n[k] == 0) { print k, reps[k], keys[k], s, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, mismatches[k] + 0; continue }
        sorted(k, 8, n[k], b); sorted(k, 11, n[k], cells); sorted(k, 13, n[k], dc)
        sorted(k, 14, n[k], db); sorted(k, 15, n[k], sb)
        printf "%s,%d,%d,%.1f,%d,%.1f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.2f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.0f,%.2f,%d,%.1f,%.0f,%.0f,%d,%d\n", k, reps[k], keys[k], s, n[k],
            n[k] / s, bytes[k] / s, median(b, n[k]), pct(b, n[k], 0.95), b[n[k]],
            median(cells, n[k]), pct(cells, n[k], 0.95),
            shifted[k] / n[k], median(dc, n[k]), pct(dc, n[k], 0.95),
            median(db, n[k]), pct(db, n[k], 0.95),
            median(sb, n[k]), pct(sb, n[k], 0.95), sent[k] / s,
            full[k] / n[k], failures[k],
            rate(tail_updates[k], tail_ms), rate(tail_bytes[k], tail_ms), rate(tail_sent[k], tail_ms),
            floor_failures[k], mismatches[k]
    }
}' "$windows" "$samples" | LC_ALL=C sort -t, -k2,2n -k1,1
