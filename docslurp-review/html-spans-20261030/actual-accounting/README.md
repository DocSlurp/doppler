# Actual span work accounting

Removed HTML-only speculative discovery reservations; baseline discovery remains shared.
The span renderer uses Result<Option<String>, ()> to distinguish unsupported evidence from
shared-budget exhaustion. Rules, actual axis sorts, edge scans and segment sorts,
cell storage/traversal, sparse word buckets, line assembly, and text rendering are charged
before their associated operations. GFM never enters the span renderer.

Validation: full cargo test and release build passed. All 254 cases were extracted afresh
with reports for candidate/baseline HTML and GFM. Candidate HTML degraded cases: 0
(previous HEAD: 62). HTML differences from baseline: 0; GFM differences: 0.
No scoring rerun was needed: all scorer input bytes match baseline exactly. This establishes
restoration of baseline outputs, not a new independently measured quality score.
The separate integrity schema adapter remains unresolved; no integrity pass is claimed.

The overlapping-candidate test no longer assumes greater total work than a successful
render: an early decline can legitimately cost less. It still checks charged work and
low-budget degradation. Existing page/document budget and dense-input tests pass.
New unit coverage verifies cheap shaded rejection and distinct exhaustion.

Original evidence, baseline-path.txt, postfix-331863c, and rust-rewrite-20261025
were preserved. This directory contains fresh output, report, and comparison evidence.
