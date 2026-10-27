# Extraction architecture validation — 2026-10-27

The three recommendations are implemented against merged baseline `33301dfcee436ea7dea6159416feace8a65a5b54`:

- Structured-text projection selects word facts once and computes line bounds once for XML/TSV. A first word at x=0 now stays inside its line bounds (previous xMin=21.68; corrected xMin=0). Format-specific escaping, precision and framing stay in the writers.
- Private Rust `PageLayout` owns membership, reading order and title assignment before serialization. The conversion caller no longer coordinates parallel table/geometry indices, membership resets or title suppression.
- Rule acceptance combines budget consumption, retention and completeness. The established limits remain 20,000 retained rules/page and 200,000 visible candidate attempts/document, including attempts rejected by full pages. Rust exposes upstream truncation separately from layout degradation, with zero-based library indices and one-based JSON pages.

## Quality and behavior

The user's acceptance priority is faster conversion with preserved semantic quality. Byte equality is diagnostic, not a gate against harmless formatting changes. This implementation happened to retain the tested Markdown output.

| Check | Result | Evidence |
|---|---|---|
| C++ build and registered CTests | 144/144 pass | [log](cpp-ctest.log) |
| Rust tests | 49 pass | [log](rust-cargo-test.log) |
| Real C++ → XHTML → Rust integration | All 4 cap/completeness scenarios pass | [before](pipeline-before.log), [after](pipeline-after.log) |
| C++ corpus | 91 successful PDFs × 10 modes unchanged; 4 matched failure statuses/mode | [results](corpus-results.json) |
| Rust golden corpus | 258/258 outputs unchanged | [log](rust-parity.log) |
| Seeded differential comparison | 1,000/1,000 match output, status, prior report fields and work | [results](fuzz-results.json) |
| Separate worker differential run | 1,000/1,000, seed 20261027 | [log](rust-differential.log) |
| Table scorer tests | 37 pass | [log](table-scorer-tests.log) |

The frozen Markdown judge v3.5 evaluated **254 authored-reference cases from 258 raw fixtures**. The four excluded real PDFs are US Code title 1, Apple FY2025 10-K, the Ghostscript manual and the inaugural-address scan; none has a generator-authored Markdown reference. They remain in conversion regression/timing coverage. Every per-case metric is unchanged. An empty-output sentinel scores zero content and table preservation, confirming that skipped work cannot pass.

| Metric | Baseline | Candidate | Cases |
|---|---:|---:|---:|
| Content token F1 | 0.9217 | 0.9217 | 254 |
| Ordered text similarity | 0.8554 | 0.8554 | 254 |
| Tolerant table score | 0.9020 | 0.9020 | 226 |
| Tolerant table content F1 | 0.9257 | 0.9257 | 226 |
| Paragraph-boundary F1 | 0.5140 | 0.5140 | 254 |
| Harness composite quality | 0.7666 | 0.7666 | 254 |

These results establish preserved measured quality, not uniformly excellent extraction. Paragraph boundaries remain a weaker baseline area; math has no scored cases in this slice. [Raw quality](quality.json), [harness](score_quality.py), [scoring method](scoring-method.md). Equal predictions reuse their deterministic score; changed predictions are scored independently against the same authored reference.

## Performance

Optimized builds, both arms warmed, five alternating-order blocks per workload, serial conversion launches. The shared host is measured using child user CPU as required by DOCSLURP.md; this is not wall latency or a server throughput benchmark.

| Workload | Baseline median user CPU | Candidate | Change |
|---|---:|---:|---:|
| Rust: 258 documents / 1,917 pages | 0.510074 s | 0.494604 s | −3.03% |
| C++: 477 pages, bbox-layout/wordmeta/rules | 1.927913 s | 1.948056 s | +1.04% |
| C++: 477 pages, TSV/wordmeta | 1.862484 s | 1.777669 s | −4.55% |

**Near-neutral performance; no general speedup claimed.** All slices remain within the predeclared ±5% investigation threshold. No tuning candidate was selected from these small differences. The new local membership set has no observed material regression on this corpus; table-heavy/adversarial allocation behavior is not separately profiled.

[Raw CPU samples](performance.json), [measurement contract](plan.md), [commands/harness](validate.py), [compiler/environment and candidate binary hashes](environment.json), [baseline identities and corpus](baseline.json), [source snapshot](source-snapshot.json).

## Council review

Three separate personas reviewed the same implementation, with at most two active agents. Their findings and coordinator responses are retained separately in [council disposition](council/disposition.md). No reviewer builds or independently repeated test runs are implied. Reports distinguish product findings from evidence limitations.

## Reproduction

Build C++ as documented in DOCSLURP.md and Rust with `cargo build --release --locked --offline`. Run `cargo test --locked --offline`, `ctest --test-dir build -j8 --output-on-failure`, and the real pipeline script from the repository root:

```sh
python3 -I docslurp/doppler-md/scripts/check_pipeline.py \
  --pdftotext build/utils/pdftotext \
  --converter docslurp/doppler-md/target/release/doppler-md
```

The retained performance/quality harnesses identify this machine's baseline binaries, fixtures and frozen judge by absolute path; they require those existing assets. They are evidence/reproduction scripts, not portable CI entry points. No paid compute or external model scoring was used. Source changes after implementation validation were documentation only.
