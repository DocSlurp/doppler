# Final validation — supersedes historical candidate results

Validated implementation cf566e64f9c73ed367659c9a6f6ee469f31eb09a against
b7684b572e4ca3f56361e95af748cc0d6b985988 after the color-budget fix.
Release rebuild and Rust tests passed (see logs). Fresh actual-accounting screen:
254 cases, zero candidate HTML degradation, zero HTML differences and zero GFM
differences against baseline. All four output/report arms are retained here.
No expensive scorer rerun: byte equality implies unchanged deterministic scorer
inputs/results relative to baseline; this is not a newly measured quality score.
Historical improvement/regression claims do not describe this final implementation.

The generated CC0 33-page fixture passes exact-once labels, page slots, 32+1 chunk
merging and all 33 combined spans. cpu.json retains five interleaved runs per arm,
child user+system CPU seconds and host load. Tiny synthetic timings are screening
only, not capacity measurements. page33.png is fresh final-chunk evidence.
No deployed defaults, VM deployment, customer data or paid calls were exercised.

Integrity is BLOCKED, not passed: supplied wild-v2 catalog, manifest and reference
metadata lack provenance.author and provenance.createdAt required by
 docslurp-authored-eval@v1. Generator hashes, upstream commit and numeric seed do
not establish these fields. Manifest construction and gate execution stopped
without inventing metadata. No scanner changes or product-path exclusions were
made; potential docslurp-review matches have not been evaluated or waived.
See integrity-status.json and provenance.json for exact base/head and hashes.

Reproduce with BASELINE_BINARY pointing to a release build of the recorded base,
PATH including Cargo, TMPDIR/TMP/TEMP=/home/dan/tmp/agents and PDFTOTEXT pointing
to build/utils/pdftotext. Run verify.py with the raster-eval venv using -I;
run synthetic.py with Python. Historical failed evidence remains untouched.
