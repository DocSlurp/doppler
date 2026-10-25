# Fast Markdown export for Poppler/DocSlurp: performance-first builder proposal

Persona: Performance Wizard (builder owning the whole solution). Date 2026-10-25. All numbers below come from completed runs stored under `results/`. **There is no universal "fastest" claim**: the machine was shared (1-min load average 16–29 on 8 logical CPUs), so only CPU time is compared and no worker-scaling conclusion is drawn.

## 0. Provenance and measurement rules (read first)

| Item | Value |
|---|---|
| Checkout commit at start of my work | `e658c5562` (HEAD later advanced to `4ca703c55` by a docs/test scaffold merge; `git diff --stat e658c5562 HEAD -- poppler goo utils fofi splash` is empty, so library/utility sources are identical) |
| "system" binaries | Arch package `poppler 26.10.0-1` (`pdftotext version 26.10.0`) from `/usr/bin` |
| "checkout build" | my private Release build (`-DCMAKE_BUILD_TYPE=Release`, GCC 16.2.1, libopenjpeg ON, DCT=libjpeg) of the checkout's sources, version string `26.10.90`, in a scratch dir under `/home/dan/tmp/agents/`. Built 12:54–12:58 local time from the `e658c5562` tree; reproduce with `bench/build-proto.sh`. The checkout's own `build/` dir contains no executables and was not used. |
| Prototype linkage | `pdf2md_proto.cc` compiled with `g++ -std=c++20 -O2` and dynamically linked (`ldd`) to the scratch `libpoppler.so.165` of the checkout build, public headers only. It is **not** linked to the system libpoppler. |
| CPU metric | `user+sys` from `getrusage(RUSAGE_CHILDREN)` deltas around each run (`bench/bench2.py`), median of N≥3 runs (N=5 unless noted). It includes every child the measured process waited for, so forked workers (`-j`) are **summed** — it is aggregate CPU, not per-worker. Wall time is recorded but is load-dominated and not used for conclusions. |
| RSS metric | `ru_maxrss` of `RUSAGE_CHILDREN` = the **largest single process** (not a sum across workers). Aggregate worker memory was not measured. |
| Failure handling | Every run is stored raw in JSONL with `rc`; `summarize.py` excludes `rc!=0` from statistics and prints `FAILED`. An earlier first attempt used `pdftoppm … /dev/null` (rc=1) and a killed `pdftohtml -xml` run; that file is kept only as `results/superseded/baseline-v1-INVALID.tsv` and is **not** evidence. |
| Inputs | Private DocSlurp samples in `~/.cache/docslurp/pricing-samples-2026-10-21/` read in place, not copied or uploaded: Apple 10-K (80 pp, tagged, 1.9 MB), US Code title 1 (21 pp, Ghostscript 8.14), scanned inaugural (26 pp, JPX background + JBIG2 mask, 2.3 MB). No synthetic fixtures were created by me; the coordinator's `make_fixtures.py` covers marked content and patterns. |

Raw files: `results/baseline-system-26.10.0.{jsonl,summary.tsv}`, `results/proto-vs-checkout-pdftotext.{jsonl,summary.tsv}`, `results/image-placement-cost.{jsonl,summary.tsv}`, `results/scanned-image-cost-single-run.tsv` (single run, `user=` only, valid rc=0), `results/output-compression-aapl.tsv`, `results/samples/*.md` (prototype output excerpts including the failures).

## 1. Findings (measured or read from source)

### F1. Plain text is cheap; every richer output has a price that depends mostly on what it asks the interpreter to do
System poppler 26.10.0, CPU s (median of 5; `results/baseline-system-26.10.0.summary.tsv`):

| tool | US Code 21 pp | 10-K 80 pp | scanned 26 pp |
|---|---|---|---|
| `pdftotext` (reading) | 0.102 | 0.347 | 0.043 |
| `pdftotext -layout` | 0.097 | 0.365 | 0.049 |
| `pdftotext -bbox-layout` (5.2 MB XML on 10-K) | 0.191 | 0.522 | 0.063 |
| `pdftohtml -xml -i` | 0.069 | 0.736 | **7.372** (3 runs) |
| `pdfimages -list` | 0.055 | 0.272 | **7.450** (3 runs) |
| `pdftoppm -r 72 -gray` (to scratch) | 0.365 | 0.802 | **11.697** (3 runs) |

Floor: roughly 4 ms/page for 10-K text extraction including coalescing. The scanned PDF costs 0.05 s as text-only but 7–12 s as soon as any image-aware tool touches it (F2).

### F2. Image *placement* is the single largest avoidable cost, and it is caused by JPX full decode in `Gfx::doImage`
Probe (`proto --mode imgs`, a `TextOutputDev` subclass whose `needNonText()` returns true and whose `drawImage*` only count calls; checkout build 26.10.90), scanned PDF **pages 1–8** (`-f 1 -l 8`), 3 runs:

* `--mode null` (text only): **0.018 s** CPU
* `--mode imgs`: **2.353 s** CPU (maxrss 84 MB vs 17 MB), 16 image callbacks. `pdfimages -list -f 1 -l 8` shows those 8 pages hold 16 JPX images (one 683×945 and one 2049×2833-ish per page) plus 8 JBIG2 soft masks.
* Ratio ≈130× for 8 pages. Over all 26 pages, `pdfimages -list` is 6.5–7.5 s, consistent.

Cause (source): `Gfx::doImage` calls `str->getImageParams(&bits,&csMode,&hasAlpha)` at `poppler/Gfx.cc:4223`, **before** it reads Width/Height and **before** `out->drawImage` is called. For JPX, `JPXStream::getImageParams` (`poppler/JPEG2000Stream.cc:161-165`) calls `init()`, which runs `opj_decode` + `opj_end_decompress` (`JPEG2000Stream.cc:392`). So the whole image is decoded merely to learn the component count. Only `needNonText()` gates this: image `Do` is skipped entirely when it is false (`Gfx.cc:4155`).

Consequence for architecture: **mixed capabilities cannot avoid the decode.** A device cannot ask for "placement without pixels" through the current `OutputDev` interface; the decode happens in Gfx before the device sees the call. Options:
1. *Gfx/JPX change (needs an experiment I did not run):* make `JPXStream::getImageParams` use `opj_read_header` only (component count and colour space are available after the header) and defer `opj_decode` to first `getChar`. Expected effect: placement cost drops from decode-bound to header-bound. **Speculation until built and measured**; `pdfimages -list` would benefit equally.
2. *No Gfx change:* run text with `needNonText=false` (fast, no images at all), then obtain image evidence from a separate cheap path: page `/Resources /XObject` dictionaries give width/height/filter/colour space without decoding; placement geometry would need a CTM-aware content scan. Not built; only the dictionary half is cheap and certain.
3. *Accept the decode only when needed:* decode JPX only for pages the router marks scanned/image-heavy, which are exactly the pages needing OCR or image export anyway.

### F3. `pdftohtml -xml` image extraction is the wrong template for Markdown assets
`results/scanned-image-cost-single-run.tsv` (one run each, valid, rc=0, pages 1–3 of the scanned PDF): `pdftohtml -xml` with image extraction **37.8 s user CPU**; with `-i` 0.81 s; `pdftotext -layout` 0.010 s. Source: `HtmlOutputDev::drawImage` (`utils/HtmlOutputDev.cc:1414-1435`) pulls every pixel through `ImageStream` into `drawPngImage` and PNG encodes at `Z_BEST_COMPRESSION` (`goo/PNGWriter.cc:118`). Only DCT images take the raw-copy path (`drawJpegImage`, :1228-1263, `-dumpjpeg`). Lesson: an export path must copy source bytes for DCT/JPX/JBIG2/CCITT where a browser or viewer can use them, and defer any re-encode.

### F4. A Markdown projection over the existing text tree costs roughly +10–40% CPU over `pdftotext -layout` in the prototype
Checkout build 26.10.90, CPU s median of 5 (`results/proto-vs-checkout-pdftotext.summary.tsv`):

| run | 10-K (80 pp) | US Code (21 pp) |
|---|---|---|
| `pdftotext -layout` (checkout build) | 0.339 | 0.097 |
| `pdftotext -bbox-layout` | 0.516 | 0.181 |
| proto `null` (interpret+coalesce+walk tree) | 0.420 | 0.125 |
| proto `text` | 0.439 | 0.116 |
| proto `md`, tables off | 0.478 | 0.148 |
| proto `md`, tables on | 0.474 | 0.146 |
| proto `md -j4` | 0.509 | 0.169 |
| proto `md -j8` | 0.558 | 0.186 |

Reading: the tree walk with per-word `std::string`/`WordView` allocation is where most overhead sits (null ≈ +25% over `-layout`); heading/bold/table logic adds only ~0.04 s on top. `-j` adds CPU (aggregate 0.47→0.56 at j8: duplicate `PDFDoc` opens, duplicate font loads, plus the sampling pass in the parent). Because of load 16–20 during these runs, I **make no wall-clock or scaling claim**; `-j8` output is byte-identical to `-j1` on the 10-K, which is the only property verified for parallelism.

### F5. Prototype pass accounting (corrects an overstatement in the design sketch)
The prototype is **not** strict one-interpretation-per-page in `md` mode. `sampleStats` interprets `min(n,6)` evenly spaced pages to find the body font size and a heading-size ladder, then emission interprets all n pages again:

* `md -j1`: `6 + n` page interpretations (10-K: 86 = +7.5%; US Code: 27 = +29%; for tiny documents ≤6 pages: 2× text).
* `md -jK` (K≥2, requires `n ≥ 2K`): parent samples 6 pages once, then K workers interpret n pages in total: still `6 + n` interpretations, but `1 + K` `PDFDoc` opens/xref parses (sampling pass + one per worker, each re-fetching fonts). Fewer than `2K` pages falls back to j1.
* `null` and `text` modes do **no** sampling, so they are lower bounds for projection cost, not apples-to-apples with `md`. All `md` rows above **include** the sampling pass in their CPU.
* A production design should remove the extra interpretations: sample from the pages already needed (first K emitted pages in order, then freeze the ladder), or buffer line records for the whole document (memory small, see below) and classify at the end. I did not implement or measure either.

### F6. Output size and compression
10-K: prototype Markdown 284 KB (gzip -6 72 KB, zstd -3 73 KB, zstd -19 61 KB), vs `-bbox-layout` XML 5.2 MB (gzip 723 KB, zstd -19 484 KB). Geometry-rich evidence is ~18× larger than Markdown before compression, so it should be a **separate, optional, compressed sidecar**, not part of the default path. `results/output-compression-aapl.tsv`.

### F7. Other measured/read hazards relevant to fast export
* Coordinator probe `pattern-grid.pdf`: a tiling pattern with needNonText on expands to 31,304 fill callbacks (coordinator figure; not re-measured here, noisy). `Gfx.cc:1937` skips patterns entirely when `needNonText` is false.
* Struct-tree text via `StructElement.cc:819-869` re-interprets the page per MCID (prior review: 2,320 interpretations / 10.26 s on a 20-page tagged doc; not independently re-run by me). Use one pass and join by MCID.
* Prior review claims I did **not** re-verify: 17× font-cache estimate, 31.5 s Form DAG blowup, 626-byte PDF costing 2.8 s. They are plausible from source but are unmeasured here. The claims I did verify: JPX full decode (F2), `Z_BEST_COMPRESSION` PNG (F3), `pdfimages -list` 7 s on the scanned file (F1).

## 2. Prototype quality failures (actual, observed)

`proto/pdf2md_proto.cc` is a performance probe. It must **not** be treated as a fidelity result:

1. **Tables: zero detected on the 10-K** (`grep -c '^|'` = 0) and zero on the US Code. On 10-K page 26 the financial table is split by poppler's block segmentation into one block per cell, so my "≥3 consecutive multi-cell lines in one block" rule never fires. Output is `#### **2025**`, `iPhone`, `$`, `209,586` … as separate paragraphs (`results/samples/aapl-p26-table-failure.md`). `pdftotext -layout` renders the same table correctly. Real table support must work from word geometry across blocks (or from a ruling/cell model), not within `TextBlock`.
2. **Heading false positives:** 323 of 327 headings are level 4; bold lead-ins such as "Item 1B." and bold-italic lines (`#### ***The price …***`) become headings; level ladder from a 6-page sample is crude.
3. **Page furniture kept:** running footers ("Apple Inc. | 2025 Form 10-K | 17") are emitted as body text. Tagged `/Artifact` is not consulted (and poppler's `TextOutputDev` never sees it; prior review + `Gfx.cc:5166-5191`).
4. **Small caps:** US Code headings come out as `R EFERENCES IN P UB . L. 116–260` (word splitting on size change) — no repair.
5. **Links:** the link code path (`enableHTMLExtras` + `TextWord::getLink`) produced **0** links on the 10-K; unverified whether the document has none or the path failed to attach them. Treat link support as unproven.
6. **No images, math, shapes, forms, annotations, metadata, outline, footnotes, multi-column verification, RTL/vertical text.**
7. Escaping is over-aggressive (`\|`, `\[`) and hyphen joining is naive.
8. No timing/memory caps, no partial-page signalling, no provenance.

## 3. Proposed solution

Aligned with the coordinator's recommendation: **a native collector plus a separate in-process projector, adaptive per-page rendering, and an optional rich HTML/SVG profile.** The performance persona adds the ordering and quality gates.

### 3.1 CLI

```
pdftomd [options] FILE.pdf [OUT]            # OUT '-' = stdout; default stdout
  -f N -l N                    page range
  --profile plain|gfm|rich     (default gfm)
  --assets none|external|inline-svg|inline-data   (default external for rich, none otherwise)
  --asset-dir DIR              external assets (images, SVG), content-hash names, deduped by object id
  --tables auto|html|gfm|off   (default auto)
  --pages-marker comment|rule|none
  --evidence FILE.jsonl[.zst]  optional page evidence sidecar (words, runs, boxes, provenance)
  --report FILE.json           per-page quality/fallback report (always cheap, default on stderr summary)
  -j N                         page-range workers (default min(cores, 4), only above a page threshold)
  --budget-ms-per-page N       work budget; exceeded => page marked partial, never silent
  --render-fallback auto|never|always   raster crop (PNG/WebP) of regions the projector cannot represent
  --jpx-decode auto|never|always        see F2
```
Streaming: Markdown pages are written in order as they finish; `--evidence` is written per page too. Exit code 0 even when pages degrade; per-page status is in `--report`.

### 3.2 Architecture and fastest path

1. **Router (per document, <10 ms):** `PDFDoc` open, metadata, outline, page count, tagged flag, per-page `/Resources` scan (XObject dictionaries: image count, filters, sizes; Pattern/Shading/Font types; annotations count). All dictionary-level, no content interpretation. Produces page class: `text`, `text+vector`, `image-dominant/scanned`, `heavy` (patterns, shadings, huge form fan-out).
2. **Collector (one interpretation per page, text-only profile by default):** `TextOutputDev`-style collection keeping *source runs* (before irreversible `coalesce`, `TextOutputDev.cc:5696`, per coordinator) with font/size/flags/colour/link/MCID when available. Text-only profile keeps `needNonText=false`, hence no JPX decode, no patterns, no shadings (F2, F7).
3. **Projector (separate, in-process, pure function of collector output):** reading order, paragraphs, headings (document-level ladder), emphasis, lists, tables, link spans, furniture suppression. Because it is a pure function of stored evidence, it can run on a different thread and be re-run without re-interpreting the PDF.
4. **Adaptive escalation per page** (only when the router or projector asks): 
   * `vector` escalation: rerun the page with a path/rule collector to get table rulings and shape bboxes (this is the *only* second pass, only for pages flagged table/figure-candidate by projector confidence).
   * `image` escalation: placements for pages the router marked image-bearing; **only after** the F2 fix or when the page is going to OCR/raster anyway.
   * `raster` escalation: crop-render (Splash) of low-confidence regions/pages.
5. **Parallelism:** page-range workers, each with its own `PDFDoc` (poppler is not safe for concurrent use of a shared `PDFDoc`; prior review §10 and my own use of fork). The router's page classes allow cost-balanced chunking (scanned pages are 100–1000× costlier than text pages, F1/F2, so equal page counts are a bad split). Document-level state (heading ladder, running-header detection) is computed from collector output without a second interpretation: collect all page line records first (memory is small relative to PDF), classify globally, then emit; or stream with a frozen ladder after K pages. Choose the buffered form by default; streaming is an option for very large documents.
6. **Output:** buffered per-page strings, ordered writer, optional zstd for sidecar evidence; Markdown itself is small (F6).

### 3.3 Rich profile: optional HTML and SVG fragments

(Per steering: allow non-standard export that mixes HTML/SVG in Markdown.)

The profile is a ladder; each rung is selected per region, never per document, and every use is recorded in the report with a reason.

| Rung | Output | Semantics | Visual fidelity | Renderer compatibility | Editing |
|---|---|---|---|---|---|
| R0 | CommonMark paragraph/heading/emphasis/links | full | none beyond text | universal | best |
| R1 | GFM pipe table | rows/cols, no spans, single-line cells | low | GitHub-style renderers; **no colspan/rowspan, no multi-line cells** | good |
| R2 | embedded HTML `<table>` with `colspan`/`rowspan`, `<thead>`, `<th scope>` | spans and header association | medium (borders/alignment via a small class set) | needs HTML passthrough; many sanitizers strip `style`/`class` but keep table tags | good (HTML-edit) |
| R3 | inline `<svg>` fragment (shapes, rules, vector figures, text-as-text where possible) | none or `<title>/<desc>` only | high for vector content | many Markdown renderers and all sanitizers disallow or strip inline SVG; GitHub strips it | poor |
| R4 | `![alt](assets/p3-f2.svg)` external SVG / PNG / WebP file | alt text only | high / exact | widely supported by image links | asset is opaque |
| R5 | raster crop of region (+ extracted text kept as hidden/adjacent text) | text next to it | exact | universal | none |

Rules:
* **Semantic first, fidelity second:** if a region is representable at R0–R2 with confidence ≥ threshold, never emit R3+. Merged-cell tables (R2) preserve *editable semantics*; SVG/raster preserve *appearance*. These are different promises and the report marks which was delivered per region (`kind: semantic|visual|both`).
* **External vs inline:** default external assets (R4/R5) because inline SVG breaks in sanitizing renderers, bloats the text that DocSlurp indexes and embeds, and inflates token counts; inline SVG (R3) only with `--assets inline-svg` for self-contained single-file output. Inline data URIs are available but off by default (size, no caching).
* **Compatibility profiles:** `gfm` guarantees only R0/R1 (+ R4 links); `rich` adds R2/R3/R5; a consumer that cannot render HTML gets the `plain` profile or the same page via `--profile gfm` from the stored evidence (projector re-run is cheap, F5).
* **Text duplication rule:** whenever a visual rung replaces a text rung, the extracted text for that region is kept (alt text, `<desc>`, or an adjacent collapsed block) so search/embedding does not lose it. Marked in the report as `text_preserved: true`.
* **Faithful fallback limits:** SVG built from the content stream is *not* guaranteed to match the rendered page: clips, soft masks, blend modes, patterns, shadings, and Type 3 fonts need Splash/Cairo (`pdftocairo -svg`) to be exact; any region containing them goes to R5 raster, or R4 cairo-SVG, never a hand-built R3. Text-in-SVG is only emitted with real font data or an explicit `font-family` fallback and is marked approximate.
* **Cost:** R2 is nearly free once a table model exists. R3 requires a second (vector) pass per flagged page; R4/R5 add a raster pass (system `pdftoppm -r 72 -gray` costs 0.8 s CPU on the 10-K, 11.7 s on the scanned file at that resolution — the latter is an image decode cost, F2) and should be lazy and cache-keyed by object id/content hash.

### 3.4 Fidelity and fallback contract

Per page, the report states for each feature one of `ok`, `approximate`, `fallback(rung)`, `unsupported`, `skipped(budget)`:

| feature | primary path | gate before accepting | fallback |
|---|---|---|---|
| text | collector with Unicode provenance | fraction of glyphs unmapped/identity-guessed/PUA below threshold | flag page `ocr_candidate`; do not guess |
| headings | doc-level size ladder + tagged `/H*` when present | ladder stable across pages; bold-only needs short single line | demote to bold paragraph |
| tables | word-geometry columns ± rulings; tagged `/Table` when present | row/column count consistent, cell text conserved (token multiset equal to source) | R2 HTML, then R5 raster crop |
| images | placement + raw stream copy | F2 constraint satisfied | placeholder with dimensions |
| math | tagged MathML/ActualText if present; else font-name/glyph heuristic → `$…$` only when ActualText/ToUnicode gives TeX-like text | unverified in this work | R5 crop; never fabricated LaTeX |
| shapes | rulings → table/rule; rest R4 | clip/mask-free | cairo SVG or raster |
| links | `/Link` annots with QuadPoints → word spans | URI scheme allow-list | plain text |
| metadata | Info + XMP + outline + page labels in front matter | none | omit |
| formatting | bold/italic/mono from font flags + name | consistent within word | drop emphasis |

**Content-conservation gate (cheap, always on):** the multiset of word tokens in Markdown (after escaping removal) must equal the collector's visible-text tokens minus declared furniture; mismatch marks the page `degraded`. This gate would have caught the table failure only partially (tokens were preserved, structure lost), so a *structure* gate is also required: any numeric-dense cluster of ≥3 aligned rows that is not emitted as table/HTML/raster is flagged.

### 3.5 Cost of quality (what each layer should cost; estimates labelled)

* Measured: text extraction ≈ 4 ms/page; projection over the tree ≈ +10–25% on top in the prototype (allocation-bound; a struct-of-arrays arena would cut this — **speculation**).
* Measured: image-aware interpretation of JPX pages ≈ 0.3 s/page (2.35 s / 8 pages) versus 0.002 s/page text-only. This dominates every other cost on scanned or image-heavy PDFs.
* Not measured: vector pass, table model, raster crops, SVG generation, structure-tree join.
* Rule: never pay image/vector/raster cost on a page unless the router or a projector confidence gate asks for it.

## 4. Experiments performed / needed

Performed (all valid, results in `results/`): F1 baseline (system), F2 image-placement probe, F3 single-run HTML image cost, F4 prototype vs checkout pdftotext, F6 compression.

Needed (not run; need tracked-code changes or more time):
1. **JPX header-only `getImageParams`** (patch `poppler/JPEG2000Stream.cc`, build in scratch copy, rerun `--mode imgs` on pages 1–8 and `pdfimages -list`). Pass criterion: callbacks identical (16), CPU within ~2× of text-only, extracted pixels byte-identical when a later `getChar` occurs.
2. **Single-pass projector with whole-document buffering** (remove F5 sampling) and arena allocation; compare against 0.339/0.097 s.
3. **Cross-block table recovery** using word geometry on 10-K pages 26 and a US Code statute table; gate = token conservation + row/column consistency, check against `pdftotext -layout`.
4. **Cost-balanced worker split** on a clean machine; measure scaling with idle CPUs.
5. **Link attachment audit** (why zero links) on a document known to contain URI annotations.
6. **Rich-profile spike:** tagged `/Table` → HTML with spans; `pdftocairo -svg` region extraction cost per figure.

## 5. Limitations

* Shared, heavily loaded machine; CPU-time only; no wall or scaling claims; three PDFs only; no CJK/RTL/math/forms documents.
* Prototype quality is poor by design (§2) and only demonstrates cost structure.
* "System" (26.10.0) vs "checkout build" (26.10.90, identical library sources to `e658c5562`) numbers are not interchangeable and are labelled in every table.
* Maxrss is per-process maximum, not aggregate worker memory.
* Prior-review claims not re-verified are listed in F7 and are not relied upon.

## 6. Approaches rejected (and why)

1. **Extend `pdftohtml -xml` / `HtmlOutputDev` into Markdown:** its image path decodes and PNG-encodes everything at maximum compression (F3, 38 s on 3 scanned pages); single-column-per-line positioned output discards the block/flow structure that Markdown needs. Reuse its link/outline handling only.
2. **Pipe `pdftotext -layout` through a regex Markdown converter:** fastest to build (≈ plain-text speed) but loses font evidence entirely; cannot do headings, emphasis or tables reliably. Keep as the `plain` fallback; this is what DocSlurp's `layoutTextToMarkdown` already does (coordinator note).
3. **Always render everything (`pdftocairo`/Splash) then OCR/vision-model it:** 11.7 s CPU for the 26-page scan just to rasterize at 72 dpi; wrong default for born-digital PDFs.
4. **One interpretation fanning out to all consumers via a tee device:** attractive but the interpretation profile (`needNonText`, patterns, JPX decode) is a property of Gfx, so a tee must take the *union* of needs and every consumer pays (F2, coordinator note on `OutputDev.h:88-110`). Escalate per page instead.
5. **Parallelism via threads in one `PDFDoc`:** not safe today (shared XRef/GlobalParams state per prior review); use processes or independent `PDFDoc`s.
6. **Inline SVG as default for figures/shapes:** rejected for compatibility (sanitizers), indexing noise and size; opt-in only.
7. **Pure-vision-model extraction as the main path:** out of scope for this performance review and costly; used only as a downstream consumer of region crops.

## 7. Files

* `report.md` — this file
* `proto/pdf2md_proto.cc` — probe source (modes `null|text|md|imgs`, `-j`)
* `bench/bench2.py`, `summarize.py`, `run-baseline.sh`, `run-proto.sh`, `build-proto.sh` — harness and reproduction (`bench/bench.py` is the superseded v1 harness)
* `results/` — raw JSONL, summaries, samples; `results/superseded/` — invalid first attempt kept for audit only
