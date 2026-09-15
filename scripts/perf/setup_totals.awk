# Adds the setup boundaries of each sample into one setup_total line in the
# samples format, so scripts/perf/summarize.sh can read it. Issue 390.
# Usage: awk -f scripts/perf/setup_totals.awk samples.csv
BEGIN { FS = OFS = ","; split("runtime_start endpoint_bind address_lookup dial_handshake stream_open seer_dial seer_dial_session", b, " "); for (i in b) setup[b[i]] = 1 }
NR == 1 { print; next }
$2 !~ /_(dial|session)$/ { next }
$6 == "process_spawn" { proc++; next }
setup[$6] {
    k = proc SUBSEP $5
    if (!(k in row)) { order[++n] = k; row[k] = $1 OFS $2 OFS $3 OFS $4 OFS $5 }
    total[k] += $7
    if ($8 != "ok") bad[k] = $8
}
END { for (i = 1; i <= n; i++) { k = order[i]; printf "%s,setup_total,%.3f,%s\n", row[k], total[k], (k in bad ? bad[k] : "ok") } }
