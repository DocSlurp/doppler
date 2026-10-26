#!/usr/bin/env bash
# Byte-for-byte parity of the release doppler-md binary against doppler_md.py golden output.
#
#   scripts/parity.sh [GOLDEN_DIR [SET]]
#
# GOLDEN_DIR holds raw/<name>.raw (doppler XHTML) and SET/<name>.md (Python output, pages
# joined by form feeds). SET defaults to golden-v23, the output of the current doppler_md.py;
# golden/ is the output of the version first ported. Prints identical/total and, for each failure, the first differing page.
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
dir=${1:-${DOPPLER_GOLDEN:-/home/dan/tmp/agents/doppler-bench.NwduaF/md-golden}}
set_name=${2:-${DOPPLER_GOLDEN_SET:-golden-v23}}
bin="$here/target/release/doppler-md"
[[ -x $bin ]] || { echo "build first: cargo build --release" >&2; exit 2; }

scratch=$(mktemp -d "${TMPDIR:-/home/dan/tmp/agents}/doppler-md-parity.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

ok=0 total=0 failed=()
for raw in "$dir"/raw/*.raw; do
  name=$(basename "$raw" .raw)
  golden="$dir/$set_name/$name.md"
  total=$((total + 1))
  if ! "$bin" "$raw" > "$scratch/out.md" 2> "$scratch/err"; then
    failed+=("$name: exit $? $(head -c 200 "$scratch/err")")
    continue
  fi
  if cmp -s "$scratch/out.md" "$golden"; then
    ok=$((ok + 1))
    continue
  fi
  # first differing page (form-feed separated) and its first differing line
  page=$(python3 -I - "$scratch/out.md" "$golden" <<'PY'
import sys
a = open(sys.argv[1], encoding='utf-8', errors='replace').read().split('\f')
b = open(sys.argv[2], encoding='utf-8', errors='replace').read().split('\f')
if len(a) != len(b):
    print(f'page count {len(a)} != golden {len(b)}', end='; ')
for i, (x, y) in enumerate(zip(a, b)):
    if x != y:
        xl, yl = x.split('\n'), y.split('\n')
        k = next((k for k, (s, t) in enumerate(zip(xl, yl)) if s != t), min(len(xl), len(yl)))
        got = xl[k] if k < len(xl) else '<end>'
        want = yl[k] if k < len(yl) else '<end>'
        print(f'page {i + 1} line {k + 1}: got {got[:120]!r} want {want[:120]!r}')
        break
PY
)
  failed+=("$name: $page")
done

echo "byte-identical: $ok/$total"
for f in "${failed[@]}"; do echo "  FAIL $f"; done
[[ $ok -eq $total ]]
