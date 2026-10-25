#!/bin/bash
# Reproduces the isolated scratch build + prototype used for results/proto-*.jsonl and image-placement-cost.*.
# Nothing is written inside the checkout; everything lives in a mktemp dir under $TMPDIR. Prints that dir.
# Library sources used for the recorded results: checkout at e658c5562 (poppler/goo/fofi/splash/utils are
# byte-identical at 4ca703c55; `git diff --stat e658c5562 HEAD -- poppler goo utils fofi splash` is empty).
set -eu
export TMPDIR=${TMPDIR:-/home/dan/tmp/agents}
R=${1:-/home/dan/code/oss/poppler}
H=$(cd "$(dirname "$0")/.." && pwd)
W=$(mktemp -d "$TMPDIR/pp-perf.XXXXXX")
cmake -S "$R" -B "$W/build" -G Ninja -DCMAKE_BUILD_TYPE=Release -DENABLE_QT5=OFF -DENABLE_QT6=OFF -DENABLE_GLIB=OFF \
  -DENABLE_CPP=OFF -DENABLE_BOOST=OFF -DBUILD_GTK_TESTS=OFF -DBUILD_QT5_TESTS=OFF -DBUILD_QT6_TESTS=OFF \
  -DBUILD_CPP_TESTS=OFF -DBUILD_MANUAL_TESTS=OFF -DENABLE_GPGME=OFF -DENABLE_NSS3=OFF -DENABLE_DCT_DECODER=libjpeg \
  -DTESTDATADIR=/nonexistent > "$W/cmake.log" 2>&1
ninja -C "$W/build" poppler pdftotext pdftohtml pdfimages > "$W/ninja.log" 2>&1
g++ -std=c++20 -O2 -DHAVE_CONFIG_H "$H/proto/pdf2md_proto.cc" -I"$W/build" -I"$W/build/poppler" -I"$R" -I"$R/poppler" \
  -I"$R/goo" -I"$R/fofi" -I"$R/splash" -L"$W/build" -lpoppler -Wl,-rpath,"$W/build" -o "$W/pdf2md-proto"
echo "$W"
