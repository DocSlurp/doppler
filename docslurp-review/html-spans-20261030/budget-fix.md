# HTML span budget repair — 2026-10-30

Base: 965411d40. Final implementation SHA is the commit containing this report;
revision outputs are in budget-revision-20261030 (never substituted for old scores).

## Threat and implementation
Hostile PDF-derived XHTML could run repeated topology scans, Cartesian allocations,
word cloning and escaping before any page/document work accounting, even for GFM.
Pass the output format and shared Budget into ruled discovery. Only HTML reserves
discovery and span work. Reservations precede scans, sorts, allocations, cloning
and escaping. Exhaustion returns None through PageLayout into the established
PageWork/DocWork plain-lines path. Budget addition saturates. Sparse per-line
cell maps replace rects-per-line buckets.

Reservations are deliberately conservative upper bounds, not measured instruction
counts. Discovery reserves quadratic line/rule work; dense HTML pages can therefore
degrade earlier than previously. This needs corpus calibration before promotion.
GFM retains its existing discovery behavior/accounting and never calls ruled_html.

Shading forces conventional grid fallback because discovery uses boxed boundaries.
Words touching/crossing cell boundaries also decline spans. Multiple source-line
fragments in one cell decline spans and use established grid assembly; no invented
spaces or independent per-line bidi concatenation. This is conservative fallback,
not a new multi-line visual-row reconstruction algorithm.

## Validation
Full cargo test passed; release build passed with requested PATH/PDFTOTEXT/temp
environment. Logs: budget-test.log, budget-build.log.
Public combined-span test now checks tiny page budget, cumulative document
exhaustion, GFM span-work exclusion, dense 20,000-line input, repeated overlapping
candidates, crossing/exact-boundary words and multilingual detached fragments.
Existing GFM exact work assertions and default HTML fixtures pass.

Measured authored synthetic first page (six table words plus page marker):
- HTML max_page_work=1: page_work_max=2048 (failed reservation), page_work degradation,
  no table, no span.
- HTML defaults: page_work_max=32860, no degradation, combined span retained.
- GFM defaults: page_work_max=2, no degradation, no span path.
See budget-revision-20261030/measured.json and corresponding Markdown.

## Blockers and remaining review
Existing synthetic.py, screen.py (254 cases) and chunk_screen.py were copied into
the new revision directory and attempted. All stop because their recorded baseline
binary no longer exists:
 /home/dan/tmp/agents/html-spans.I5sBUD/docslurp/doppler-md/target/release/doppler-md
No worktree was created and no substitute baseline was silently chosen.
Original evidence is untouched. New logs are failures, not new quality results.
The full comparison and regression-family reassessment remain blocked; no claim
of corpus default non-regression or production readiness is made.

The independent core integrity judge was invoked read-only against wild-v2 and
failed closed with unsupported-catalog-schema, scanning zero files/cases.
It is not an integrity pass. No paid calls or benchmark-specific production logic.

Remaining: restore/reconstruct exact baseline under parent authorization, complete
254-case comparison and chunk check, inspect default degradation and the listed
fidelity families, add broader perturbation/separate-candidate coverage and obtain
independent re-review. Conservative fallback sacrifices valid fragmented spans.
