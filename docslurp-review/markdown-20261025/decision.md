# Fast PDF parts and rich Markdown: decision review

Review date: 2026-10-25. Target: DocSlurp.io. Source checkout: Poppler `e658c5562` (26.10.90). Runtime experiments: mostly installed Poppler 26.10.0; the performance builder also measured a private Release build of checkout 26.10.90 (GCC 16.2.1), with dynamic linkage verified. Tables identify their provenance. This is a review with executable experiments, not a shipping exporter. Five independent Sonnet 5.5 builders supplied alternatives; the coordinator checked source, ran additional probes, challenged defects, and makes the recommendation below. User explicitly permits optional nonstandard HTML/SVG fragments.

During the review, independent work advanced the checkout to `4ca703c55`, adding `DOCSLURP.md` and `test/docslurp/` scaffolding. `git diff e658c5562 HEAD` showed no changes to the Poppler extraction sources being cited. This review did not make those commits or modify that work. New regression cases should use the new scaffold when implementation begins. The new notes recommend interleaved baseline/candidate measurements, median of at least five runs, and CPU time on this busy machine; future acceptance benchmarks should follow that guidance.

## Recommended direction

Build **one native PDF-parts collector with a separate layout and serialization layer**, exposed through one CLI. Keep the production fast path in process: no obligatory JSON serialization between collection, layout and writing. Markdown, rich Markdown, structured JSON, images and geometry consume the same page evidence. Make a versioned JSONL dump available for experimentation and downstream integrations.

Default to native semantics and geometry; escalate uncertain regions to a visual representation or optional recognition. Do not make page rasterization or model inference a prerequisite for clean digital text. Do not claim that an SVG picture is an editable table or that a crop has recovered an equation's meaning.

The bold move is to make the **PDF parts bundle the product contract**, with Markdown as one useful view. It lets DocSlurp improve reading order or table inference without reparsing the PDF, and it makes extraction decisions inspectable.

Proposed command (not implemented):

```sh
pdfparts input.pdf --out out/ --emit markdown,json,assets \
  --profile rich --tables auto --math auto --svg external \
  --render adaptive --jobs 4

pdfparts input.pdf --out out/ --emit markdown,json,assets \
  --profile rich --svg inline --html inline --render adaptive
```

`--jobs 4` is an example, not a measured optimum. A portable `gfm` profile remains available; an HTML-capable `rich` profile allows merged tables and styled spans. Inline SVG is an explicit richer-viewer choice. Asset layout, Markdown dialect, and recognition effort are independent options.

The bundle contains `document.md`, `document.json`, optional `pages/*.jsonl`, `assets/`, and a manifest. Record the source digest, engine/version, options, page transforms, element IDs, asset references, feature availability, unresolved regions and completeness. Saving all per-glyph evidence is optional; avoid turning maximum debug output into the default cost.

## What the experiments establish

| Observation | Evidence | Meaning and limit |
|---|---|---|
| Named-property MCID missing | Coordinator fixture emits one begin and two ends; selecting MCID 7 returns empty text | Fix callback delivery before trusting tags; system runtime and checkout source agree on this path |
| Nested MCID-less Span truncates selection | `Before inside after` becomes `Before inside ` | Preserve every nesting frame, not just numbered frames |
| Compact pattern becomes 31,304 fill callbacks | Coordinator expanded versus intercepted pattern fixture | Non-text capture can multiply work. Interception skipped semantics; it is not a quality-equivalent speedup |
| 41,527 / 42,111 words join a structure leaf | Architect's 80-page AAPL experiment | The join seam is useful; 98.6% is coverage, not structural accuracy, and its key is insufficient for general Form content |
| Mixed HTML preserves table spans | Fidelity prototype and rendered output | Tested on a synthetic table; ambiguous rule repairs are flagged, not assumed correct |
| Existing XML can lose cell boundaries | Fidelity's AAPL page 32 probe | Word-edge clustering finds three numeric columns, while end-to-end XML projection fails after runs merge columns |
| Small synthetic equation reconstructed | Fidelity prototype emits sum, limits, subscript, fraction and equation number | One authored case, uncalibrated heuristic; not evidence of broad math accuracy |
| Prototype serializers can regress fidelity | Literal emphasis inside HTML, heading false positives, superscript misordering | Test rendered Markdown, not only strings. Fidelity branch fixes its emphasis case; architect prototype retains failures |

See [coordinator probes](coordinator/README.md), [architect report](architect/report.md), and [fidelity report](fidelity/report.md). Raw timings across experiments vary substantially under concurrent host load. No experiment establishes the fastest complete exporter, and no complete proposed native pipeline was benchmarked. Ignore failed/terminated runs as performance evidence.

The [architecture challenge response](architect/response-round2.md) supersedes its first-round scope and fidelity promises. It reproduces page/Form MCID collisions, repeated Form placements, and named-property imbalance corrupting a following frame. The 98.6% association result must not be promoted to an accuracy claim. Frame balance is necessary but not sufficient: acceptance tests must check expected ownership, nesting and text, not only equal counts.

### Measured cost boundaries

The performance builder's checkout-linked measurements use aggregate child user+system CPU, including fork workers. Five valid runs give these medians:

| Mode | AAPL, 80 pages | US Code, 21 pages |
|---|---:|---:|
| Checkout `pdftotext -layout` | 0.339 s | 0.097 s |
| Experimental Markdown, one worker | 0.474 s | 0.146 s |

The Markdown prototype found no tables and has many false headings, with no image/math/shape/metadata support. It also interprets six sample pages before interpreting every page (86/27 total interpretations). These results measure cost structure, not equivalent quality or a complete exporter. Host load was 16–29; no wall-clock scaling claim is supported. Reported maximum RSS is the largest process, not combined worker memory.

On scanned pages 1–8, text-only callbacks cost 0.018 s CPU versus 2.353 s for image-aware callbacks. The pages contain 16 JPX images and eight JBIG2 soft masks. `Gfx::doImage` calls `getImageParams()` before the image callback; JPX initialization proceeds to `opj_decode`. Thus a metadata-only callback cannot avoid this cost. **Add a predecode image-information path**: investigate header-only JPX parameter discovery and dictionary-based resource indexing, with explicit treatment of inline images, Forms, patterns and masks. A resource dictionary lists candidates, not necessarily visible placements. No such patch was built or measured here.

After the user installed profiling tools, a coordinator `perf record -e cpu-clock:u -F 199 -g` run on checkout `build/utils/pdfimages -f 1 -l 8 -list` captured 434 samples with zero lost samples. Self samples included 24.88% in `JPXStream::init`, 22.12% in OpenJPEG `opj_dwt_decode_real` and 19.12% in `opj_tcd_decode_tile`. This independently supports the decode attribution for listing images. It is one sampled run, not a speedup measurement; frame-pointer call chains contain incomplete frames, so use self symbols rather than precise inclusive attribution. [Raw profile](coordinator/scan-perf.data), [text report](coordinator/scan-perf-report.txt).

See [performance report](performance/report.md) and its retained valid raw results. Reject a global text-first then graphics-second design for pages already known to need both: choose capabilities before the first pass where possible, and disclose conditional replays.

## Architecture and ownership

1. **Document index:** read Info/XMP, page geometry, outlines, annotations, structure tree, role maps and associated content once. Traverse structure nodes without calling `StructElement::getText()` per leaf: that function invokes `displayPages` (`poppler/StructElement.cc:819–869`).
2. **Page evidence collector:** capture glyph/run text, mapping provenance, style, quads/baselines, marked-content ancestry, paths/rules, images and placement transforms. Preserve evidence before `TextPage::coalesce` imposes a layout (`TextOutputDev.cc:5696`). Reuse existing word formation initially, with raw evidence retained for difficult scripts and math.
3. **Layout resolver:** generate competing hypotheses from valid tags, visible ruling geometry, word gutters, style and document context. Resolve reading order and content ownership before serialization. Keep the decision, supporting element IDs and warnings separate from recorded facts.
4. **Asset materializer:** distinguish original embedded bytes, display-ready images and composed visual regions. Materialize only demanded assets, retaining masks, clips, transforms, color/decode interpretation and paint dependencies where needed.
5. **Writers:** plain text, GFM, rich Markdown, HTML and structured JSON share one semantic representation. They own escaping and dialect-specific output, not table detection.

These can be separate C++ modules behind neutral structs in the same binary. A Poppler-free projector API is valuable; a mandatory Python process or serialized JSON boundary is not required. Use the Python experiments to develop algorithms, then profile and port the production hot path. No new PDF parser or universal display-list engine is needed for the first useful release.

The collector must be explicit about `OutputDev` capabilities. `needNonText`, tiling/shading callbacks, Type 3 handling, forms, coordinate direction, and image-stream consumption affect interpretation. Blindly forwarding callbacks to a text device and a renderer does not establish equivalent semantics.

### Single-pass promise

Promise **one native collection pass per selected page**. Metadata and annotation/tree traversal do not require another interpretation of the page content.

There are two honest options for visual fallbacks discovered after layout:

* **Adaptive default:** re-interpret only pages needing composed visuals, at most once per requested render configuration where practical, and service all their regions together. Log extra interpretations. Do not call this strict single-pass rendering.
* **Explicit record-and-replay mode:** retain a sufficiently complete page-local rendering representation during the initial pass, then replay regions. Measure font/image decode work and peak memory; do not confuse the lightweight evidence list with a replayable display list.

Do not delay the first exporter on proving a universally faster renderer recorder. A sparse-document fast path can win by doing less work; image-heavy pages may justify simultaneous collection and rendering. Measure both against the same output requirements.

## Feature preservation contract

| Feature | Native reconstruction | Rich representation | When uncertain |
|---|---|---|---|
| Ruled tables | Visible line/rectangle edges plus word assignment; tags for cell roles/spans | HTML `table`, `thead`, `th`, `rowspan`, `colspan`, editable cell content | Keep alternate topology or visual crop; flag inferred rule repair |
| Borderless tables | Persistent word-edge/gutter alignments, numeric columns, repeated row patterns, wrapped-cell continuity | HTML table; GFM only for simple rectangular tables | Avoid converting prose columns or lists into tables; preserve region and words |
| Headings | Valid structure roles, outline support, document-level typography and numbering | Markdown headings with stable anchors | Leave prose as prose; font size alone must not create headings |
| Headers/footers | Repetition and position across pages, Artifact tags | Separate furniture nodes, optionally shown | Retain source text; suppression must be reversible |
| Images | Source object plus every visible placement | Relative image asset or inline asset; figure/caption association | Render the composed region when raw pixels omit masks, clips or overlays |
| Shapes/charts | Path topology, fill/stroke, transform, clip and paint order | SVG fragments or referenced SVG | Raster region when SVG subset cannot reproduce appearance; no invented chart data |
| Math | Authored semantic content where present; limited glyph-layout grammar otherwise | LaTeX/MathML where verified; SVG for appearance | Uncertain formula keeps visual plus raw text/provenance; do not invent LaTeX |
| Links | Annotation actions/destinations and quads joined to spans | Markdown links or HTML anchors; internal anchors stable | Keep unresolved targets in metadata rather than guessing destinations |
| Formatting | Font descriptors/name evidence, baseline, rise, fill/stroke, size | Markdown emphasis/code; HTML strong/em/sup/sub and optional styles | Preserve source runs; distinguish superscript from footnotes |
| Metadata | Info, XMP, page labels/boxes/rotation, outlines, language, source and engine IDs | JSON canonical; optional front matter | Preserve raw versus normalized values and conflicts |
| Reading order | Tags checked against geometry; region and column ordering | Reflowable paragraphs/lists, local visual islands | Record ambiguous order; never globally y-sort a multi-column page |

Complex table content must use an HTML inline serializer. `**bold**` inside a raw HTML table is not a reliable emphasis mechanism. Use `<strong>`, `<em>`, `<sup>`, escaped text and links within those cells. Blank lines and Markdown delimiters need profile-aware handling. GFM's table grammar has a single header row and no block content or span syntax; see the [GFM specification](https://github.github.com/gfm/#tables-extension-).

Rich mode declares its required viewer features. SVG fragments need unique IDs and rewritten local references for definitions, masks, clips and gradients. Extracting a `viewBox` alone can retain unrelated full-page contents and bloat; generate region-scoped content with its dependencies. Avoid inherited font substitution for fidelity-sensitive glyphs: retain outlines or use a raster fallback where needed. Text in SVG paths remains visual, not editable semantic text; keep searchable text separately with ownership so it is not duplicated in prose.

## The key data-model changes

The important unit is a **placed content occurrence**, not an xref ID or an MCID alone.

* Join tags using document/page context, content-stream or Form scope, MCID and the invocation needed to disambiguate repeated placements. A Form can recur under different transforms; page/MCID is not a general unique key.

  For page content, use the page content namespace; do not assume each member of a `/Contents` array creates a new MCID namespace. For Forms retain the full object reference, including generation, and invocation ancestry. A structure reference to a Form may not identify a unique placement; represent that ambiguity instead of assigning a guessed instance.
* Keep glyph/source byte ranges and logical replacement ranges separately. Ligatures, combining marks and ActualText can map many glyphs to one string, or one glyph to several characters. Word-start lookup cannot assign every character's ownership safely.
* Normalize coordinates once, recording original page boxes, rotation, UserUnit and invertible transforms. Keep quads for rotated text/images; a bounding rectangle is inadequate for exact overlap.
* Table cells need row/column index, row/column span, header relations, text blocks, source IDs and an inference status. `string[][]` cannot carry merged semantics.
* Images need source resource identity and generation, dimensions, encoding/decode/mask identity, placement matrix, visible clip and asset handle. Deduplicate encoded bytes independently of display-ready appearances.
* Paths need their segments and graphics context, not just counts or bounding boxes. Simple rules can use a compact fast representation. Complex operations must be retained as unsupported visual dependencies, not silently discarded.
* Record `native`, `inferred`, `visual`, `recognition_required`, `unavailable` distinctly. A heuristic score is not a calibrated probability. Track semantic coverage, visual coverage, omissions and fallback cost separately.

Preservation status is independent of representation: **preserved, approximated, referenced-only, lost**. A missing asset with a bbox comment is referenced-only, not preserved. Report these outcomes per element and allow callers to reject incomplete bundles.

## Fast path priorities

1. Avoid multiple CLI parses for text, images, geometry and metadata. Satisfy a requested artifact set in one coordinated job.
2. Keep compact numeric evidence in memory; make JSON a requested output. Avoid per-glyph allocation, repeated font-name strings, formatted JSON on every callback, and quadratic joins. Use interned style/resource IDs and indexed ranges.
3. Walk the structure tree once and join once. Fix both marked-content defects before treating tags as an authoritative fast path.
4. Capture image metadata without pixel decoding when possible; extract original compressed data when it is display-compatible. Do not require high-compression PNG for every asset.
5. Handle patterns and shadings as compact operations or explicit fallback dependencies. The coordinator probe demonstrates why naive expansion can dominate a supposedly cheap geometry pass.
6. Reuse per-document font resolution only under correct document/resource/generation identity. Lazy font programs and caches need Unicode and geometry parity gates; previous review speed estimates are not carried forward as measurements.
7. Bound page-level work and memory. Prefer worker processes or separate document instances until sharing and cache safety are demonstrated; reuse workers to amortize startup and font work. Record actual throughput, peak RSS and slowest-page behavior.
8. Infer document typography and repeated furniture from collected summaries. Finalizing Markdown after seeing the document need not reparse the PDF. Streaming should state when page-level output is provisional.

## DocSlurp integration

Read-only inspection of adjacent DocSlurp checkout `5d4d21fc`:

* `worker/src/native-pdf/poppler-engine.ts:17` collapses whitespace in layout text and declares text/Markdown only. Replace that rich-output path with a bundle-backed adapter; retain plain-text fallback behavior as a separate capability.
* `worker/src/native-pdf/types.ts` already has optional `scan()` returning runs, image placements and glyph statistics from one pass. Extend it with versioned evidence/semantic output rather than making callers invoke separate methods that each interpret the page again.
* Existing table rows, image boxes and drawing counts are too narrow for the user's requirements. Add cell spans, placement transforms, structured paths, provenance and feature status while keeping engine-neutral interfaces.
* `worker/src/extractors/local-artifact-adapters.ts:103` has distinct Poppler text/image/HTML/metadata/embedded-image producers. One bundle producer should fulfill those demands and feed page routing, Markdown, chunking and citations from shared IDs.

Keep the neutral engine contract; no need for every consumer to depend on Poppler classes. Report extraction and fallback cost through existing DocSlurp telemetry. This review makes no DocSlurp edits.

## Table judging requirement

Follow the [table acceptance and scoring contract](table-acceptance.md), now implemented in the [table scorer](../../test/docslurp/table_scoring/README.md) and applied to [recent retained outputs](table-eval/report.md). Judge logical cells and header coverage after normalizing spacing and alignment. Accept repeated header labels and flattened merged titles, including a shared group title repeated in each child column. Require each source-provided header at least once per applicable column, either explicitly or through a clear spanning relationship. Score exact span topology separately; a semantically correct GFM flattening is acceptable. Missing values and incorrect column assignments remain errors.

## Delivery sequence and acceptance

**First vertical slice:** named/nested marked-content fixes, evidence collector, linked prose with headings/styles, simple and merged tables, manifest, GFM/rich writers, and complete asset references. Preserve visual fallbacks for unresolved regions. Validate the output in the actual DocSlurp viewer.

**Second slice:** image composition and scoped SVG, robust column/reading order, table repair confidence, cross-page tables/furniture, document typography. Add deterministic bounded resource handling and parallel jobs only with quality parity.

**Optional enrichment:** route unresolved scanned/table/math regions to local OCR or layout/math recognition. Preserve the original visual and provenance. Never force every digital page through these costs.

Before promoting any pipeline, benchmark the same requested outputs on a fixed, retained corpus. Include born-digital text, tagged forms, ruled/borderless/merged tables, multi-column prose, scanned/OCR layers, rotated pages, RTL/CJK, Type 3/outlined math, masks/transparency, reused Forms and patterns. Retain fixture source, expected structure and rendered references.

Measure wall and CPU time, time to first completed page, per-page p50/p95, total memory/output bytes, content interpretations, render/decode counts and fallback rates. Pair these with text retention and order, table cell adjacency/spans/header relationships, heading precision/recall, link target agreement, complete asset references, and region visual comparisons. Report both all-output and native-only modes; never compare a raw-text sink against a full rich bundle as if they delivered the same product.

For DocSlurp's scientific preservation eval, follow its existing designated development fixture and matched encodings before expanding the suite. No broad preservation eval was run here. Calibrate thresholds on development data and reserve holdouts; a handful of successful synthetic cases is a mechanism demonstration, not a quality percentage.

## External alternatives and renderer scope

The [PyMuPDF API documentation](https://pymupdf.readthedocs.io/en/latest/pymupdf4llm/api.html) now directs Markdown/JSON users toward Document methods; a future comparison should pin the installed version and actual entry point. The [Docling pipeline reference](https://docling-project.github.io/docling/reference/pipeline_options/) separates its native extraction path from layout/table-model pipelines. These are useful reference architectures and comparison candidates, not benchmark winners established by this review.

GFM parses raw HTML, but hosts may apply additional sanitization; the [GFM introduction](https://github.github.com/gfm/#what-is-github-flavored-markdown-) explicitly distinguishes those steps. No live GitHub/GitLab/Obsidian compatibility matrix was tested. Rich mode should declare capabilities and retain external PNG/SVG or portable output options instead of claiming universal rendering.
