# Performance Wizard — round 1

Verdict: **approve from the performance perspective; no blocking findings.** The implementation satisfies the reviewed ownership/accounting contract without a demonstrated material throughput regression. Two low-priority optimization followups are justified by source inspection, but neither is a measured bottleneck or a prerequisite to landing. Do not claim a speed win.

Reviewed branch `docslurp/deepen-extraction-modules` against `33301dfcee436ea7dea6159416feace8a65a5b54`, including working-tree changes and new product/test/evidence files. Excluded `docslurp-review/rust-rewrite-20261025`. Applied the Council Performance Wizard brief, codebase-design/DEEPENING guidance, DOCSLURP.md, and applicable storage/read-only instructions. No builds, tests, network work, product edits, or agent delegation were performed. This report is independent of the other Council reports and their coordinator disposition. Evidence observed through 2026-10-27 17:20 UTC; quality results completed during inspection.

## Optional findings

### P3 — dense membership incurs avoidable hashing and bucket storage

Location: `docslurp/doppler-md/src/page_layout.rs:99–106` (`replace_members`), called at lines 75 and 124.

The baseline used dense line-index flags. The replacement hashes every detected member and performs a hash lookup for each surviving line candidate. On a table-heavy page/region with thousands of dense IDs, this adds hashing, bucket growth, and substantially more membership storage per entry than flags. This is a concrete constant-factor cost, not an asymptotic regression: both implementations remain linear for this step. Empty/no-table regions skip the lookup pass, and the new implementation avoids the baseline's unconditional page-sized flag arrays and explicit reset loop, so a blanket claim that HashSet is slower overall would be wrong.

Optional action: measure dense table-heavy pages separately, including allocations/peak memory, and compare a private scoped bitmap or internally owned generation marks. Keep lifetime and overlap correctness inside PageLayout/its helper; do not restore caller-managed membership resets. Benchmark sparse/many-small-region cases too, since zeroing a page-sized bitmap per region can erase the saving. The current aggregate Rust measurement is slightly lower CPU, so this is an optimization candidate rather than a regression finding.

### P3 — filter placement references before cloning

Location: `docslurp/doppler-md/src/layout.rs:247–248`.

The new generic element payload changes `.copied()` to `.cloned()`, but cloning still precedes the left/right geometry filter. With `Option<Rc<PlacedTable>>`, every examined table is now reference-counted in both candidate collections, including placements immediately rejected by a filter. Repeated gutter searches on a page containing many ruled tables amplify these unnecessary increment/decrement pairs. Line-only elements have no such reference-count traffic.

Optional action: filter the borrowed elements first, then clone only accepted placements. This preserves element order and overlap behavior and requires no output-format compromise. The expected saving is limited to rejected table-placement clones; no timing evidence isolates its magnitude, and existing work limits bound the search. This is not evidence of a harmful overall Rc regression.

## Sharing, memory, and work considered

- `Rc<Table>` already existed in the baseline. Keeping it avoids copying cell strings when layout elements overlap or move through prose grouping. It uses non-atomic reference counts, not locks. I reject treating all table sharing as newly introduced overhead.
- The outer `Rc<PlacedTable>` at `page_layout.rs:73,122` adds one heap allocation per detected table and count traffic during geometry cloning. Its benefit is cheap sharing of an owned placement without parallel table/box index vectors. Gutter placements are created after splitting and immediately lowered to `Seq`, so avoiding that outer allocation there is a plausible later simplification. There is no allocation profile or measured regression establishing that this additional complexity is worthwhile now.
- At `page_layout.rs:63–75`, all kept line placements are built before ruled members are removed with `retain`. A page dominated by ruled tables temporarily constructs and retains vector capacity for lines the baseline filtered before construction. Later, `blocks` retains gutter grids until page serialization rather than serializing each region immediately. These are credible memory differences on dense pages; peak RSS was not measured. Default 20,000-line fallback and existing work limits constrain ordinary layout, but do not prove equal memory usage. This is a residual measurement gap, not a demonstrated OOM or blocker.
- `page_layout.rs:195–198` still clones cell rows when inserting a title for Markdown rendering. The baseline's `TableItem::markdown` did the same and also cloned the header. An iterator/view-based renderer could remove those row/string copies if profiling identifies titled large tables as important. I reject labeling this an introduced regression or demanding a rewrite solely to avoid harmless formatting differences.
- C++ `projectWord/projectLine` (`utils/pdftotext.cc:255–295`) computes logical text once per word, collects metadata only when requested, and projects each line in one pass without an additional retained word collection. The original font-name helper already copied the font string. The refactor does not introduce an obvious extra text allocation or per-word encoding lookup. Initializing bounds from the first word fixes zero/negative-coordinate handling without a costly compatibility path.

## Benchmark assessment

Source: `docslurp-review/architecture-20261027/performance.json`, method at `validate.py:109–140`.

Five measured blocks follow warmups, alternate arm order, launch children serially, require successful exit, and measure child user CPU using resource accounting. This follows DOCSLURP.md. Parent Python launch/scoring CPU is excluded; child process startup and conversion are included. `/dev/null` output includes serialization but excludes meaningful downstream storage/pipe backpressure. C++ baseline and candidate caches both use RelWithDebInfo, `/usr/bin/c++`, and `-O2 -g -DNDEBUG`; Rust uses release binaries. Binary hashes and environment are recorded. Although retained baseline worktrees have different HEAD commits, their relevant C++ directories and Rust package respectively have no committed diff from the requested baseline, and no local modifications were found in those inspected scopes.

- Rust, 258 documents: median user CPU **0.510074 → 0.494604 s**, **−3.03%**; total CPU **0.676861 → 0.666192 s**, **−1.58%**.
- C++ bbox-layout/wordmeta/rules, Ghostscript 477 pages: user **1.927913 → 1.948056 s**, **+1.04%**; total **1.970681 → 1.996842 s**, **+1.33%**.
- C++ TSV/wordmeta, same PDF: user **1.862484 → 1.777669 s**, **−4.55%**; total **1.911241 → 1.830373 s**, **−4.23%**.

These are observed sample medians, not established improvements. Sample ranges overlap and the host is shared. Five runs support the narrow conclusion that these slices show no material regression; they do not establish statistical equivalence or causality. The Rust aggregate includes 258 separate process startups and can hide a layout-specific slowdown. Only one PDF is timed for C++; flat bbox/no-metadata modes have correctness comparisons but no performance slice here. There is no peak-memory/allocation profile, per-document timing distribution, or combined C++→Rust throughput benchmark. None is required to invent a blocker for a near-neutral architectural refactor.

## Contract and quality evidence

The caller now requests `PageLayout::within` then serializes it (`lib.rs:285–286`). Membership replacement, region order, and title transfer occur within the private module before serialization; caller-owned table vectors and reset choreography are deleted. Interface-level tests cover overlapping regions, title ownership, wrapped cells, and discarding a partial layout on work exhaustion. This is meaningful depth rather than merely moving the same coordination back into the caller.

`TextPage::addRule` (`poppler/TextOutputDev.cc:2949–2959`) decrements the remaining document allowance before rejecting a full page, stops decrementing at zero, and marks incompleteness only on rejection. The 20,000 retained/page and 200,000 visible candidate attempts/document semantics are preserved, including rejected candidates on full pages. Moving the constant-time check does not justify a speed claim. Clip/visibility filtering still precedes acceptance. Parser/report changes carry producer truncation separately from layout fallback.

Inspected supplied evidence reports C++ 144/144, Rust 49 tests, 258/258 golden parity, 1,000 seeded differential matches, 91 successful PDFs across ten modes plus four matching failures per mode, and four real producer→converter completeness cases passing. These are existing results, not tests rerun by this reviewer. Byte comparisons are useful diagnostics and do not become an acceptance requirement.

The completed `quality.json` records **254 authored-reference documents**, no missing/unpaged cases, and identical component rows across arms. Aggregate quality is **0.7666**, tolerant-table score **0.9020** and tolerant-table-content F1 **0.9257** (226 table-scored cases) for both arms; each recorded difficulty-level quality also matches. The empty-output sentinel scores zero. Equal predictions are scored once per case by the deterministic frozen judge; changed predictions would be scored separately. Four real PDFs lack authored references and are explicitly excluded from quality scoring. These results support preservation on the scored corpus, not universally perfect extraction or new table-quality gains.

No arbitrary byte-parity demand, unmeasured speed claim, or speculative performance blocker is warranted. Retain the cautious performance claim and coverage disclosure; the optional findings can be dispositioned independently.
