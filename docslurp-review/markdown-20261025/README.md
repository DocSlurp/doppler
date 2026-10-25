# Markdown export council

[Visual review with actual source/output evidence](coordinator/final-review.html).

Start with the [coordinator decision review](decision.md). The five independent proposals are preserved separately below. All candidate code is experimental; no exporter has been integrated into Poppler or DocSlurp by this review.

| Council builder | Proposal and rationale | Main limitation or disagreement |
|---|---|---|
| [Architect](architect/report.md) | A versioned page-evidence producer, independent projector and asset writer. Makes new formats and layout iterations reuse the same extraction. | First probe has false headings, incomplete scope and broken rich emphasis. Read the [second-round challenge response](architect/response-round2.md), which corrects the contract. |
| [Ruthless Minimalist](minimalist/report.md) | A small `TextOutputDev` subclass and tree walker, assets and tags added in tiers. Reuses mature word/flow APIs and can ship incrementally. | Default placeholders do not satisfy the full preservation goal; cost and implementation-size projections remain estimates. Its XML experiment fails on real columns/tables. |
| [Performance Wizard](performance/report.md) | Native in-process projection, bounded page workers and explicit cost controls. Measures the overhead of Markdown and non-text collection. | A fast prototype is not equivalent to a complete rich exporter; typography sampling, image decoding and worker memory need explicit accounting. |
| [Genius Inventor / Fidelity](fidelity/report.md) | Combine tags, ruling topology and word alignment; use hybrid HTML tables, selective math inference and SVG figures. Preserves what ordinary Markdown cannot represent. | Real borderless XML conversion fails; synthetic repairs are guesses; SVG subset and math coverage are narrow. |
| [Clean-slate Radical](radical/report.md) | Region-adaptive export built around reusable events and a parts bundle, with rich fragments instead of forcing every region into prose. | Event recording alone does not provide faithful replay; text coordinates, stream handling and output cost need verification. |

The coordinator recommends combining the architect's ownership boundaries with the minimalist's existing `TextOutputDev` seam and the fidelity proposal's rich serializers, while keeping the performance costs explicit. This is a stated synthesis; it does not replace or conceal the competing reports.

## Implemented table scorer

[Scorer and CLI](../../test/docslurp/table_scoring/README.md) · [Scored recent outputs](table-eval/report.md) · [JSON results](table-eval/results.json). Thirty-seven regression tests pass. The twenty-six retained-output comparisons include explicitly paired interpretations of the ambiguous ruled fixture.

## Table judging

[User-specified table acceptance contract](table-acceptance.md): tolerate whitespace/alignment and repeated headers; require source header coverage for every applicable column.

## Evidence and reproductions

* [Coordinator callback probes](coordinator/README.md): named-property and nested-content failures; pattern callback expansion. Includes retained original PDF generators and raw results.
* [Scoped Form/ActualText probe](architect/round2/results-scope.txt): demonstrates why page/MCID alone is insufficient.
* [Table export](fidelity/results/ruled_merged.proto.md) and [rendered table HTML](fidelity/results/ruled_merged.rendered.html): actual mixed output, including flagged repairs.
* [Math output](fidelity/results/heading_math.proto.md): one synthetic equation, explicitly uncalibrated.
* [SVG diagram](fidelity/results/shapes_figs/fig1.svg) and [SVG chart](fidelity/results/shapes_figs/fig2.svg): simple editable geometry with font-substitution limitations.
* [Performance results](performance/results/): raw runs and summaries; invalid initial runs are retained under `superseded/` and excluded.

Scope: five Sonnet 5.5 council builders, at most five running concurrently, plus one short follow-up architecture challenge after its first builder finished. One gpt-6.1-sol reporter prepared checkpoints. No model-cost telemetry was exposed; no external recognition APIs or remote compute were launched. Review files were isolated by candidate directories rather than competing edits to production code. Separate Git worktrees were unnecessary for these standalone experiments.

The checkout changed independently while the review ran; see the provenance note in `decision.md`. Early probes use installed Poppler 26.10.0. The performance report separately identifies later checkout-linked runs. Treat each experiment's own provenance as authoritative rather than assuming all results use one library build.
