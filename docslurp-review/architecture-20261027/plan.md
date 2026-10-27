# Acceptance and measurement contract

Baseline: merged `33301dfcee436ea7dea6159416feace8a65a5b54`. Candidate: `docslurp/deepen-extraction-modules`.

The user permits harmless Markdown byte differences to obtain substantially faster conversion, provided tolerant Markdown/table quality remains high. Byte comparisons are diagnostic, not the acceptance contract. Required facts, table ownership, numeric values, reading order, rule limits and truthful completeness reporting remain required.

Implement three recommendations: shared structured-text facts, page-layout ownership before serialization, and bounded rule evidence with completeness propagated to Rust. No new dependency or paid compute. Existing retained baselines and optimized release builds provide recoverable comparison arms.

Primary speed metric: median child user CPU seconds for five alternating baseline/candidate blocks, after warming each arm, on 258 documents (1,917 pages) and the 477-page Ghostscript document in bbox-layout/wordmeta/rules and TSV/wordmeta modes. Report total CPU secondarily. Timing includes process startup and conversion, excludes scoring and fixture preparation. No wall-time speed claims on this shared host. Both arms must exit successfully and have passed separate output checks.

A >=5% median user-CPU reduction is worth investigating as a speed gain; >5% regression on any slice triggers investigation. Near-neutral speed is acceptable for verified correctness and ownership improvements. Quality: existing frozen tolerant Markdown judge on 254 generator-authored cases from the 258-document corpus (four real PDFs have no authored reference); no aggregate or per-level decrease >0.005, no new missing/unpaged cases, and no observed table-content loss. Required targeted tests must pass. This run does not authorize lower factual correctness in exchange for speed.

Bounded experiment: initial three-slice timing and quality pass; at most one local membership representation experiment if evidence identifies a regression or avoids predictable dense-ID hashing overhead, then a fresh five-block confirmation. No broader parameter search or remote workload. Minimal faithful arm is the implemented deep layout module; dropping detection or title assignment would change the requested result and is excluded. Exact-byte compatibility machinery may be removed if a measured candidate meets semantic gates.

Evidence: baseline.json, corpus-results.json, fuzz-results.json, pipeline logs, performance.json, quality.json, worker test logs and council outputs in this directory. The local benchmark script preserves commands and raw per-block CPU samples. Machine/compiler identities are captured alongside results.
