#!/bin/bash
# usage: N=5 run-baseline.sh OUT.jsonl [BINDIR]   (BINDIR empty => PATH tools = system poppler)
# Scratch cwd is under $TMPDIR (never the repo, never /tmp); pdftoppm/pdftohtml outputs land there.
set -u
export TMPDIR=${TMPDIR:-/home/dan/tmp/agents}
OUT=$1; BIN=${2:+$2/}; N=${N:-5}
S=$HOME/.cache/docslurp/pricing-samples-2026-10-21
H=$(cd "$(dirname "$0")" && pwd)
X=$(mktemp -d "$TMPDIR/perf-run.XXXXXX")
R(){ python3 -I "$H/bench2.py" "$OUT" "$N" "$@"; }
for f in uscode-title/USCODE-2024-title1.pdf sec-10k/aapl-10k-fy2025-as-filed.pdf scanned-document/inauguralofourfi00newy.pdf; do
  n=$(basename "$f" .pdf); P=$S/$f
  R pdfinfo:$n $X -- ${BIN}pdfinfo $P
  R pdftotext-raw:$n $X -- ${BIN}pdftotext -raw $P -
  R pdftotext-layout:$n $X -- ${BIN}pdftotext -layout $P -
  R pdftotext-reading:$n $X -- ${BIN}pdftotext $P -
  R pdftotext-bbox-layout:$n $X -- ${BIN}pdftotext -bbox-layout $P -
  R pdftotext-tsv:$n $X -- ${BIN}pdftotext -tsv $P -
  R pdftohtml-xml-ignore-images:$n $X -- ${BIN}pdftohtml -xml -i -stdout -q $P
  R pdfimages-list:$n $X -- ${BIN}pdfimages -list $P
  R pdffonts:$n $X -- ${BIN}pdffonts $P
  R pdftoppm-r72-gray:$n $X -- ${BIN}pdftoppm -r 72 -gray $P $X/pg
done
echo "scratch (remove when done): $X"
