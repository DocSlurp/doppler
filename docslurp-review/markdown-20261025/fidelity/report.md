# Fidelity-first Markdown export for DocSlurp — Genius Inventor / layout-fidelity proposal

Date 2026-10-25 · Poppler checkout `e658c5562` (master) · all measurements use **system** poppler 26.10.0 binaries (`/usr/bin/pdftotext|pdftohtml|pdftocairo|pdfimages`); the checkout's `build/` has no built utilities, so **no checkout build was used or modified**. Nothing outside `docslurp-review/markdown-20261025/fidelity/` was written (scratch: `/home/dan/tmp/agents/mdverify.*`, venv with markdown-it-py 4.2.0, removed at end).

Labels used throughout: **[F] finding** = observed/verified in source or by experiment here. **[S] speculation** = not verified. **[P] prototype defect** = known failure of my Python probe, not of the proposal.

---------------------------------------------------------------------------

## 0. Verdict in six lines

1. Poppler's text path already holds ~80% of what a fidelity exporter needs (word boxes, font size/bold/italic, spaceAfter, underlines, link quads, a weak `tableId`). What it **throws away** is vector geometry: rulings, fills, image CTM/quads. Tables, shapes, and figure regions cannot be done well from text alone. [F]
2. Fastest correct design: one **in-process `MarkdownOutputDev`** (C++) that records *evidence* during a single `Gfx` interpretation — words (not HtmlOutputDev runs), axis-aligned rule segments + thin filled rects, filled/stroked rect-ish fills, image placements, link quads, MCID/struct tags — then a pure post-pass builds blocks/tables/math/figures. No raster, no second parse. [proposal, built on F]
3. Tables: **ruling-graph → span grid** for ruled, **word-edge alignment** for borderless, **text-cluster repair** for rule-deficient rows. Prototype recovers rowspan/colspan on a hand-built ruled table and 3 right-aligned numeric columns on a real Apple 10-K page. [F, small sample]
4. Output has **two profiles**: `portable` (CommonMark/GFM only) and `rich` (adds HTML tables with spans, inline SVG, `<sup>/<sub>`, `$`/`$$` math). The user's requested non-standard mixing is first-class in `rich`, with an explicit per-fragment fidelity record (§6).
5. Every non-trivial fragment carries a **confidence + fallback**: visual crop (PNG, optionally paired with editable SVG/HTML). Math confidence is a heuristic score, **uncalibrated**, backed by one synthetic equation only.
6. Cost: ruling/figure evidence via `pdftocairo -svg` measured **~0.20 s/page** vs ~0.01 s/page for text; in-process path recording should cost a small fraction of that. [F measured; in-process figure is S]

---------------------------------------------------------------------------

## 1. Proposed CLI

```
pdftomd [options] file.pdf [out.md]      # new util; or `pdftotext -md` if upstream prefers
  -f/-l N                      page range
  --profile portable|rich      portable = CommonMark+GFM tables only (default); rich = HTML/SVG/math fragments
  --tables  gfm|html|auto|crop auto: GFM when no spans, HTML when spans/nonrect, crop when confidence < τ
  --figures none|crop|svg|both crop = PNG sidecar; svg = inline/sidecar editable vector; both = img + <details>svg
  --assets  dir|inline|none    external files (default dir=<out>_assets/) vs data: URIs vs drop
  --math    off|unicode|latex|crop+latex   (default crop+latex under rich, unicode under portable)
  --confidence-comments        emit <!-- fidelity: kind conf=… basis=… fallback=… --> per fragment
  --report report.json         machine-readable provenance: per block bbox, source runs, tags, flags, timings
  --ocr-hint                   flag pages whose text provenance is untrustworthy (see §4) rather than OCRing
  --jobs N                     page-range parallelism, one PDFDoc per worker
```
`--report` JSON is the DocSlurp contract (coordinator notes `NativePdfTable` today only has bbox + string rows; it needs cells with spans, header association, source runs, confidence, fallback refs — `docslurp-review/markdown-20261025/coordinator/README.md`). Markdown is a **projection** of that evidence, so the same pass can feed chunking and Markdown.

---------------------------------------------------------------------------

## 2. Architecture

```
Gfx (single interpretation, needNonText=true only for vector/image *recording*, no raster)
 └─ EvidenceOutputDev : OutputDev
      words   <- TextPage words (NOT HtmlOutputDev runs; see §5.3)           [reuse]
      rules   <- stroke()/fill()/eoFill(): axis-aligned 2-pt subpaths, thin rects, rect fills
      fills   <- rect-ish fills + colour (zebra bands, header shading, white-out occluders)
      images  <- drawImage*/drawMask*: CTM quad, ref id, size, bpc, SMask presence
      links   <- AnnotLink quads (per word), outline/dest   [exists]
      marks   <- begin/endMarkedContent: MCID, Artifact, ActualText, Lang (balanced stack, Gfx-owned)
      paint order index on every record (needed for occlusion, §3.1)
 └─ post-pass (pure C++, no Gfx):  page model -> blocks -> {table, heading, list, math, figure, para}
 └─ serializers: markdown(portable|rich), report.json
```
Why not reuse existing dumpers: `TextOutputDev::stroke` returns immediately unless `doHTML`, and only keeps single 2-point subpaths (`poppler/TextOutputDev.cc:5746-5760`); `opRectangle` only builds a path (`poppler/Gfx.cc:1730-1743`), so `re f`/`re S` rulings reach an OutputDev only through `fill/stroke(state)` with a `GfxPath`. So rulings are obtainable with **no Gfx changes**, only a new OutputDev (capability booleans at `poppler/OutputDev.h:80-106`). Pattern/shading caveat: tiling patterns can expand to tens of thousands of fill callbacks (coordinator measured 31,304 callbacks vs 1 intercepted); the OutputDev must intercept patterns and record their bbox as one figure element (coordinator README, `callback-results.json`).

Existing hidden fields that the post-pass should read rather than recompute: `TextBlock::tableId/tableEnd` (private, `TextOutputDev.h:446-447`, assigned `TextOutputDev.cc:3469-3660` — a *clustering hint*, not row/col/span structure); `TextWord::spaceAfter/underlined` (`TextOutputDev.h:210-211`); font name/size/bold/italic (`TextOutputDev.h:121-128, 189-206`); fake-bold/overprint dedup (`TextOutputDev.cc:1647`, thresholds `:157`). pdftotext's `-tsv` / `-bbox-layout` emit word boxes but no font, no space_after, flows commented out (`utils/pdftotext.cc:603`) — so the system CLI cannot be the fast path without new fields.

---------------------------------------------------------------------------

## 3. Mechanisms (the inventions)

### 3.1 Ruled tables: ruling-graph → union-find span grid  [prototype F]
1. Collect axis-aligned segments (strokes of 2-pt paths; thin filled rects ≤ ~1.5 pt as rules). Snap coordinates (tol 1.5 pt) → candidate gridlines `xs[]`, `ys[]`.
2. For each elementary cell (i,j) test whether its right and bottom edges are ≥ 90 % covered by a rule on that gridline. **Missing shared edge ⇒ union the two cells.** Connected components = logical cells; `(r0,c0,rowspan,colspan)` fall out; a component whose bbox ≠ its member count is flagged **non-rectangular** (→ HTML `rowspan` cannot express it; use crop fallback).
3. Assign words by center point; repair step below.
4. **Occlusion (painter's order):** opaque fills painted after a rule hide it. A fill hides a rule only if it covers the **entire stroke width** — my first prototype used ±1 pt tolerance and wrongly erased real rules (see §5.1). The real implementation must use stroke width from `GfxState` and paint-order indices.
5. **Rule-deficient repair:** a spanned cell containing ≥2 text clusters separated by >12 pt (and not a header) is split per cluster (cluster extends to the column before the next cluster). Always emitted with a flag `REPAIRED`; `--tables=crop` refuses repairs.

### 3.2 Borderless tables: word-edge alignment (not run gutters)  [F on real page; prototype defect P]
Right-/left-/center-aligned numeric columns show as clusters of word edges. On the real Apple 10-K "Consolidated Statements of Operations" page (`results/real_aapl_ops_page.gutters.txt`): 176 words, 65 numeric; right-edge clustering (tol 3 pt, ≥5 members) yields exactly **three columns at x1 = 420.9 / 489.7 / 558.9, 20 hits each**; 20 of 32 rows hit ≥2 columns (remaining rows are section labels / footers). A row-evidence score = (#columns hit)/(#columns) plus label-column continuation and zebra bands/underline rules gives table extent. Wrapped labels: a line occupying only column 0, gap ≤ 1.2× median row pitch, previous row filled in other columns, lowercase start → merged into the previous row (works on synthetic fixture).
**Known failure [P/F]:** my prototype consumed `pdftohtml -xml` runs; HtmlOutputDev merges adjacent words across columns into one run, so the same real page produced one cell holding `$ 307,003  $ 294,866  $ 298,085` (`results/real_aapl_ops_page.proto.md`). Lesson: the C++ design must consume `TextWord` boxes, never HtmlOutputDev runs.

### 3.3 Reading order, headings, lists
- Reading order: keep Poppler's flow/block order inside non-table regions; tables/figures are lifted as atomic blocks placed by their top-left; tagged pages use `StructTreeRoot` order when present and `MarkInfo/Marked` not suspect (existing but slow path: `StructElement.cc:819-869` re-renders per leaf — build one MCID→words map instead; prior-review claim ≈80× verified-by-reference only, not re-measured by me).
- Headings: font-size **ranks** among lines that are short, not table-bound, and (bold ∨ larger than body mode) → `#` levels by distinct size rank. Prototype ranks correctly on fixture (20 pt→`#`, 14 pt→`##`). Real pages need body-mode computed per document (outlier-robust) and outline/StructElem H1-H6 override.
- Lists: bullet/number glyph in its own word at consistent left margin + hanging indent. [S, not tested]

### 3.4 Math: geometry → LaTeX with crop fallback  [synthetic only]
Cluster runs around a large Symbol-font operator; limits = runs above/below the operator center; scripts = runs with size < 0.85 body whose center is above/below the previous base and within a small x gap; fraction = two stacked runs separated by a horizontal rule from the evidence rule set; trailing `(n)` ≥100 pt away → `\tag{n}`. On my fixture it emits `\sum_{i=1}^{n} x_{i} = \frac{a}{b} \tag{1}` and inline `E = $mc^{2}$ and $x_{i}$ + y`. **Scope of evidence: one hand-written equation in Helvetica/Times/Symbol, with positions chosen by me.** It says nothing about LaTeX-produced PDFs (CMMI/CMSY/CMEX private encodings, radicals as glyph+rule, matrices, multi-line alignment). The numeric score previously printed (`conf=0.9`) was an arbitrary constant and has been removed; output now says `geometric heuristic, uncalibrated`. A real implementation needs a font-name→glyph-table layer (cm*, msam, stix, Cambria Math) [S] and a calibration set (arXiv sample) before any confidence number is shown. Failure policy: if parse is not a clean tree, emit the **crop + alt text** (Unicode linearization), never wrong LaTeX.

### 3.5 Shapes and images: figure regions by element clustering  [prototype F]
Merge (gap 10 pt) all non-table, non-occluder fills, rules and image placements into connected regions; attach text inside (+14 pt caption band) as labels; render crop with `pdftocairo -x -y -W -H` (prototype) or Splash/Cairo on demand in-process. Fixture result: flow diagram → Figure 1 (labels Parse, Classify), bar chart → Figure 2 (A, B, C), embedded image → Figure 3 (`results/shapes.figures.md`, crops in `results/shapes_figs/`). **Image composition rule:** if any vector/text element overlaps an image bbox, emit the composited crop (not the raw XObject); otherwise export the raw image (`pdfimages`-style) and place by CTM. Raw `pdfimages -list` has no placement bbox (only ppi) — placement must come from `drawImage` CTM (`OutputDev.h:308`; prior review notes the CTM is available at `utils/ImageOutputDev.cc:236-250`, unverified by me).
Hairline-only regions (single rule) are decoration/table fragments, not figures; fixture has no negative test for e.g. page borders [S].

---------------------------------------------------------------------------

## 4. Fidelity and fallback contract

Every emitted fragment has `kind`, `bbox`, `source` (runs/MCIDs/refs), `basis` (ruling|alignment|structure|heuristic), `flags[]`, `confidence_class` ∈ {high, medium, low} (class, not a probability, until calibrated), and `fallback` ∈ {none, crop, svg, text}.

| kind | high | medium | low → fallback |
|---|---|---|---|
| table, ruled | all cells rectangular, ≥90 % edge coverage, no REPAIRED | one REPAIRED cell | non-rectangular component or rules from patterns → **crop** + best-effort text table |
| table, borderless | ≥3 rows, ≥2 aligned columns, row-hit ≥ 0.8 | wrapped-row merges | <3 rows or many empty cells → plain paragraphs + crop |
| heading | outline/struct match or unique size rank | size rank only | ambiguous → bold paragraph |
| math | n/a until calibrated | clean parse | any unparsable cluster → crop + Unicode alt |
| figure | image / ≥4 elements | clustered vectors | huge element count / patterns → crop only |
| text | glyph→Unicode via ToUnicode/AGL | identity-guess | provenance flags drive `--ocr-hint` (prior-review item, unverified here) |

Honest limits: scanned pages (no text) → image only; rotated pages need /Rotate-aware coords (prior review says `-bbox/-tsv` ignore /Rotate, `pdftotext.cc:593-594`; I did not test); right-to-left and vertical writing untested; tagged-PDF paths untested.

---------------------------------------------------------------------------

## 5. Experiments and results (all reproducible; commands in §8)

### 5.1 Ruled-table fixture: real topology vs labels
Fixture `fixtures/ruled_merged.pdf` (generator `src/mkfixtures.py`). **Real drawn topology**, read from the rendered PDF and its rule list (not from the generator's comments; image `results/images/ruled_merged.closeup.png`, page `results/images/ruled_merged.page.png`):
- H rules y(top-down) 92,122,182,212,242 over x 72–472; **y=152 only x 172–472** (so "EMEA" spans rows 1–2).
- V rules: x=72,172,472 full height; **x=272 only from y=122** (header cell "2025 revenue" spans columns 1–2); **x=372 only y 92–182** (absent in the last two rows).
- The two white fill rects (x 173–271, 273–371, y 213–241) are **offset by 1 pt and do not cover** the x=172 / x=272 rules (stroke 0.8 pt), so those verticals **are visible** in the footer row (confirmed by the closeup). My fixture comment "colspan 3 / whiteout" was wrong: the label "Total (colspan 3)" is text only. The text "Total (colspan 3)" is also drawn **twice** at the same position (generator draws it before and after the fills) — an overprint that Poppler's dedup (`TextOutputDev.cc:1647`) removes and my prototype dedups by (text,x,y).
- Therefore the **true grid** is: header row = Region | [2025 revenue (USD m) span 2] | Growth; EMEA rowspan 2; APAC row = APAC | Q1 | [one merged cell x272–472 holding "980.2" and "-1.2%"]; footer row = Total (colspan 3)-text-in-col0 | empty | [merged x272–472 holding "+3.9%"].

Prototype outputs on the **fixed** occlusion test (`results/ruled_merged.proto.norepair.md` = rules only; `.proto.md` = with repair):
- rules only: rowspan=2 EMEA ✔, header colspan=2 ✔, APAC row `colspan="2"` cell "980.2 -1.2%" (flagged rules-deficient, matches drawn topology), footer `<td>Total…</td><td></td><td colspan="2">+3.9%</td>` ✔ (matches drawn topology).
- with repair: APAC row split to 980.2 | -1.2% (repair is a **guess** the PDF's rules do not support; flagged `REPAIRED`).
- Earlier prototype bug [P]: occlusion tolerance of ±1 pt erased the real x=172/x=272 rules and produced `colspan="4"` footer; this was my bug, not the PDF's topology.

### 5.2 Markdown portability check (actual rendering, markdown-it-py 4.2.0 commonmark preset + table + strikethrough, html on)
- Earlier prototype wrote `**bold **` / `*italic *` (whitespace inside delimiters) and `**bold**` inside raw `<td>`. **Negative control** (`results/emphasis_negative_control.md`→`.rendered.html`): both stay literal (`<p>Body **bold ** and *italic * x</p>`, `<td>**b**</td>`). Reproduced; fixed.
- Fix: whitespace is moved outside delimiters and special characters escaped for Markdown context; **inside HTML tables the serializer emits `<strong>/<em>` with `html.escape`** (HTML blocks do not parse Markdown). Re-render: `results/heading_math.rendered.html` shows `<strong>bold</strong> and <em>italic</em>`, and the table `.rendered.html` passes `<table>` through with `<strong>` cells.
- Remaining rendering defects [P]: `$` math delimiters are rendered literally by plain CommonMark (expected; needs a math extension or KaTeX/MathJax in the consumer); `\tag{1}` same.

### 5.3 Borderless fixture and real-page probe
`results/borderless.proto.md`: header row, three numeric columns, wrapped-label merge "Widget beta with a long description wrapped" ✔ (zebra shading ignored — usable as extra evidence [S]). Guard added after a **false positive**: a prose page with sub/superscripts produced a 4-column "table" (script runs counted as columns); fixed by counting only body-size runs and requiring ≥3 consecutive candidate rows. Real page: see §3.2 (word-edge probe good; run-based prototype bad).

### 5.4 Text-only baseline on the same fixtures (system `pdftotext -layout`, `results/*.layout.txt`)
Table: rowspan/colspan structure lost (blank cell under "Region"); display math becomes `∑ x = abi` with limits scattered over three lines; superscript `2` floats on its own line away from `mc`; image and shapes vanish entirely. `pdfimages -list` reports the image (64×64, 38 ppi) but no position (`results/shapes.pdfimages.txt`).

### 5.5 Speed probes (single runs, machine under concurrent council load; `results/bench/timings.txt`)
| tool (system 26.10.0) | input | wall | user |
|---|---|---|---|
| pdftotext plain | AAPL 10-K 80 p | 0.80 s | 0.35 s |
| pdftotext -layout | same | 0.71 s | 0.33 s |
| pdftotext -bbox-layout | same | 1.09 s | 0.48 s |
| pdftohtml -xml | same | 1.39 s | 0.63 s |
| **pdftocairo -svg** | **pages 1–20 only** | **4.07 s** | 1.46 s (≈0.2 s/page, 10.4 MB SVG) |
| pdfimages -list | same 80 p | 0.63 s | 0.23 s |
| pdftotext -layout / pdftohtml -xml | US Code title 1, 21 p | 0.29 / 0.14 s | 0.09 / 0.06 s |
Reading: the SVG route to rulings costs roughly 20× text per page, which is why §2 records vector evidence in-process instead. The in-process cost is **not measured** (needs a build + prototype OutputDev; see §9).

### 5.6 Figure fragments: crop vs editable SVG (`results/shapes_figs/`, `results/images/fig1_crop_vs_svg.png`, `fig2_crop_vs_svg.png`)
Fig 1 (box-arrow diagram): PNG crop 2,882 B (110 dpi) vs inline SVG 751 B (6 elements). Fig 2 (bar chart): PNG 804 B vs SVG 888 B. SVG keeps editable labels and exact geometry; **limits**: fonts are substituted by the viewer (generic Helvetica,Arial), text positions use `pdftohtml` run boxes (baseline approximated as `y1-2`), arrowhead is a polygon (ok), clipping/transparency/gradients/patterns/embedded raster are not translated. No numeric pixel-diff reported (viewBox/margins differ between the crop and the SVG render, so an RMSE would be misleading).

---------------------------------------------------------------------------

## 6. Rich profile: HTML/SVG fragments in Markdown (user-requested, non-standard)

Four independent axes; the CLI should expose them separately rather than as one "rich" blob.

| axis | choices | consequences |
|---|---|---|
| **Renderer compatibility** | `portable` (CommonMark+GFM) vs `rich` (raw HTML + SVG + math) | GFM tables cannot express spans: *"Block-level elements cannot be inserted in a table"*; cells are inline-only (GFM spec, verified). Raw HTML blocks pass through when the line starts with a block tag such as `<table>` (start condition 6 lists `table, tbody, td, th, tr`), and **Markdown inside HTML blocks is not parsed** (verified by my render check). `svg` is not in the type-6 list; it works as an inline/type-7 HTML start (depends on the parser) [S per parser]. |
| **Editable semantics vs visual preservation** | semantic: HTML `<table>` with `rowspan/colspan`, `<th scope>`, text, `<sup>/<sub>`, `$latex$`; visual: PNG/SVG of the region | Semantic = searchable, chunkable, lossy for exotic layout; visual = faithful, opaque to text search. Recommended default: **both** — semantic fragment as the content, crop referenced from a sibling `<!-- fidelity: … fallback=assets/p1-t1.png -->` comment or `<img>` inside `<details>`. |
| **External assets vs inline** | `--assets dir` (relative `![](assets/…)`) vs `--assets inline` (`data:` URIs / inline `<svg>`) | Inline SVG makes a self-contained file but bloats; `data:` image URIs are **not accepted by html-pipeline's default sanitizer** (img src limited to http/https/relative per `html-pipeline/lib/html_pipeline/sanitization_filter.rb`, fetched 2026-10-25). Prefer relative asset paths for portable output; inline only when the consumer is DocSlurp itself. |
| **Faithful fallback limits** | crop = pixel-faithful at a given DPI, loses text; SVG = vector but approximate fonts; HTML table = structure but no ruling/shading unless styled | State the loss per fragment in `--report`, not just in prose. |

Sanitizer evidence (what a Markdown consumer may strip) — html-pipeline's **default** allowlist (a library used by GitHub-style pipelines; **GitHub.com's live behavior is not verified here**): `table`, `rowspan`, `colspan`, `img` allowed; **`svg` not allowed, `style` element/attribute not allowed**, `img src` only http/https/relative. Consequence: in `rich` mode keep spans via attributes (`rowspan/colspan` survive), avoid `style=`; ship SVG as a **sidecar file referenced by `<img src="assets/fig1.svg">`** for sanitizing renderers, with inline `<svg>` only behind `--assets inline`. Bold/italic in cells must be `<strong>/<em>`. A `rich` Markdown file should also be fully readable as text when HTML is stripped (cell text stays in document order).

Concrete merged-table exporter (implemented in the prototype): `render_table()` emits GFM if no span and `--tables gfm`; HTML `<table>` with `<th>` first row, `rowspan/colspan`, `<strong>/<em>`, `html.escape` otherwise; non-rectangular components are *not* serializable and must use the crop fallback (flag implemented, fallback not).

---------------------------------------------------------------------------

## 7. Competing approaches considered and rejected (or demoted)

| approach | why not primary |
|---|---|
| `pdftotext -layout` + whitespace column splitting | loses spans, merges columns, no rulings; measured §5.4. Keep only as `portable` last-resort text. |
| Parse `pdftohtml -xml` runs (my prototype's input) | runs merge across columns (real page failure, §3.2); no vector data, no images placement. Prototype-only. |
| `pdftocairo -svg` as ruling source | works (prototype) but ~0.2 s/page and 10 MB per 20 pages; use only as the experiment oracle. |
| `TextOutputDev` `tableId` | private clustering hint (`TextOutputDev.h:446`); no rows/cols/spans; exposing it alone cannot produce merged cells. Use as a weak prior only. |
| Raster layout models (ONNX/YOLO-style) as primary | out of scope for "extremely fast"; keep as an optional crop-refine step for low-confidence regions [S]. Existing cache has `layout-onnx-runtime`; not evaluated. |
| Mermaid/diagram reconstruction of vector figures | reconstructing semantics from boxes/arrows is unreliable; crop/SVG instead [S]. |
| Rendering every page to image for visual diff | too slow for the fast path; reserve for on-demand fallbacks and a CI visual-regression oracle. |

---------------------------------------------------------------------------

## 8. Reproduce

```sh
export TMPDIR=/home/dan/tmp/agents TMP=$TMPDIR TEMP=$TMPDIR
cd docslurp-review/markdown-20261025/fidelity
python3 -I src/mkfixtures.py fixtures                      # 4 synthetic PDFs
python3 src/md_proto.py fixtures/ruled_merged.pdf [--no-repair|--tables gfm|html]
python3 src/md_proto.py fixtures/borderless.pdf ; python3 src/md_proto.py fixtures/heading_math.pdf
python3 src/figures.py fixtures/shapes.pdf results/shapes_figs     # figure regions + crops
python3 src/svgfrag.py fixtures/shapes.pdf results/shapes_figs 72 112 322 152 fig1   # editable SVG
python3 src/gutter_probe.py ~/.cache/docslurp/pricing-samples-2026-10-21/sec-10k/aapl-10k-fy2025-as-filed.pdf 32
# CommonMark check: python3 -m venv $D/v && $D/v/bin/pip install markdown-it-py ; $D/v/bin/python src/render_check.py results/heading_math.proto.md
```
Fixtures are synthetic and tiny, no private documents used; real-document probes read only the public AAPL 10-K sample from `~/.cache/docslurp` (nothing uploaded). Raw outputs are in `results/`.

---------------------------------------------------------------------------

## 9. Known failures, limitations, and what I did not test

- Fixtures are hand-built; topology labels in my generator comments were wrong (§5.1) — only the rendered PDF/rule list is authoritative.
- Prototype `--tables gfm` path (`results/ruled_merged.proto.gfm.md`) is wrong in two visible ways [P]: it builds its cell grid **before** the REPAIRED split (so the APAC row still shows `980.2 -1.2%` in one cell although the flag says repaired), and spans are simply placed in the first cell with empty neighbours, losing the header span. That is inherent to GFM (no spans), but the repaired-grid ordering is a bug. The HTML path is the validated one.
- Prototype inline-math spacing still imperfect; math score removed as uncalibrated; no LaTeX-font PDFs tested; no matrices/radicals/multiline.
- Prototype consumes pdftohtml runs → real borderless financial tables collapse into one cell; word-level path only validated by the gutter probe, not an end-to-end table emitter on real pages.
- One real page tested for borderless alignment; no real ruled-table page, no multi-page tables, no rotated pages, no RTL/CJK, no tagged-PDF join, no annotations/links in output, no list detection.
- Heading detection tested on one fixture; real documents need document-level body mode and outline/struct override.
- SVG fragment is approximate (fonts, baseline); no gradients/clips/transparency; no numeric visual diff.
- GitHub.com's current sanitizer not verified (only html-pipeline default config read); other renderers (GitLab, Obsidian, VS Code) differ.
- No in-process OutputDev was built, so the speed claim for in-process recording is a projection, not a measurement.

**Needed experiment (requires a checkout build, not done to avoid shared mutable state):** build a minimal `EvidenceOutputDev` as a separate out-of-tree executable linking installed libpoppler (as the coordinator's `callback_probe.cc` does) that records words + rules + rect fills + image CTMs and writes JSON; time it on the AAPL 10-K (80 p) vs `pdftotext -bbox-layout` (1.09 s) and vs `pdftocairo -svg` (≈0.2 s/p) to replace the §5.5 projection with a measurement; then run the §3.1/3.2 post-pass on the JSON for the real ruled/borderless pages.

## 10. Artifact index (all under `docslurp-review/markdown-20261025/fidelity/`)
`fixtures/*.pdf` · `src/{mkfixtures,md_proto,figures,svgfrag,gutter_probe,render_check}.py` · `results/{*.layout.txt,*.bboxlayout.html,*.pdftohtml.xml,*.svg}` baselines · `results/{ruled_merged,borderless,heading_math}.proto*.md` · `results/*.rendered.html`, `emphasis_negative_control.*` · `results/shapes.figures.md`, `results/shapes_figs/` · `results/real_aapl_ops_page.{proto.md,gutters.txt}` · `results/images/*.png` (page renders, `ruled_merged.closeup.png`, `real_aapl_ops_page.png`, crop-vs-SVG pairs) · `results/bench/timings.txt`.
