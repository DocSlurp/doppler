#!/usr/bin/env bash
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH" TMPDIR=/home/dan/tmp/agents TMP=/home/dan/tmp/agents TEMP=/home/dan/tmp/agents
cd "$(dirname "$0")/../.."
scratch=$(mktemp -d "$TMPDIR/html-spans.XXXXXX")
git archive b7684b572e4ca3f56361e95af748cc0d6b985988 docslurp/doppler-md | tar -x -C "$scratch"
printf '%s\n' "$scratch" > docslurp-review/html-spans-20261030/baseline-path.txt
cargo build --release --manifest-path "$scratch/docslurp/doppler-md/Cargo.toml"
