#!/bin/bash
# usage: N=5 run-proto.sh OUT.jsonl CHECKOUT_BUILD_DIR PROTO_BIN
# Compares checkout-build pdftotext (26.10.90, Release) with the pdf2md_proto probe linked to the same libpoppler.so.
set -u
export TMPDIR=${TMPDIR:-/home/dan/tmp/agents}
OUT=$1; BB=$2; PROTO=$3; N=${N:-5}
S=$HOME/.cache/docslurp/pricing-samples-2026-10-21
H=$(cd "$(dirname "$0")" && pwd)
X=$(mktemp -d "$TMPDIR/perf-proto.XXXXXX")
R(){ python3 -I "$H/bench2.py" "$OUT" "$N" "$@"; }
for f in sec-10k/aapl-10k-fy2025-as-filed.pdf uscode-title/USCODE-2024-title1.pdf; do
  n=$(basename "$f" .pdf); P=$S/$f
  R co-pdftotext-layout:$n $X -- $BB/utils/pdftotext -layout $P -
  R co-pdftotext-bbox-layout:$n $X -- $BB/utils/pdftotext -bbox-layout $P -
  R proto-null-j1:$n $X -- $PROTO --mode null $P
  R proto-text-j1:$n $X -- $PROTO --mode text $P
  R proto-md-notables-j1:$n $X -- $PROTO --mode md --tables 0 $P
  R proto-md-j1:$n $X -- $PROTO --mode md $P
  R proto-md-j4:$n $X -- $PROTO --mode md -j 4 $P
  R proto-md-j8:$n $X -- $PROTO --mode md -j 8 $P
done
echo "scratch (remove when done): $X"
