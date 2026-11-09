# Performance tuning: late page-range extraction

## Decision and operating bounds

- User-visible operation and goal: make `pdftotext -f FIRST -l LAST` fast and memory-efficient when a short range starts late in a large PDF.
- Scope / preserved behavior: identical extracted output and page-tree error behavior; no trusting forged `/Count` values; no public API break.
- Time / spend: one local session, no paid or remote work. Artifacts live in `/home/dan/tmp/agents/poppler-page-range`.
- Stop rule: integrate only a correctness-preserving candidate with a meaningful CPU or peak-RSS gain; otherwise keep the baseline and record the diagnosis.
- Project priority: correctness first, then peak RSS and user CPU for equal-length late ranges.

## Frozen measurement contract

| Scenario | Input and completion oracle | Objective | Gates |
|---|---|---|---|
| Early slice | Deterministic balanced 50,000-page synthetic PDF; pages 1-10; exactly 10 form feeds | Reference cost | Exit 0, exact page count |
| Late slice | Same PDF; pages 49,991-50,000; exactly 10 form feeds | At least 20% less user CPU or RSS | Same output as baseline; existing tests pass |

- Source revision: `3a40db737`; RelWithDebInfo build, GCC C++23.
- Host: x86_64 Linux, Intel i7-7700K (8 logical CPUs), 62 GiB RAM.
- Observation unit: a fresh child process. Screening uses three runs; frozen candidate confirmation uses eight paired AB/BA blocks.
- Metrics: child user CPU seconds and `wait4` peak RSS MiB. Profiling, if needed, is diagnostic only.
- Minimal arm: traverse the page tree without constructing irrelevant `Page` objects; eligible only if malformed-tree behavior stays compatible.

## Hypotheses and experiments

| ID | Mechanism / prediction | Falsifier | Decision |
|---|---|---|---|
| H1 | `Catalog::cachePageTree(first)` constructs and retains every preceding `Page`; late-slice CPU/RSS rises sharply with `FIRST`. | Late and early equal-length slices have similar cost. | Confirmed: baseline late slice used 6.3x RSS and 10.2x CPU in screening. |
| H2 | Rendering selected pages dominates; reducing page-tree allocations will not materially change the result. | A traversal-only probe removes most late-range overhead. | Rejected: omitting skipped `Page`/`PageAttrs` objects cut RSS by about 82%. |
| H3 | XRef parsing/I/O dominates independently of page objects. | Profiles and allocation ablation show page construction is minor. | Partly supported: leaf traversal remains, but removing skipped vector/map bookkeeping produced a confirmed CPU gain. |

## Confirmation and integration

- Accepted candidate: `pdftotext` tells `Catalog` that it is a one-way consumer starting at `-f`. The catalog still fetches and validates every skipped leaf, but constructs and indexes only selected/later `Page` objects.
- Eight interleaved runs on one 50,000-page fixture, exact selected labels and form-feed counts checked every run:
  - late pages 49,991-50,000: median user CPU 0.4707 -> 0.3453 seconds (26.6% lower); median peak RSS 145.6 -> 23.3 MiB (84.0% lower);
  - early pages 1-10: 0.0567 -> 0.0634 seconds and 23.0 -> 23.2 MiB; the small CPU difference is within the noisy startup-scale slice and is not claimed as an improvement.
- Regression protection: `docslurp-pagecount` now extracts the final ten pages of a nested 250-page tree. The benchmark independently checks exact visible page identities.
- Validation: all 15 pagecount/pdftotext/smoke tests pass after a complete dependent rebuild. Full suite: 128/148 pass; 20 unrelated environment failures are signing fixtures/keyrings, missing Valgrind loader debuginfo, or dependent not-runs.
- Reproduce confirmation with `python3 performance/page-range-slicing/bench.py --binary base=/path/to/baseline/pdftotext --binary candidate=build/utils/pdftotext --fixture /path/to/pages-50000.pdf --pages 50000 --runs 8`.

## Findings and next action

- Outcome: accepted. A short late slice now retains memory proportional to the selected/later cached pages rather than the skipped prefix, while preserving the full structural walk and its error checks.
- Remaining cost: XRef/object fetches for skipped leaves are still linear in the range start. Skipping subtrees via `/Count` was rejected because forged counts would change page identity unless paired with a separate validating walk.
