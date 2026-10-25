#!/bin/bash
# Load-independent cost: instructions executed (callgrind) by each pdftotext binary.
# usage: bench_ir.sh PDF FIRST LAST label=binary ... -- pdftotext-args...
pdf=$1 first=$2 last=$3; shift 3
bins=(); while [ "$1" != "--" ] && [ $# -gt 0 ]; do bins+=("$1"); shift; done; shift
tmp=$(mktemp -d "${TMPDIR:-/home/dan/tmp/agents}/bench-ir.XXXXXX")
for lb in "${bins[@]}"; do
  label=${lb%%=*} bin=${lb#*=}
  valgrind --tool=callgrind --callgrind-out-file="$tmp/$label.out" "$bin" -f "$first" -l "$last" "$@" "$pdf" /dev/null >/dev/null 2>"$tmp/$label.err"
  printf '%-8s %s instructions\n' "$label" "$(grep -h 'Collected' "$tmp/$label.err" | awk '{print $4}')"
done
rm -rf "$tmp"
