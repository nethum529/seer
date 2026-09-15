#!/usr/bin/env bash
# Prints n, failures, median, and p95 for each workload, condition, cache, and
# boundary in a samples file from scripts/perf/transport.sh. The p95 uses the
# nearest rank method. A failed sample is counted but has no time.
set -euo pipefail

file=${1:?usage: scripts/perf/summarize.sh samples.csv}
echo "workload,condition,cache,boundary,n,fail,median_ms,p95_ms"
tail -n +2 "$file" |
    awk -F, -v OFS=, '{ print $2, $3, $4, $6, ($8 == "ok" ? "ok" : "fail"), $7 }' |
    LC_ALL=C sort -t, -k1,4 -k6,6g |
    awk -F, '
function flush(   median, rank) {
    if (key == "") return
    if (n == 0) { printf "%s,0,%d,-,-\n", key, failed; return }
    median = (n % 2) ? v[(n + 1) / 2] : (v[n / 2] + v[n / 2 + 1]) / 2
    rank = int(0.95 * n)
    if (rank < 0.95 * n) rank++
    printf "%s,%d,%d,%.3f,%.3f\n", key, n, failed, median, v[rank]
}
{
    k = $1 "," $2 "," $3 "," $4
    if (k != key) { flush(); key = k; n = 0; failed = 0; delete v }
    if ($5 == "ok") v[++n] = $6; else failed++
}
END { flush() }'
