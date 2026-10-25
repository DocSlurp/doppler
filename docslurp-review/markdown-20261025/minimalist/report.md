# Ruthless Minimalist: fast Markdown export from Poppler for DocSlurp

Date 2026-10-25. Checkout `/home/dan/code/oss/poppler`, master @ `e658c5562` (PACKAGE_VERSION `26.10.90`).
**No checkout build exists** (`build/utils` has only CMakeFiles). All measurements use **system Poppler 26.10.0**
(`/usr/bin/pdftotext`, `pdftohtml`, `pdfinfo`, `pdftocairo`, `pdftoppm`, `pdfimages`; Ghostscript 10.08.0 made the fixture).
No tracked files were modified by me. `git status` shows a modified `CMakeLists.txt` and untracked `DOCSLURP.md`; neither is mine.
There is no repo-level `AGENTS.md` in the checkout; only `/home/dan/AGENTS.md` (scratch under `/home/dan/tmp/agents`), which I followed.

Labels: **[F]** = verified finding (source line or a run I did). **[S]** = speculation / estimate.

## 0. Recommendation in one paragraph

Ship **`pdftomd`**: one new `utils/pdftomd.cc` (~700–1000 LoC **[S]**), a *subclass of the existing `TextOutputDev`* plus a
post-pass over the layout tree Poppler already builds. No changes to libpoppler are required for tier 1 **[F: see §2]**.
Page cost stays at pdftotext cost (one interpret per page, no rendering). Everything else is opt-in tiers that only cost
when the page needs them: raster/SVG crops for figures and vector diagrams, and the structure tree for tagged PDFs.
Do **not** start with a new layout engine, a ML model, or an `OutputDev` fan-out framework.
Where Poppler can't know (math, complex tables, diagrams) emit a *labelled fallback* (HTML fragment or image crop) and a
machine-readable per-page quality report instead of pretending.

## 1. Proposed CLI

```
pdftomd [options] <pdf> [out.md|-]
  -f N -l N               page range (reuse pdftotext arg parsing)
  --profile gfm|rich      gfm (default): CommonMark+GFM only.  rich: may embed HTML/SVG fragments (see §5)
  --assets DIR            write images / figure crops / svg here, reference by relative path (default: no assets, placeholders only)
  --inline-assets BYTES   rich only: inline SVG/data-URI under this size (default 0 = never)
  --figures none|placeholder|crop|svg   how vector-art regions are handled (default placeholder)
  --tables auto|rules|aligned|off
  --structure auto|on|off use StructTreeRoot when the PDF is Tagged (default auto)
  --front-matter          YAML: title/author/subject/keywords/dates/pages/producer/tagged/lang/outline
  --report FILE.json      per-page fidelity report (§4)
  --shard I/N             emit only pages i mod n (for parallel workers; see §6)
  -opw/-upw, -cropbox     same as other utils
```
Output is deterministic UTF-8; page boundaries as `<!-- page N -->` (or a thematic break, flag). Exit 0 even for
degraded pages; degradation is reported, never silent.

## 2. Architecture (smallest thing that works)

```
PDFDoc ─► for each page: displayPage(MdDev)
             MdDev : TextOutputDev           (reading order, flows/blocks/lines/words, fonts, colour, links, baseline)
               + override fill/stroke/eoFill  (record h/v rules and rects: table borders, shape bboxes, path counts)
               + override needNonText()=true  ONLY if --figures≠none or images wanted; drawImage*() records CTM bbox
                 (+ optionally forward to ImageOutputDev to write files)
          ─► MdWalker: flows→blocks→lines→words ⇒ classify (heading/para/list/code/table/figure) ⇒ emit CommonMark
          ─► optional: struct-tree pass (tagged only) / crop pass (figures only)
metadata: PDFDoc::getDocInfo*, Catalog outline, link annotations (via TextOutputDev link inlining)
```

Evidence **[F]** that this reuses existing parts and needs no library change:

| Need | Existing API (checkout) |
|---|---|
| Reading order, columns, blocks | `TextOutputDev::getFlows()` → `TextFlow::getBlocks()` → `TextBlock::getLines()` → `TextLine::getWords()` — `poppler/TextOutputDev.h:493,415,341,655`; `utils/pdftotext.cc:560-570` already walks it for `-bbox-layout` |
| Per-word font/size/colour/baseline/underline/link | `TextWord::getFontInfo/getFontSize/getColor/getBaseline/isUnderlined/getLink` — `TextOutputDev.h:181,206,196,208,211,212` |
| Bold / italic / mono / serif flags | `TextFontInfo::isBold/isItalic/isFixedWidth/isSerif` — `TextOutputDev.h:124-128` |
| Links → words | `enableHTMLExtras(true)` (`:876`) + `setInlineLinkURIs` (`:878`); `processLink` is gated by `doHTML \|\| inlineLinkURIs` at `TextOutputDev.cc:5848`; link-to-word assignment in `coalesce` (`:2899,2960`) |
| Table rules (h/v lines, thin rects) | `TextOutputDev::stroke/fill` already detect them, but only feed `addUnderline` and only when `doHTML` (`TextOutputDev.cc:5746-5833`). The list is private, so the subclass re-records the same geometry (~60 LoC) in its own overrides. Both are virtual (`TextOutputDev.h:826-828`) |
| Images | `needNonText()` is a virtual returning false (`TextOutputDev.h:796`); Gfx skips image XObjects when false (`Gfx.cc:4155`). Override to true, record bbox in `drawImage`; `ImageOutputDev` (`utils/ImageOutputDev.h:44,134`) can be a delegate to write files |
| Metadata | `PDFDoc::getDocInfoTitle/Author/...` (`PDFDoc.h:275-278`), outline via Catalog |
| Tagged structure | `StructTreeRoot` / `StructElement::Type` (H1…, Table, TR, TD, L, LI, Figure, Formula, Link, Code; `StructElement.h:166-225`) |
| Build wiring | copy the `pdftotext` stanza in `utils/CMakeLists.txt:138-` |

### Tier plan (each tier is shippable; later tiers only add)

* **T0 (day 1, zero C++): script over `pdftohtml -xml`.** Probe in this dir (`xml2md.py`, ~110 lines). Proves headings, bold/italic,
  links, pipe tables, and metadata (`pdfinfo`). Useful as a regression oracle and as a stopgap, *not* as the product (§3 shows why).
* **T1: `pdftomd` C++ tool** as above: headings, paragraphs, lists, code, links, emphasis, tables (rule-based then alignment-based),
  metadata/front-matter, image placeholders, basic super/subscript, per-page report.
* **T2: assets.** `--assets`: embedded images via `ImageOutputDev`; figure crops via the existing Splash/Cairo path (only for pages/regions flagged "has vector art").
* **T3: structure.** For `Tagged: yes` docs use the structure tree for table/list/heading roles instead of heuristics. Caveats in §3.
* **T4 (not the minimalist's job, listed for honesty):** formulas (OCR/ML), scanned pages (OCR), semantic tables for borderless layouts.

## 3. Findings and experiments (what is proven)

### 3.1 Cost of the existing paths (system Poppler 26.10.0, best-of-3 wall, machine was busy with other council runs, so ±2× noise)
Raw: `results/bench-system-poppler-26.10.0.tsv`; script `bench.py`.

| doc (pages) | pdftotext plain | -layout | -bbox-layout | -tsv | pdftohtml -xml (no img) | pdfimages -list |
|---|---|---|---|---|---|---|
| AAPL 10-K (80, tagged) | 0.41 s | 0.60 | 0.99 | 1.22 | 1.52 | 0.38 |
| US Code title 1 (21, untagged) | 0.23 | 0.37 | 0.51 | 0.73 | 0.31 | 0.18 |
| scanned book (26, image only) | 0.11 | 0.19 | 0.15 | 0.28 | see tsv | see tsv |

**[F]** The layout tree (`-bbox-layout`) costs roughly 2× `pdftotext` plain, almost all of it *printing* 5 MB of XML; a native walker skips that.
**[F]** AAPL full XML→Markdown with my Python: 0.68 s user for `pdftohtml`, 0.15 s user for the converter. So T0 is already ~sub-second per 80 pages (single CPU) **[F]**; a C++ T1 should land at ≈ pdftotext cost, 0.4–0.6 s **[S]**.
**[F]** `pdfinfo -struct-text` on AAPL took **57 s wall (12.5 s user)** for 80 pages — it re-runs page display per leaf element (`StructElement.cc:819-869`; the coordinator found the same). Using the structure tree through that API per element is a trap; T3 must traverse once and join by MCID.

### 3.2 Fidelity probes (T0 prototype; outputs in `results/`)
* `results/synth.md` from `fixtures/synth.pdf` (Ghostscript-made: title, h2, italic, rules-table, superscript, mono, rect, image, link, outline). **[F]**
  Got: `#`/`##` from size+bold, `*italic*`, a correct 4-col pipe table with bold header, link text, metadata (Title/Author/outline `Intro` visible in XML).
  Missed: (a) E=mc² came out `2 E = mc` — I sorted rows by `top`, so the superscript preceded the base (my prototype bug; the fix is to use baseline/`TextWord::getBaseline` in a line, which T1 has); (b) monospace line not fenced (the family is in `fontspec`, trivially fixable; T1 has `isFixedWidth`); (c) image/rect dropped (`-i` and no shape support in XML); (d) link attached to the whole *line* not the span — pdftohtml's XML link granularity is line-level. T1 gets word-level link via `TextWord::getLink`.
* `results/aapl-p28.md` (real financial table): table *detected* but wrong: `$` and amounts are separate cells, header row has fewer columns than body (column count differs per row). **[F]** Needs column clustering by x-position across all rows of the table (T1 work item #1) and merging of currency-symbol cells.
* `results/uscode-p3.md` (3-column legal page): **[F] reading order destroyed** — my re-sort across the XML lost the flow/column grouping. Lesson: *never post-process a flat XML*; iterate Poppler's flow→block order. This single result is the strongest argument for the C++ tool over a script, and it is a quality argument, not a speed one.
* **[F] AAPL is `Tagged: yes` but has no H1–H6 at all**: `pdfinfo -struct` tag counts: TD 4163, P 979, TR 656, Link 242, LI 133, Table 64, Figure 2, no `H*`. Headings still need font heuristics even for tagged PDFs; tags help for tables/lists/figures-with-alt only. The first 8 tables are empty-looking `Table/THead/TBody/TR "Row 1"/TD` (cover-page layout tables), so "Table tag" ≠ "data table" either.

### 3.3 Source-level hazards that shape the design (**[F]**, line cites)
* `Gfx.cc:5166-5170`: BDC forwards only inline dictionaries (`numArgs==2 && isDict`) or one-arg BMC. Named-property BDC (`/P /MC0 BDC` with `/Properties`) is not forwarded (coordinator reproduced). A tagged-PDF MCID join will silently lose those paragraphs → T3 needs a ~10-line fix in Gfx *or* must fall back to geometric layout for pages where MCID coverage < threshold. This is the only place I'd touch libpoppler, and only for T3.
* `TextOutputDev.h:796` `needNonText()=false` also skips patterns/shadings/forms-with-images (`Gfx.cc:1937,1964,1991,4155`). That's why text extraction is fast; turning it on costs (coordinator measured 31k fills for one tiling pattern). So `MdDev` keeps it false by default and sets it true only when images/figures are requested, preferably for the second pass of only the flagged pages.
* `TextOutputDev.cc:5696` coalesce runs at page end and groups irreversibly, fine for a single consumer, the one DocSlurp needs here.

### 3.4 HTML/SVG fragments probe (pdftocairo -svg, system 26.10.0; `results/svg-probe.txt`) **[F]**
| page | time | SVG bytes | `<text>` | glyph defs |
|---|---|---|---|---|
| synth p1 | 0.08 s | 82 KB | 0 | 102 |
| AAPL p28 | 0.49 s | 505 KB | 0 | 142 |
| US Code p3 | 0.22 s | 522 KB | 0 | 96 |
Whole-page SVG is **glyphs-as-paths, zero `<text>`**: visually faithful, not searchable/editable, ~0.5 MB per text page. `pdftocairo -svg -x -y -W -H` kept `viewBox 0 0 612 792`
and produced a 51 KB file instead of 82 KB; I did not verify geometry, so treat region SVG crop as **unproven** (`results/synth-crop-attempt.svg`).
Raster crop works: `pdftoppm -x -y -W -H` of AAPL p28, 0.20 s (incl. document open) vs full page 0.90 s via pdftocairo -png at 72 dpi.
Embedded-image listing is nearly free (`pdfimages -list`: 0.38 s for 80 pages) and tells you per page whether T2 is needed **[F]**; fixture inline image shows up as `[inline]`.

## 4. Fidelity / fallback contract

Every page gets a record in `--report`:
```json
{"page":28,"order":"flow|raw|struct","text_source":"unicode|ocr-needed|mixed","tables":[{"bbox":[...],"method":"rules|aligned|struct|fallback-html|fallback-image","conf":0.8}],
 "figures":[{"bbox":[...],"kind":"raster|vector|mixed","asset":"a/p28-f1.png","alt":"…"}],
 "math":[{"bbox":[...],"method":"unicode-inline|html-sub-sup|image|none"}],
 "warnings":["no-text-layer","pua-glyphs:12","multi-col-guess"]}
```
Ladder (each element degrades to the next rung, never drops content):

| Element | Preferred | Fallback 1 | Fallback 2 |
|---|---|---|---|
| Heading | tag H1–H6 (if present) → size/weight rank among body | bold-only short line → `##` | plain paragraph |
| Paragraph | block from TextBlock; join lines, de-hyphenate (`TextLine` hyphen flag) | one line per block | |
| List | struct `L/LI` or bullet/number glyph + indent | paragraph | |
| Code | `isFixedWidth` run ≥ 1 line → fence | inline backticks | |
| Emphasis | `**`/`*` per word run from `isBold/isItalic` | none | |
| Links | `TextWord::getLink` per word span; URL-looking text autolinks | footnote list of page links | |
| Table | ruling-line grid (T1) → x-alignment clusters → struct `Table` | `rich`: HTML `<table>` with spans; `gfm`: pipe table w/ repeated merged cells + warning | raster crop + text dump, flagged |
| Image | `![alt](assets/…)` (alt from struct Figure/Alt if tagged) | placeholder `<!-- image 640x480 p3 -->` | |
| Shapes/diagrams | placeholder with bbox + path count | PNG crop (T2) | SVG fragment in `rich` |
| Math | Unicode text + `<sup>/<sub>` by baseline shift (rich) or `^{}`/`_{}` text (gfm) | image crop of the line | flagged `math:none` |
| Scanned / no text layer | emit nothing, `text_source:"ocr-needed"` | | |
| Metadata | YAML front matter from Info dict + XMP Title/lang | | |

Rule: a page is only claimed "ok" if text coverage (chars emitted ÷ chars seen by TextPage) ≥ 0.98 and no `warnings` of severity error. The report lets the DocSlurp worker escalate per page to a heavier engine instead of
re-processing the document.

## 5. Optional non-standard HTML/SVG mixing (steering update)

Define two profiles, because "renderer compatibility", "editable semantics" and "visual preservation" are three different axes:

| Axis | `gfm` profile | `rich` profile |
|---|---|---|
| Renderer compatibility | Pure CommonMark + GFM (pipe tables, fences, `[]()`). Safe in any viewer, LLM chunker, diff tool | Raw HTML (`<table>` with `rowspan/colspan`, `<sup>/<sub>`, `<figure>`, `<span style>`) and SVG. CommonMark permits raw HTML blocks, but many sinks sanitize or escape it **[S: GFM's tagfilter and typical sanitizers strip `<style>/<script>`/inline `<svg>`; I did not re-verify against the current spec this session]** |
| Editable semantics | Cell text and headings stay real, but merged cells are lost (repeated or blank) | Merged tables keep `rowspan/colspan`; text remains real characters (searchable, selectable) |
| Visual preservation | Low: shapes and diagrams → placeholders/PNG | Higher: diagrams as SVG, colour via `style` |
| Assets | External PNG/SVG under `--assets` (relative path) | Same by default. Inline `<svg>`/data-URI only under `--inline-assets BYTES` |

Guidance:
1. **Default external, not inline.** Verified SVG pages are 80–520 KB each (§3.4); inlining that into Markdown destroys token budgets and diff-ability. Inline only small fragments (e.g. < 8 KB: rules, charts with few paths **[S threshold]**).
2. **Two kinds of SVG are not interchangeable.** What Poppler gives today (`pdftocairo -svg`) is *visual-only*: glyphs-as-paths, no `<text>`. Editable SVG text would need a new emitter that writes `<text>` from `TextWord` positions + a drawing pass for paths; that is not a quick job and is T4. In `rich`, put the **real text in the Markdown/HTML around the SVG** (or in `<title>/<desc>` and `aria-label`) so search still works, and keep SVG purely as the visual layer.
3. **HTML table only when pipe table cannot represent the structure** (spans, multi-row header, nested). Otherwise stay pipe: smaller and LLM-friendly.
4. **Always emit a sidecar fallback pointer** on rich fragments, e.g. `<!-- fallback: assets/p28-t1.png -->`, so a consumer that strips HTML can still reach a visual.
5. **Limits:** SVG region crop unproven in Poppler's cairo SVG path (§3.4) — until verified, vector regions are rasterised (`pdftoppm -x -y -W -H`, proven) or whole-page SVG is referenced. Fonts/PUA glyphs: SVG-as-paths survives even broken ToUnicode, which makes it an *excellent* fallback for pages where the text layer is garbage (`pua-glyphs` warning), at the cost of searchability.
6. `rich` output must still pass a CommonMark parse without error; test with a Markdown linter in CI.

## 6. Fastest path and cost of quality

| Step | What you get | Cost to build **[S]** | Cost per run |
|---|---|---|---|
| T0 script on `pdftohtml -xml` | headings/emph/links/rough tables | 0.5 day (exists as a probe) | ≈ pdftohtml (0.3–1.5 s per 20–80 pp) + 0.15 s |
| T1 `pdftomd` (no libpoppler change) | proper order, word-level links, mono, sub/sup, rule+alignment tables, metadata, report | 1–2 weeks incl. fixtures | ≈ pdftotext plain ×1.2 (0.4–0.6 s / 80 pp) **[S]** |
| T2 assets | images & figure crops | +3–5 days | + `pdfimages -list` 0.38 s to decide; raster crop ~0.2 s/region; second pass only on flagged pages |
| T3 struct tree | tagged tables/lists/alt text | +1 week and a small `Gfx.cc` fix for named BDC | **never** via `getText()` per leaf (57 s on 80 pp **[F]**) |
| Math / OCR / borderless tables | beyond Poppler | out of scope | model/OCR cost |

Parallelism: `PDFDoc` is not shared across threads safely; scale by *process*/`--shard I/N` over page ranges (document open ≈ 0.15–0.2 s, **[F]** from the crop timing) and concatenate. 8 cores here.
Quality is bought in the long tail: table detection and figure policy dominate; everything else is already in the layout tree.

## 7. Rejected approaches (and why)

* **New layout engine / rewrite of `TextOutputDev`.** The existing flow/block/line/word tree already gives reading order and fonts; my probe showed the *quality loss comes from discarding that tree* (flat XML), not from the tree itself.
* **Script over `pdftotext -bbox-layout`.** Keeps blocks but no fonts → can't do headings/emphasis/mono; 5 MB XML for 80 pp.
* **Script over `pdftohtml -xml` as the product.** Fast, but loses flow structure (USCODE columns scrambled), link granularity is line-level, and images/shapes absent in XML.
* **Fan-out `OutputDev` “tee” framework / prior review's `PagePass`, `InterpretProfile`, `GlyphTextResolver`.** Valuable for engine hygiene, but none is needed to ship Markdown; a single subclass plus a lazy second pass covers it. I did not verify the prior review's numbers (e.g. 2,537 Gfx8BitFont builds, 17×), so I rely on none of them.
* **Rasterise every page + vision model.** Violates "extremely fast", paid/remote compute not allowed here, and kills searchable text.
* **Whole-page SVG for everything.** 0.5 MB/page, glyph paths (no text) — a fallback, not a representation.
* **Use `pdfinfo -struct-text` / `StructElement::getText` for tagged text.** 57 s for 80 pages **[F]**.

## 8. Limitations / unknowns (honest)

* Everything measured is system Poppler 26.10.0 and external tools; the C++ tool is **not built or benchmarked**. The "T1 ≈ pdftotext cost" figure is an estimate.
* Benchmarks ran while other council members loaded the machine; wall times are best-of-3 with ±2× noise (e.g. AAPL XML measured 1.5 s in bench, 3.6 s in one cold run, 0.68 s user).
* Table detection heuristics were only exercised on 3 real pages + 1 synthetic; no accuracy metric. Borderless, merged and rotated tables remain unproven.
* Math: only super/subscript-by-baseline is cheap; real equations (fractions, radicals, fonts like CM/Symbol with PUA) need an image or model fallback; not tested on real math PDFs (none in the local corpus).
* Right-to-left, vertical writing, rotated pages: not tested; `TextWord::getRotation` exists (`TextOutputDev.h:207`).
* HTML sanitization behaviour of target viewers is unverified (§5).
* DocSlurp integration (coordinator: `worker/src/native-pdf/poppler-engine.ts:17-28` collapses whitespace; `NativePdfTable` has no cells) — I did not re-read that repo. The `--report` JSON is meant to map onto richer table/figure records there; that mapping is a separate decision.

## 9. Artifacts (all under this directory)

* `report.md` — this file; `README.md` — short rationale + reproduction
* `bench.py`, `results/bench-system-poppler-26.10.0.tsv` — raw timings
* `xml2md.py` — T0 prototype; `results/synth.md`, `results/aapl-p1.md`, `results/aapl-p28.md`, `results/uscode-p3.md` — its outputs
* `fixtures/synth.ps`, `fixtures/synth.pdf` — synthetic fixture (public, authored here); AAPL 10-K and US Code PDFs are public documents read in place from `~/.cache/docslurp`, not copied
* `results/svg-probe.txt`, `results/synth-crop-attempt.svg` — SVG/crop probe

Reproduce: `python3 bench.py "" results/x.tsv`; `pdftohtml -xml -stdout -i -zoom 1 fixtures/synth.pdf > s.xml && python3 xml2md.py s.xml`; `pdfinfo -struct <pdf> | sed 's/^ *//' | awk '{print $1}' | sort | uniq -c`.
