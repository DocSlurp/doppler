#!/usr/bin/env bash
# usage: bench.sh RECORDER_BIN OUT.tsv  ; 5 runs each, reports median wall ms. System poppler tools = 26.10.0 (/usr/bin).
REC=$1; OUT=$2; S=~/.cache/docslurp/pricing-samples-2026-10-21
PDFS=("$S/sec-10k/aapl-10k-fy2025-as-filed.pdf" "$S/uscode-title/USCODE-2024-title1.pdf" "$S/scanned-document/inauguralofourfi00newy.pdf")
cmds=("pdftotext __ -" "pdftotext -layout __ -" "pdftotext -bbox-layout __ -" "pdftohtml -xml -i -stdout -nodrm __" "pdftocairo -svg __ /dev/null" "$REC null __" "$REC text __" "$REC full __")
echo -e "pdf\tcmd\tmedian_ms\truns_ms" > $OUT
for pdf in "${PDFS[@]}"; do for c in "${cmds[@]}"; do
  ts=(); for i in 1 2 3 4 5; do
    cmd=${c//__/$pdf}; a=$(date +%s%N); eval "$cmd" >/dev/null 2>&1; b=$(date +%s%N); ts+=($(( (b-a)/1000000 )))
  done
  med=$(printf '%s\n' "${ts[@]}" | sort -n | sed -n 3p)
  echo -e "$(basename $pdf)\t${c//__/PDF}\t$med\t${ts[*]}" >> $OUT
done; done
