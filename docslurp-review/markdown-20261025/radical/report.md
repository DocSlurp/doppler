# Radical clean-slate proposal: fast, layout-aware Markdown export from Poppler for DocSlurp

Persona: clean-slate radical, owning the whole solution.
Checkout `e658c5562` (branch master). Observed 2026-10-25.
Everything below is labelled **[FINDING]** (observed or read in source), **[PROBE]** (my out-of-tree experiment, scoped), or **[SPECULATION]** (reasoned, not measured).

## 0. Verdict in five lines

1. "Markdown from Poppler" is the wrong first abstraction. Markdown is a lossy projection. Build a **page event IR** (a recorder `OutputDev` that keeps text runs, rules/rects/paths, image placements, links, and later marked content) and make Markdown, HTML, SVG fragments and the DocSlurp `scan()` types *projections* of it.
2. Pure display-list replay is the wrong lossless layer: too big, and it chases rendering semantics (clips, patterns, soft masks) that Markdown never uses. Pure text-layout (`TextPage`) is the wrong layer too: it has already thrown away rules, shapes, image placement and run style before anyone can use them. **Recommended: event IR + region-adaptive projector + per-region fallback ladder.**
3. The fast part is fine: interpretation alone is ~0.25 s CPU for an 80-page tagged 10-K **[PROBE]**. The slow parts are (a) naive per-glyph serialisation and (b) image-stream work that `Gfx` does before any device sees the image (JPX init) **[PROBE]**. Both are design constraints, not Markdown problems.
4. The user's "non-standard" mixing of HTML and SVG is supported as a declared **`rich` profile**. Spans, merged cells, `<sup>`, and shape fragments are emitted only there. The `strict` profile degrades with explicit, machine-readable markers instead of silently flattening.
5. Compatibility breaks are named in §7. The biggest: a new tool and a new schema; `pdftotext`/`pdftohtml` behaviour is untouched.

## 1. Evidence base

### 1.1 Source (all paths relative to the checkout)

| Claim | Evidence |
|---|---|
| OutputDev offers everything a recorder needs: `drawChar`, `beginString/endString`, `fill/eoFill/stroke`, `drawImage/Mask/Masked/SoftMasked`, `processLink`, `beginMarkedContent`, `beginActualText`, `beginForm/drawForm` | `poppler/OutputDev.h:272-334` |
| `drawChar` receives `x,y,dx,dy` from `Gfx` as text-state user-space values (`getCurTextX()+riseX`), **not** device coordinates. `TextPage::addChar` transforms them itself. Paths and images are separately in user space and must be transformed by the device | `poppler/Gfx.cc:4035`; recorder fix in `radical/src/recorder.cc` |
| Capability booleans gate interpretation. `needNonText()` is consulted at the image, shading, form and inline-image sites | `poppler/OutputDev.h:80-117`; `poppler/Gfx.cc:1937,1964,1991,4155,4340,4668` |
| `Gfx::doImage` calls `str->getImageParams()` unconditionally when it reaches the image, and `JPXStream::getImageParams` runs `init()` | `poppler/Gfx.cc:4223`; `poppler/JPEG2000Stream.cc:161-165` |
| `TextOutputDev` already records underline candidates from thin filled rects and strokes, but the table work is a block-ordering heuristic with only a private `tableId` hint. It outputs no rows, cols or spans | `poppler/TextOutputDev.cc:5746-5833` (stroke/fill underline), `:3468-3648` (tableId) |
| `TextWord` carries per-word font info, size, colour, rotation, link, baseline. `TextFontInfo` carries bold/italic/fixed/serif/symbolic **descriptor flags** | `poppler/TextOutputDev.h:59-98,181-215,250` |
| `pdftotext` has `-bbox-layout` and `-tsv`, but no style/colour/rules/images | `utils/pdftotext.cc:123-130` |
| `pdftohtml -xml` is a separate engine (`HtmlOutputDev`) with its own bold/italic and link logic and drops rules | `utils/HtmlOutputDev.cc:444-460,525-528,612-672,1180,1436` |
| libpoppler private API is exported and installed, so an out-of-tree device can be compiled and run today | `/usr/include/poppler/*.h`, `/usr/lib/libpoppler.so.165` (26.10.0, same version as the checkout's tools) |

Coordinator-saved findings I did not independently re-run (`coordinator/README.md`): named-property BDC is not forwarded to `beginMarkedContent` (`Gfx.cc:5166`); nested MCID-less spans truncate in `MarkedContentOutputDev` (`:80,:97`); tiling-pattern expansion hazard (31,304 fills vs one callback); DocSlurp `poppler-engine.ts:17-28` collapses layout text; `types.ts:100-114` `scan()` shape; `NativePdfTable` has only bbox plus string rows. I treat these as inputs, not as my results.

### 1.2 Prior HTML review: claims I verified or could not

* **Verified [PROBE]**: `pdftotext -bbox` reports a page `width/height` of 612x792 for a `/Rotate 90` page whose word boxes extend to x=708 (`results`: fixture `xform-3p.pdf` page 2). The page size is not rotated. The recorder emits rotated page size (792x612) in a `page` record.
* **Verified [PROBE]** (one fixture): rotated/mirrored text is shredded by the layout path. `pdftotext` on `xform-3p.pdf` yields fragments ("es", "ty", "d", "txet derorrim" for mirrored text). The run-level IR keeps "thirty degrees", "ninety up" and "mirrored text" intact with start/end points. This justifies run-level capture *before* coalescing.
* **Not verified**: the 71% ICC claim, the Form-DAG DoS numbers, the 80x struct-text claim. My style-cache experiment (§4.3) did not find colour conversion to be a cost in my recorder.

## 2. Framing challenge

| Question | Answer |
|---|---|
| Is the product a "Markdown exporter"? | No. DocSlurp needs tables, figures, formulas, links and provenance for retrieval and proof locators. Markdown is one of several views. The coordinator note that `scan()` already has runs, placements and glyph totals points the same way. |
| Is Poppler's layout engine the base? | No. `TextPage::coalesce` is irreversible and happens at `endPage` (`TextOutputDev.cc:5696`). Use it as an *optional oracle* for reading order, not as the substrate. |
| Does Markdown need the whole display list? | No. It needs text with style, rule/rect geometry, image boxes, link rects, and structure. Clips, shadings and patterns only matter for the visual fallback, and a visual fallback is cheaper by re-rendering the *region* than by replaying a recorded list. |

### Approaches compared

| Approach | Lossless? | Speed | Markdown fidelity | Verdict |
|---|---|---|---|---|
| A. Text-layout only (`pdftotext -layout`, `-bbox-layout`, `-tsv`) | No: no style, rules, images | Fastest (0.1-0.5 s on my samples) | Poor for tables/headings | Keep only as an oracle and as the last-resort fallback |
| B. Post-process `pdftohtml -xml` | No: rules, shapes dropped; link attribution imprecise | Moderate | Medium (has fonts, b/i, links) | **Fastest-to-ship stopgap**, rejected as the architecture. See §6 |
| C. Full display list (record every operator incl. clips/patterns/masks) | Near-lossless | Big IR; pattern expansion hazard | High but wasted | Rejected as default; keep a *per-region* vector/raster fallback |
| D. Event IR + region-adaptive projector (**recommended**) | Lossless w.r.t. the declared event set | ~Interpretation speed | High where regions classify well | **Chosen** |
| E. ML layout (DocLayNet/TATR-style) for every page | N/A | Slow | High on scans | Not on the fast path; only a router for scanned/ambiguous regions. No remote compute used here |
| F. Struct-tree first (tagged PDF) | Only for tagged docs | Fast if done once | High when tags are good | A *source of hints* in the projector. The 10-K here is tagged (`pdfinfo`) but most corpus PDFs are not |

## 3. Proposed architecture

```
PDF --Gfx--> PageEvents (OutputDev subclass, one interpretation)
               text runs | rules/rects/paths | image placements | link rects | MC/MCID | page meta
        --> Page IR (compact columnar, versioned)         <-- the lossless bundle core
        --> Region classifier (text/table/figure/formula/shape/header-footer/unknown)
        --> Projector(profile) --> Markdown (+ HTML/SVG fragments in rich) + manifest.json
        --> Fallback executor (region-scoped): HTML table -> SVG fragment -> cropped raster asset -> text-only
```

Rules of the design:

* **One interpretation per page.** A sink graph, not subprocess composition. Raster is *lazy*, only for regions whose fallback ladder reaches "raster".
* **Capture before grouping.** Runs keep baseline start/end points (direction, rotation), font name, size, colour, rise, bold/italic, and (to add) a per-glyph x-advance array so link-rect boundaries can split a run.
* **Classification is a pure function of IR** so it can be re-run offline against a stored bundle, with new heuristics, without re-opening the PDF.
* **Router signals available for free** from the IR: glyph count per page, share of area covered by images, JPX/JBIG2 presence. These select the scan path (skip text, plan OCR).

### 3.1 Region-adaptive hybrid

| Region | Detector (from IR) | `strict` output | `rich` output |
|---|---|---|---|
| Heading | font size > body x 1.2 (document-level body), short line, optional bold; struct tag if present | `#`..`######` | same |
| Paragraph | line grouping by baseline, running text | text with `**`, `*`, `[a](url)` | same + `<sup>/<sub>` |
| Ruled table | h/v rule grid (stroke "line"/"rule" + thin fills) with text-in-cell assignment | pipe table; header row inferred; spans flattened with a marker comment | `<table>` with `colspan/rowspan`, `<th>` |
| Borderless table | >=3 consecutive lines with equal column count and x alignment | pipe table with "header inferred" marker | same (+ confidence) |
| Formula | symbol font, rise/size delta, stacked baselines | `^{}`/`_{}` and Unicode | `<sup>/<sub>`, optional MathML or an SVG/raster crop for complex layouts |
| Shape/callout | filled rect or non-axis path, not a rule | HTML comment record (no silent drop) | inline `<svg>` fragment |
| Image | `drawImage*` placement bbox + object number | `![alt](assets/pN-objM.png)` | same, optionally data-URI under a size cap |
| Link | link annot rect x run/glyph extents | `[text](uri)` | same |
| Header/footer | repeated text in page margin bands across pages (needs doc-level pass) | drop or `<!-- -->` | optional `<header>/<footer>` |
| Unknown/complex | classifier confidence below threshold | cropped raster asset + OCR/text note | same, optionally SVG |

## 4. Experiments (all out-of-tree; scope labels matter)

Layout of artifacts (everything under `docslurp-review/markdown-20261025/radical/`):

* Fixture generators (stdlib only): `src/make_fixture.py`, `src/make_fixture_xform.py`, `src/make_fixture_span.py`
* Fixtures: `fixtures/synthetic-2p.pdf`, `fixtures/xform-3p.pdf`, `fixtures/span-table-1p.pdf`
* Recorder (C++): `src/recorder.cc` compiled against **system libpoppler 26.10.0** headers, **not** a checkout build: `g++ -std=c++20 -O2 [-DSTYLE_CACHE] [-DNOEMIT] -I/usr/include/poppler recorder.cc -lpoppler`
* Projector prototype (Python): `src/render_md.py events.jsonl strict|rich`
* Results: `results/*.events.jsonl`, `results/*.{strict,rich}.md`, `results/bench-20261025.tsv`, `results/stylecache-cpu-aapl10k.txt`, `results/scan-probe.txt`
* Benchmark driver: `src/bench.sh`

Binaries under test: system `pdftotext/pdftohtml/pdftocairo 26.10.0` (`/usr/bin`) and the recorder above (linked to `libpoppler.so.165`). No checkout build was used or modified.

### 4.1 Correctness of the recorder: coordinator issue fixed and tested

* Original `recorder.cc` mixed coordinate spaces: text was in user space (y=740 for a heading) while paths/images were device space. Fixed by transforming `x,y` with `state->transform` and `dx,dy` with `transformDelta` in `drawChar`, and by emitting `y1`.
* Transformed-text fixture `xform-3p.pdf` (30-degree `Tm`, 90-degree `Tm`, `cm`-flipped scale, `/Rotate 90` page, mirrored `Tm`) now yields correct device coordinates. Example: the heading is now at y=52, and the 30-degree run has start (100,192) and end (162.96,155.65). Mirrored text is at x from 300 to 232.66. The `page` record carries rotation and rotated dimensions. See `results/xform-3p.events.jsonl`.
* Residual scope limits **[FINDING, not solved]**: no clip tracking; no Type3 interpretation (`interpretType3Chars` false); no ActualText/marked-content use; no pattern/shading handling; no soft-mask handling; fill-colour only for text; non-axis strokes labelled `poly`; images are placements only and the stream is never consumed, so no inline-image decode correctness claim is made.

### 4.2 Fidelity on the synthetic fixtures (`strict` vs `rich`)

* `synthetic-2p.pdf`: headings (`#`, `##`), bold/italic, link attribution at run level, ruled table, borderless table (header "inferred" marker), image placement with object id, filled rect, superscript, Symbol-font Greek, second-page heading using a document-level body size. `results/synthetic-2p.{strict,rich}.md`.
* `span-table-1p.pdf`: header cell spanning two columns via an omitted inner rule. `strict` flattens (`| Region | Revenue |  |` + marker comment); `rich` emits `<th colspan=2>Revenue</th>`. `pdftotext -layout` gives `Region   Revenue` / `North    10.5   12.0`, which cannot express the span at all.
* `pdftohtml -xml -i` on the same fixture **[FINDING, single fixture]**: wrapped `" spans, plus a "` in `<a href>` though the link rect starts at x=400 where the linked run begins. It offers no rules, shapes or images, and collapses "Helvetica-Bold" into family "Helvetica" plus `<b>`.
* **Bug found in the prototype**: Base-14 non-embedded fonts report `isBold/isItalic` = 0 even for "Helvetica-Bold". The projector falls back to name matching (`bold|black|heavy`, `italic|oblique`). A real device should expose `GfxFont::getWeight()` and the font name together, and the IR should record both the descriptor flags and the name.
* **Prototype limitations (not production-safe)**: `rich` mode does **not** HTML-escape extracted text. In production every text node in HTML/SVG context must be escaped, and attribute values sanitised, because PDF text is untrusted. `strict` escapes only `\ * _ |`, not the whole CommonMark set. Header detection is "first ruled row" or synthesised. The classifier is heuristics on three tiny fixtures; **no accuracy claim is made for real documents.**

### 4.3 Performance probes (scoped; system under concurrent load)

Load average was 11-16 on 8 cores during these runs, because other council members were running. Wall times are noisy. User CPU is more stable.

**Scope warning:** the rows are not equivalent work. `pdftotext` builds reading-order text. The recorder `null` mode only runs interpretation with no output. `text` mode emits per-run JSON. `full` mode additionally enables non-text interpretation (needNonText) and emits paths/images/links. No row is a complete Markdown exporter.

`results/stylecache-cpu-aapl10k.txt`, 80-page tagged 10-K (3 runs each, user CPU seconds):

| Run | user CPU |
|---|---|
| recorder `null` (interpret only) | 0.24-0.30 |
| recorder `text` (JSON per run) | 0.94-1.09 |
| recorder `text`, emission compiled out (`-DNOEMIT`) | 0.24-0.26 |
| recorder `full` | 0.98-1.01 |
| system `pdftotext` (text) | 0.34 |
| system `pdftotext -bbox-layout` | 0.51 |

* **[FINDING]** Interpretation plus run capture costs about the same as `pdftotext` (0.25 vs 0.34 s CPU). The jump to ~1.0 s is entirely my serialiser: this PDF emits **one run per glyph** (271,704 runs for 271,704 chars). A real recorder must coalesce adjacent same-style glyphs in memory and write a binary columnar IR.
* **[FINDING]** Caching colour/font conversion (`-DSTYLE_CACHE`) made no measurable difference, so I do not claim ICC or colour conversion as a bottleneck for this recorder.
* `results/bench-20261025.tsv` (5-run medians of wall ms, pre-fix recorder with the coordinate bug; interpretation cost is unaffected). 10-K: pdftotext 545, `-layout` 748, `-bbox-layout` 1112, `pdftohtml -xml -i` 1498, `pdftocairo -svg` 7444 (shown only to size the "render everything" cost). The run was killed during the scanned-document rows because `pdftohtml -xml` (27-35 s) then `pdftocairo` dominated; use `results/scan-probe.txt` for the scan.

`results/scan-probe.txt`, 26-page scanned PDF (JPX + JBIG2 layers, 52 image placements), user CPU:

| Run | user CPU |
|---|---|
| recorder `null` | 0.04 s |
| recorder `text` | 0.05 s |
| recorder `full` | **6.7 s** (15 s wall in-process) |
| `pdftotext` | 0.04 s |

* **[FINDING]** Enabling non-text interpretation makes `Gfx` do roughly 170x more CPU work on this scan, even though my `drawImage*` overrides never touch the stream. `Gfx::doImage` calls `str->getImageParams()` before any device callback (`Gfx.cc:4223`), and `JPXStream::getImageParams` runs `init()` (`JPEG2000Stream.cc:161-165`).
* **[SPECULATION, unprofiled]** The cost is JPX decode inside `init()`. This is consistent with the prior review's "full JPX decode for params" claim, but I did not profile it (no `perf` on this machine). **Needed experiment on a checkout build:** instrument `doImage` to time `getImageParams`, then test a header-only parameter path.
* Design consequence: a scan router must decide *before* enabling image callbacks, so it needs a cheap pre-pass (page resources, `/Filter`, image count, glyph count from a `text`-mode pass at ~0.04 s) and an interpretation profile that skips `getImageParams`. Today that decision is not expressible in `OutputDev`; it needs a profile-style hook at the `Gfx` level (a checkout change, not done here).

## 5. CLI proposal

Name: **`pdftomd`** (new utility under `utils/`; no change to existing tools).

```
pdftomd [options] file.pdf [out.md]
  -f N -l N                  page range
  --profile strict|rich      strict: CommonMark+GFM only (default). rich: may embed HTML tables, <sup>/<sub>, inline <svg>
  --bundle DIR               write lossless bundle (IR, assets, manifest) in addition to Markdown
  --assets external|inline|none   images and crops: files in DIR/assets, data URIs up to --inline-max KB, or placeholders
  --inline-max KB            default 64
  --tables auto|ruled|text|off
  --math unicode|html|mathml|raster-fallback
  --shapes drop|comment|svg  (rich: svg; strict: comment)
  --fallback none|svg|raster  region fallback ceiling
  --scan-policy mark|skip|ocr-request   what to do for pages with few glyphs and large images
  --max-bytes N --max-ops N  per-page budgets (partial results marked in manifest)
  --json-status              machine-readable per-page/region statuses to stdout/stderr file
  --no-header-footer         drop repeated margin text (needs a doc-level pass)
```

Exit codes: 0 ok; 1 open/decrypt failure; 2 partial (budget or unsupported features hit; manifest says which); 3 usage.

Stability: `manifest.json` carries `schema_version`, `poppler_version`, `tool_version`, `profile`, `page[].{w,h,rot}`, `region[].{id,kind,bbox,status,confidence,source_events[],asset?}`, and `features_unsupported` counters per page.

## 6. Fastest path and the cost of quality

| Tier | What ships | Cost | Evidence |
|---|---|---|---|
| T0 (days) | `pdftohtml -xml -i` post-processor: heading by size, b/i, links; no rules/shapes/images | Lowest effort; ceiling is low (no table rules; fixture showed link slop; 30 s measured on one scan under load) | §4.2, `bench-20261025.tsv` |
| T1 (weeks) | Out-of-tree recorder as in this report (private libpoppler API, version-pinned), coalescing + binary IR, region projector, strict+rich | Interpretation at roughly `pdftotext` CPU; the quality work is the classifier | §4.3 |
| T2 | In-tree `pdftomd` using checkout `PagePass` ideas: InterpretProfile, marked-content stack in `Gfx`, per-document font cache | Needed for tagged-PDF correctness and for scan speed (skip `getImageParams`) | `coordinator/README.md` callback findings; §4.3 scan row |
| T3 | Optional raster/ML routing for scans and low-confidence regions | Slow; opt-in; not measured here (no remote compute used) | not measured |

Cost of quality, by region (qualitative; **[SPECULATION]** except where measured): ruled tables and links are cheap once IR exists; borderless tables, formulas and header/footer need doc-level stats and have the highest false-positive risk; shapes/figures cost raster or SVG output and pattern handling; scanned content cost is dominated by image decode, as measured.

## 7. Fidelity and fallback contract

### 7.1 Profiles and what each can promise

| Dimension | `strict` | `rich` | `bundle` (adds to either) |
|---|---|---|---|
| Renderer compatibility | CommonMark + GFM pipe tables. Safe in GitHub, most chat UIs, DocSlurp indexers | Needs a renderer that supports inline HTML (and SVG). Not guaranteed in every sanitising viewer | n/a |
| Editable semantics | Tables as GFM; no spans. Edits stay in Markdown | HTML tables with spans are semantic but harder to edit; `<sup>` etc. | IR is the source of truth for re-projection |
| Visual preservation | Low to medium; shapes only as comments; complex regions as images | Medium to high: shapes as SVG, merged cells, raster crop if needed | Highest short of the PDF |
| Assets | `external` default; `inline` base64 optional with a size cap | Same; SVG may be inline | Assets in `assets/` with content hashes |
| Failure mode | Explicit markers: `<!-- strict: spans flattened -->`, `<!-- borderless table: header row inferred -->`, `<!-- shape ... omitted -->` | Falls back down the ladder per region | Manifest lists every degraded region |

### 7.2 Fallback ladder (per region, never per document)

pipe table -> HTML table (rich) -> SVG fragment (rich) -> cropped raster asset -> text-only with `status: degraded`.

* Every step is recorded in the manifest, including the reason (`spans`, `rotation`, `curve/pattern`, `low-confidence`).
* A raster crop is produced by re-rendering only the region (a bounded `pdftocairo`/Splash page-slice call), not by replaying the IR.
* **Limitations that remain honest**: inline SVG here is a *constructed approximation* of fills and rules, not a faithful vector replay of the page. Pattern fills, shadings, blend modes, soft masks, and clips are not preserved by it. Fidelity for those regions is the raster fallback only.

### 7.3 Lossless bundle contract (what "lossless" means)

* Lossless **with respect to the recorded event set**, not the PDF. The bundle lets you regenerate any Markdown profile and the DocSlurp `scan()` structures without re-opening the PDF.
* Contents (v1): `manifest.json` (schema, versions, per-page and per-region status), `pages.ir` (binary columnar runs, rules/paths with bbox and class, image placements with object id and transform, link rects, page boxes and rotation), `fonts.json` (name, descriptor flags, weight, base-14 flag, embedded flag), `assets/` (extracted images, crops), `doc.md`.
* **Not captured in v1** (declared in `features_unsupported`): clips, shadings, patterns, soft masks, blend modes, Type3 glyph shapes, annotations beyond links, optional-content visibility, per-glyph advances (to add), marked-content/struct ownership (to add; blocked by the named-property BDC issue in the coordinator findings).
* So the bundle is not a replacement for the PDF. It is a stable evidence layer that records what it dropped.

### 7.4 Compatibility breaks (named)

1. **New tool and schema**, versioned; no changes to existing CLI outputs. Consumers must opt in.
2. **Coordinates**: the IR uses device space with y down, rotation applied, and an explicit rotated page size. This differs from `pdftotext -bbox`, whose page size ignores `/Rotate` (verified).
3. **Run-level text, not word-level**: strings are not re-split by `TextPage`. Reading-order text from `pdftotext` and IR order can differ; the manifest names which engine ordered each region.
4. **Private libpoppler API** is used for T1: it can change between Poppler releases; pin and gate on `poppler_version`.
5. **Strict output can differ from `pdftotext -layout`** on tables and rotated text by design.
6. DocSlurp: `NativePdfTable` (bbox + string rows) is insufficient for spans or provenance; `layoutTextToMarkdown` whitespace collapsing (`poppler-engine.ts:17-28`, per coordinator) loses table alignment. Both would need new typed fields. I did not read the DocSlurp repo myself.

## 8. Rejected or demoted

* Rebuilding Markdown by regex over `pdftotext -layout` text: loses rules, style, rotation, link geometry.
* Whole-page rasterisation as the default (`pdftocairo -svg` 7.4 s vs 0.5 s text on the 10-K; scan cost also large).
* Full display-list recording as the primary IR: size, pattern expansion hazard, little Markdown value.
* ML layout for every page: speed and cost; no remote compute used; kept only as an optional router.
* A single Markdown dialect for everything: forces either silent loss or non-portable output; profiles make the choice explicit.

## 9. Open questions and next experiments (need a checkout build or more fixtures)

1. Profile `getImageParams` in `doImage` on the scan; try header-only JPX parameter reading. (Needs checkout build.)
2. Add glyph-advance arrays and marked-content ownership to the recorder; fix named-property BDC at the `Gfx` seam; re-run on the coordinator's tagged fixtures.
3. Replace JSON with a coalesced binary IR and re-measure; the 0.7 s serialiser overhead is a lower bound on what I wasted, not a final number.
4. Evaluate the classifier on real tables with rules and with merged cells, e.g. SEC 10-K statements. I only ran synthetic fixtures; no accuracy claim.
5. A rotated-table fixture and RTL fixtures.
6. Compare my table output to `pdftotext -layout` and to the DocSlurp ground truth (if available) on the same pages.

## 10. Reproduce

```sh
R=docslurp-review/markdown-20261025/radical
export TMPDIR=${TMPDIR:-/home/dan/tmp/agents}; B=$(mktemp -d "$TMPDIR/radical-build.XXXXXX")
g++ -std=c++20 -O2 -DSTYLE_CACHE -I/usr/include/poppler $R/src/recorder.cc -o $B/rec -lpoppler
python3 -I $R/src/make_fixture.py $B/s.pdf && $B/rec full $B/s.pdf $B/s.jsonl
python3 -I $R/src/render_md.py $B/s.jsonl rich
```
