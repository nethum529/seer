#!/usr/bin/env zsh
# Runs as the shell of the terminal under test. Driver: scripts/perf/terminal.sh.
# Usage: terminal_io.zsh <name> <result-dir> <corpus-dir>
# A terminal answers DA1 only after it parses every earlier byte, so
# cat plus DA1 is the time to parse all the output, not only to drain it.
zmodload zsh/datetime
name=$1
results=$2
corpus=$3
io=$results/io-$name.txt
rtt=$results/rtt-$name.txt
: > $io
: > $rtt
sleep 2
print -- "grid: $(stty size)" >> $io
stty -icanon -echo
da() { local reply; printf '\e[c'; read -rs -d c reply; }
da
for file in plain sgr; do
  for run in 1 2 3 4 5 6; do
    t0=$EPOCHREALTIME
    command cat $corpus/$file.txt
    t1=$EPOCHREALTIME
    da
    t2=$EPOCHREALTIME
    printf '%s run %d: cat %.1f ms, cat+DA1 %.1f ms\n' $file $run \
      $(( (t1 - t0) * 1000 )) $(( (t2 - t0) * 1000 )) >> $io
  done
done
printf '\e[2J\e[H'
sleep 1
for run in {1..300}; do
  t0=$EPOCHREALTIME
  da
  t1=$EPOCHREALTIME
  printf '%.3f\n' $(( (t1 - t0) * 1000 )) >> $rtt
  sleep 0.01
done
stty sane
print -- done >> $io
sleep 1
