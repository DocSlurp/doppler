# Distinct margin text preservation

The default `furniture_preserve_headings=true` requires literal repetition (case-folded and trimmed, counted once per page and separately per margin) before deleting a running header or footer. Text differing in numbers is retained regardless of font size, body distance or split-line layout. Recognized isolated page numbers still drop. Setting the option to false restores aggressive digit-normalized repetition matching.

This favors retaining content over removing variable running headers/footers. Identical repeated real titles or fragments can still be removed. It is not a lossless-title guarantee.

## Current validation

- 79 Rust tests pass; release build passes. New regression was observed failing before the fix.
- Original pilot retains all four titles; previously failing confirmation now retains all three. Exact publication headers and Page n footers still drop.
- Fresh 254-case comparison against the retained pre-heading baseline changes 166 outputs. One case changes table layout: a recovered section prefix causes an existing payment block to be represented as a table. No unchanged-corpus claim applies to this change. Added variable footer text is expected.
- Independent integrity gate passes, with no findings. Raw replay and corpus outputs are retained in the app performance/header-deletion-fix-2026-10-30 directory.
- Independent adversarial and fidelity reviews found no high security issue. The fidelity reviewer requested a new corpus/table comparison; the figures above resolve that validation gap, but do not establish end-to-end app fidelity.
- The optional frozen full-text scoring pass was interrupted after conversion; no final aggregate quality scores are claimed.
- No deployment, live VM test, provider calls or app-pin update.

## Historical evidence (superseded candidate)

# Conservative numbered-heading preservation

Margin furniture is grouped by top/bottom band and counted once per page. Exact literal repetitions keep their own counts. A line that repeats only after digit normalization may now be retained if it is a short, prominent top-band heading, has nearby smaller body text outside the margin, and is not a fragment sharing a vertical start with another line. The checks reuse `h2_ratio`, `heading_max_chars`, `minor_heading_ratio` and `title_gap`.

`furniture_preserve_headings` defaults to true. Setting it to false disables the new heading exemption; it does not restore cross-margin grouping. Exact repeated headers, ordinary-size numbered running headers and isolated page numbers still drop. Typography evidence uses linear word scans, not additional median allocations/sorts. Cached body and line-start evidence is computed at most once per page and only when normalized repetition could cause removal.

This is intentionally conservative. Split headings, multicolumn body association, unusual typography and larger title-to-body gaps may prevent recovery. It is not a semantic classifier and cannot guarantee every title is retained.

## Evidence and limits

- Full Rust suite: 78 tests pass; release build passes.
- Four-page authored integration pilot: four titles recovered (previously none), table bytes unchanged.
- Final 254-case synthetic public corpus comparison: all outputs byte-identical to baseline, all tables unchanged, zero degradation. This establishes regression preservation, not corpus-wide title gains. Frozen v3.6 scorer unchanged; the rejected first candidate and its scores remain in external evidence.
- Independent three-page authored PDF confirmation: two titles recovered (previously none), body retained and genuine running header/page numbers removed. The full three-of-three title-retention gate **failed**. The smaller heading exceeded the conservative proximity bound. No tuning was performed against that confirmation. This remains an experimental draft, not validated broad title recovery.
- Independent authored-provenance integrity gate passed on the working production diff (8,435 files scanned, 8,164 production files); no fixture names/answers or score-weight changes in product code. The initial sandbox-blocked scan is retained separately.

Six interleaved process runs per arm, user+system CPU median, same host and `pdftotext`, `pdftomarkdown --html`:

| Workload | Pages | Baseline seconds | Candidate seconds |
| --- | ---: | ---: | ---: |
| Authored pilot | 4 | 0.011779 | 0.011503 |
| Public synthetic annual report | 9 | 0.039055 | 0.035507 |
| Public Ghostscript manual | 477 | 1.790243 | 1.778953 |

No speedup is claimed. The public-manual baseline range was 1.680–2.025 seconds, candidate 1.762–1.921 seconds; host noise exceeds the median difference. The two non-pilot workloads emitted equal output hashes between arms. Timing is local CLI CPU, not deployed ingestion latency.

## Reviews

Independent initial adversarial/fidelity reviewers identified exact-header contamination by other numbered lines and added median work before budgets. Revisions count exact literals per margin and use linear min/all typography scans. Fragment rejection also prevents the first candidate's changed form layout. Independent final reviewers reproduced prior cases and established no remaining medium/high issue in their scope. Their low findings (unnecessary preprocessing and rollback wording) were addressed locally. Reviews do not establish exhaustive resource bounds, every layout, VM integration or approval to merge.

## PDFust integration baseline

DocSlurp uses PDFust for rendering/probing as well as optional extraction surfaces; this pass measured the actual binding adapter's page-render helper. Dependency-complete core imports repaired the initial harness failure before processing. The loaded binding reports version 0.13.0 / source f702a7a0e59e7a4285a4eddb798d9608d936d9c1, which differs from the current PDFust checkout; no source-version equivalence is claimed.

Six interleaved four-page/100-DPI runs per lane count produced identical image hashes. One lane: median wall 36.16 ms, CPU 36.55 ms. Four lanes: wall 12.77 ms, CPU 41.72 ms. Four lanes are already the default; this is configuration evidence, not a new engine optimization. No PDFust code or global concurrency default changed. Broader rendering/probing throughput and live VM behavior are unmeasured.

Raw outputs, hashes, source provenance, rejected-candidate evidence, scripts and measurements are retained in the app's `performance/engine-pass-2026-10-30/`. Only aggregate evidence is committed here. No customer inputs, paid calls, merges, deployments or releases were used.
