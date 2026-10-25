# Architect report: Markdown export for DocSlurp from Poppler

Persona: Architect (module boundaries, evidence/IR ownership, single-pass multi-output contract, semantic tags vs geometry, incremental migration).
Date 2026-10-25. Checkout `/home/dan/code/oss/poppler` @ `e658c5562` (version 26.10.90, **no built binaries**). All runtime experiments use the **system** poppler 26.10.0 (`/usr/bin/pdftotext`, `/usr/lib/libpoppler.so.165`, private headers in `/usr/include/poppler`). No tracked file was modified. No subagents. Prior review `architecture-review-20261025-123158.html` was checked, not trusted (see §9). Coordinator notes in `../coordinator/README.md` were read after my probes; they agree with my findings and add the pattern-fill and nested-Span results (not re-run here).

Labels: **[F]** = finding reproduced here or read in source. **[S]** = speculation / estimate, not measured.

---

## 1. Answer to the persona question

> Which architecture gives fidelity without coupling extraction to Markdown?

**Evidence-first, Markdown-last.** Poppler owns one *page interpretation* that produces **PageEvidence** (a typed, serialisable IR). Markdown is one *projection* of that IR, in a separate module with its own tests and profiles, with no Poppler types in its signature. Three rules:

1. **Poppler records; it does not decide Markdown.** It emits words, per-char style provenance, paths, image placements, link quads, marked-content frames, and the structure-tree leaves. It does not emit `#`, `|`, `**`.
2. **Tags and geometry are two independent evidence channels, joined by a key, never merged into one guess.** The join key is `(page, MCID)` per glyph run. A projector chooses: tag-driven when the join is trustworthy, geometry-driven otherwise, and records which one it used per block.
3. **One interpretation per page fans out to many sinks** (text, shapes, images, struct join). Rendering to a bitmap is an optional later sink, not a prerequisite.

This is the same seam the prior review calls "StructuredPage + PagePass + SemanticPage". My contribution is that it is **already demonstrable from outside tracked code**: `src/evidence_probe.cc` is a ~190-line `TextOutputDev` subclass against the installed library that produces the joined IR in one pass (§5).

## 2. Module boundaries

```
PDF ──► [A] PageEvidence producer (C++, in libpoppler / a new util)
          single Gfx pass per page, sinks: TextPage, MarkedContentFrames,
          PathSink, ImageSink, LinkSink;  + once-per-doc: Metadata, StructTree walk, Outline
        │  IR = versioned JSONL / FlatBuffers  (the ONLY contract)
        ▼
      [B] Projector(s)  (no Poppler dependency; any language)
          ├─ markdown/gfm      portable, lossy, deterministic
          ├─ markdown/rich     GFM + optional HTML tables / <sup>/<span style> / inline SVG
          ├─ chunks            DocSlurp chunker input (blocks with provenance)
          └─ json/tsv          debugging / golden tests
        │
        ▼
      [C] Asset writer (optional): images by xref id, SVG files, page crops (lazy raster)
```

* **[A] owns facts**: coordinates in page space (y-down, /Rotate normalised), char→Unicode provenance, MCID per glyph run, path geometry + colour + line width, image CTM quad + xref id, link quads + action.
* **[B] owns judgement**: heading levels, list detection, table reconstruction (struct-driven or ruling-driven), reading order selection, emphasis merging, de-hyphenation, dropping Artifacts. Judgement is cheap to iterate when it is outside C++ and has no build cycle.
* **[C] owns bytes**: it is the only place that decodes/encodes pixels. The Markdown refers to assets by stable id (`assets/p{page}-img{xref}.png`), so Markdown generation never blocks on image decoding.

Why not "put it in `pdftotext` as `-md`": that fuses A and B in one C++ file, and every heading-threshold change becomes a Poppler release. That is the failure mode of `pdftohtml` (`utils/HtmlOutputDev.cc`, 1758 lines, its own engine separate from `TextPage`). [F]

## 3. Proposed CLI

New tool, same repo, thin: `pdftoevidence` (A) and `pdftomd` (A+B convenience wrapper). Names are placeholders.

```
pdftoevidence [-f N] [-l N] [-opw/-upw] [--format jsonl|fb]
              [--with text,paths,images,links,struct]    # default: all; "text" alone = today's cost
              [--page-chunk-bytes N]                      # stream per page, bounded memory
              [--assets-dir DIR]                          # only if images requested
              file.pdf [out|-]

pdftomd       [-f N] [-l N] [--profile gfm|rich] [--assets inline|dir|none]
              [--svg none|inline|file] [--tables auto|struct|ruling|text]
              [--headings auto|struct|size] [--provenance none|comments|attrs]
              [--max-fallback-ratio R] [--evidence-out FILE.jsonl] file.pdf [out.md|-]
```

* `--evidence-out` is the contract: the same run that writes Markdown can also write the IR (DocSlurp ingestion then gets Markdown + chunks + images from one parse; matches `worker/src/extractors/local-artifact-adapters.ts` demand per the coordinator).
* `pdftomd` = `pdftoevidence | projector` in one process for speed; the projector is a library so there is no pipe cost in production, and a pipe in tests.
* `--with` is the cost dial: each extra sink costs an interpretation feature (see §6, `needNonText`).

## 4. IR (PageEvidence) — minimum fields the projectors needed

Record kinds exactly as emitted by `evidence_probe` (JSONL, one record per line; `results/*.evidence.jsonl`):

| record | fields | source in Poppler |
|---|---|---|
| `doc` | pages, tagged, producer | `Catalog::getStructTreeRoot` |
| `meta` | Title/Author/Subject/Keywords/Creator/Producer/dates | `PDFDoc::getDocInfoStringEntry` |
| `struct` | page, mcid, path (`Document/Table/TR/TD`), row, col | one walk of `StructElement` tree, **no page re-render** |
| `page` | n, w, h, mcBegin/mcEnd/mcUnderflow | counters (diagnostic) |
| `block`/`line` | ids, bbox; words[] | `TextFlow→TextBlock→TextLine→TextWord` |
| word | text, bbox, baseline, font size, rot, font name, flags b/i/mono/serif, underline, space_after, colour, link URI, `mc` (MCID), `art`, `cp` | `TextWord` getters (`TextOutputDev.h:181-216`) + frame join |
| `path` | fill/stroke, bbox, nPoints, curve, colour, line width, `mc` | `OutputDev::fill/eoFill/stroke` + `GfxState::getPath` |
| `image` | kind, bbox in page space, w, h, xref id, inline?, `mc` | `OutputDev::drawImage*` |
| `link` | rect, URI | `Page::getAnnots` (Link annots) |

Fields still missing (need Poppler change, or a lossy substitute): line-level baseline/rotation and writing mode (private, `TextOutputDev.h:350,440,224`); `invisible` text accessor (`TextOutputDev.h:247`, set at `.cc:428`, no getter); per-char font within a word (only `getFontInfo(idx)`, which I used for idx 0 only); `TextBlock::tableId` is private (`.h:446`); clip paths; a MCID on `TextWord` directly (I derive it, §5).

## 5. Experiment 1 — single-pass evidence probe (out-of-tree) [F]

Files: `src/evidence_probe.cc` (probe), `src/mkfixtures.py` (hand-written PDFs), `src/md.py` (projector), `src/bench.py`, `src/stats.py`. Fixtures: `fixtures/{tagged,untagged,tagged_inline_only}.pdf` (H1, paragraph, list, 3x3 ruled table with TH/TD tags, link annotation, XObject image, red rectangle, superscript, Courier-Oblique run). Build: `g++ -std=c++20 -O2 src/evidence_probe.cc $(pkg-config --cflags --libs poppler) -I/usr/include/poppler -o $TMPDIR/evidence_probe`.

How the join works without touching Poppler: the probe subclasses `TextOutputDev`, mirrors `TextPage::charPos` (incremented by `nBytes` per `drawChar` at `TextOutputDev.cc:2662-2779`, and by `incCharCount` at `.cc:2782`; `needCharCount()` is true for `TextOutputDev`, `.h` `needCharCount() override { return true; }`), records `(charPos → current MCID/Artifact)` run-lengths from its own `beginMarkedContent/endMarkedContent` stack, and after the page ends looks up `word->getCharPos()` (`TextOutputDev.h:208`). Struct leaves come from a single recursive walk of `StructElement` (public API) with row/col counters derived from `TR`/`TH`/`TD` ordering.

Result on the 80-page tagged AAPL 10-K (`results/aapl.struct-join-stats.json`): 42,111 words, 41,560 carry an MCID, **41,527 (98.6%) join to a struct leaf**, 3,758 struct leaves (TD 1,853, P 1,221, LBody 301, Link 242, Lbl 133). One interpretation per page; struct text was **not** obtained via `StructElement::getText` (which does a full `displayPages` per MCID leaf: `StructElement.cc:819-869`; the prior review reports 2,320 interpretations / 10.26 s on a 20-page file, I did not re-run that).

Why this matters architecturally: it shows the seam. The join belongs inside Gfx/TextOutputDev (so every consumer gets it), but today it must be reconstructed from callbacks, and the reconstruction is only as good as the callbacks (next).

## 6. Experiment 2 — callback defects that break the "tags ↔ geometry" join [F]

**6a. Named property lists silently lose MCIDs.** `Gfx::opBeginMarkedContent` calls `out->beginMarkedContent` only if `args[1].isDict()` (`poppler/Gfx.cc:5166-5169`); the common `/P /MC1 BDC` form (properties via resource dictionary) is a Name, so no begin; but `opEndMarkedContent` always calls `out->endMarkedContent` (`Gfx.cc:5173-5195`). Fixture `tagged.pdf` (one paragraph via named list) vs `tagged_inline_only.pdf` (same, inline dict), system 26.10.0:

```
tagged:             pdfinfo -struct-text  P → ""                      probe: mcBegin=13 mcEnd=14 underflow=1
tagged_inline_only: pdfinfo -struct-text  P → "Revenue grew 12% ..."  probe: mcBegin=14 mcEnd=14 underflow=0
```
(`results/named_property_list_gap.txt`). Real-world prevalence: **80 of 80 pages of the AAPL 10-K show end-without-begin** (`mcUnderflow>0`, from `/OC` and/or named lists). Joined still reached 98.6% because most MCIDs there are inline, but any devices that keep a stack (`MarkedContentOutputDev.cc:80-102`, my probe) will pop the wrong frame after a miss; a misjoin cannot be detected downstream.

**6b. Images and some paths are not delivered to text-only devices.** `Gfx.cc:1937, 1964, 1991, 4155, 4340, 4668` gate on `out->needNonText()`; `TextOutputDev::needNonText()` returns false. My probe overrides it to true. On the fixture: 9 path records with `EV_NONTEXT=0` vs 10 with `=1` (the extra one is the image). So fills/strokes still arrive (useful for rulings), images do not. Cost is in §7.

**6c. Link attach needs `setInlineLinkURIs(true)` (or `enableHTMLExtras`).** Without it `word->getLink()` was empty; with it the fixture's `docs` word carries `https://docslurp.io/` (`TextOutputDev.cc:5696-5702, 5843`). Link attachment is bbox-rect based, not QuadPoints-accurate. [F source, F fixture]

**6d. Font style flags are not reliable for non-embedded base-14 fonts.** `Helvetica-Bold` and `Courier-Oblique` in the fixture report `b=0,i=0` (and `Courier-Oblique` `m=0,serif=1`) from `TextFontInfo::isBold/isItalic/isFixedWidth` (descriptor flags, `TextOutputDev.cc:380`). The projector must fall back to font-name regex (`md.py: BOLD/ITAL`). [F on fixtures; embedded-font behaviour not tested here]

## 7. Experiment 3 — cost of the extra evidence [F, noisy]

`results/bench.txt`, best/median of 3, machine not quiesced (medians vary up to 2x; treat as order of magnitude). System binaries vs out-of-tree probe linked to the same library (26.10.0):

| AAPL 10-K, 80 pp, 1.86 MB | best s | median s |
|---|---|---|
| `pdftotext -raw` | 0.72 | 1.52 |
| `pdftotext -layout` | 0.75 | 1.21 |
| `pdftotext -bbox-layout` | 2.43 | 2.46 |
| `pdftohtml -xml` | 2.70 | 2.84 |
| probe, text-only (needNonText=0), full JSON out (10.7 MB) | 1.36 | 2.00 |
| probe, + images/paths (needNonText=1) | 1.48 | 2.29 |
| projector (Python, evidence→gfm / rich) | 3.15 / 2.36 | |

USCODE title 1 (21 pp, 0.2 MB, untagged): -layout 0.32, -bbox-layout 0.67, probe 0.47-0.50. (`pdftohtml -xml` 0.11 s there is odd; it emitted 404 KB so it did run; I did not investigate.)

Reading: the evidence pass is ~1.8x `pdftotext -layout` but ~0.55x `-bbox-layout`/`pdftohtml -xml`, and includes struct join + shapes + links the others lack. JSON emission and the Python projector are not optimised; **the projector is currently slower than the extraction** (3 s) — a prototype artifact, not a design property. Do not read these as a product benchmark. The prior claim that `-bbox-layout` is much slower than `-layout` reproduced (3.2x on AAPL) [F]; I did not find its cause.

## 8. Experiment 4 — two Markdown profiles from the same IR [F]

`src/md.py --profile gfm|rich`, outputs `results/tagged.gfm.md`, `results/tagged.rich.md`, `results/untagged.gfm.md`, plus full-document outputs for AAPL and USCODE.

Fixture, GFM profile: H1 → `# Quarterly Report`; link → `[*`docs`*](https://docslurp.io/)`; bullets; ruled table → GFM pipe table (header bold); red rectangle → HTML comment recording it was dropped; image → `![image 2x2](assets/p1-img8.png)` + bbox comment. Rich profile: table → `<table data-ds-source="ruling-grid" data-ds-confidence="0.9">`, rectangle → inline `<svg viewBox=... ><rect/></svg>`.

Honest defects visible in the output (kept, not hidden): (1) superscript `2` landed on a separate TextLine and was joined as ` 2 E = mc`, i.e. the projector needs a sub/superscript pre-merge using baseline+size; Poppler gave a separate line. (2) in the rich table Markdown `**` leaks into HTML cells (needs a profile-aware inline renderer). (3) On AAPL the geometric heading rule (`size ≥ 1.25x body`) turned checkbox glyphs (`☒`, `☐`) and short fragments into `##` headings (19 `#` lines, many wrong). Struct role should win when the join is good; I did not implement struct-driven headings/tables in `md.py` (only the evidence for them), so the AAPL tagged advantage is **unrealised in the prototype**. (4) Reading order is `y0` sort of lines, not Poppler's block order — multi-column pages will interleave. 82 pipe-table lines emitted for AAPL were not manually audited.

## 9. Prior review claims: verified / not verified

Verified by me in source: marked-content begin conditional / end unconditional (`Gfx.cc:5166-5190`); `needNonText` gating (lines above); `StructElement::appendSubTreeText` re-interprets pages (`StructElement.cc:819-869`); `hasSpaceAfter`/`getBaseline`/`getLink` accessors exist but line baseline/rotation are private; `invisible` has no getter. Reproduced by me: empty struct text for named property list; `-bbox-layout` slower than `-layout`. **Not verified (inherited, do not rely):** the ~80x struct-text slowdown, O(n^2.8) reading order, JPX 7 s, 66 MB font inflation, XRef parallelism claims, speed-up estimates in "After" panels (they are labelled estimates in the HTML).

## 10. Fidelity / fallback contract

Every block in the IR/Markdown carries *how it was derived* and *how much to trust it*. Proposed `source` and `confidence` per block (surfaced as HTML comments in gfm, `data-ds-*` attributes in rich, fields in chunks):

| block kind | preferred derivation | fallback 1 | fallback 2 | when to fall back |
|---|---|---|---|---|
| paragraph/text | tag `P` + joined glyph runs | geometry line/block merge | raw content order | join ratio < threshold or struct untrustworthy (`/MarkInfo Suspects`) |
| heading | tag `H1..H6` (+ `/Lbl`) | size ratio vs body mode, bold, short line, not a checkbox/bullet glyph | none (emit paragraph) | no tag; gate geometric rule on min length and glyph class |
| list | `L/LI/Lbl/LBody` | bullet/number glyph prefix + indent | paragraph | no tags |
| table | `Table/TR/TH/TD` + `RowSpan/ColSpan` attrs | ruling grid (axis-aligned strokes/rect fills) | whitespace-column alignment | tags absent/incomplete; rulings < 2x2 |
| image | `drawImage` placement + xref id + `/Alt` | XObject listing | none | needNonText off → `images: unavailable` flag, not silent |
| shape/vector | path records → SVG (rich) | bbox comment (gfm) | omit w/ count | curves/clips/patterns beyond budget |
| link | link annot rect ∩ word boxes (QuadPoints later) | rect only | text only | no URI action |
| math | ActualText / ToUnicode text + sup/sub by baseline | rich: `<sup>/<sub>`; both: keep Unicode | raster crop reference | outlined glyphs / Type3 / no mapping (provenance `unmapped`) |
| metadata | Info dict (+ XMP not implemented here) | none | none | n/a |

Global contract: the writer never silently drops. If a sink was off, a feature failed, or a budget was hit, the document front matter lists `unsupported: [...]`, per-page `partial: true`, and a `fallback_ratio` (blocks derived by geometry fallback ÷ total). Callers can fail closed with `--max-fallback-ratio`. Rendering for OCR or visual preservation is **opt-in, per page**, triggered by evidence (e.g. glyph coverage zero, invisible-text layer, unmapped-glyph ratio) rather than always.

## 11. Rich profile: optional HTML and SVG fragments (user steering)

The rich profile is explicitly **non-standard Markdown**: GFM remains the baseline, HTML/SVG are additive and must degrade. Separate axes, decided independently per feature:

**Axis 1 — renderer compatibility.** GFM tables, `<sup>`, `<sub>`, `<img>`, `<a>`, `<details>` survive GitHub/GitLab/most renderers. Inline `<svg>`, `<table colspan/rowspan>`, `style=` attributes are stripped or sanitised by many (GitHub strips `style` and inline SVG; typical sanitisers allow `colspan/rowspan`). **Contract:** profile `rich` declares `html: true, svg: inline|file`, and every HTML fragment is followed (or preceded) by nothing essential: the text content must be present in the fragment itself so a sanitiser that strips tags still yields readable text. SVG is emitted as `![](assets/p1-shape3.svg)` (file mode, widely supported) by default, `inline` only when the consumer declares it.

**Axis 2 — editable semantics vs visual preservation.** Two different goals; do not mix in one fragment:
* *Semantic* (default for tables): `<table>` with `<th scope>`, `colspan/rowspan`, no absolute positioning; text is editable, reflows. Merged cells exist only here (GFM cannot represent them; the GFM profile must either duplicate the text across spans or flatten, and record `lossy: merged-cells`).
* *Visual*: SVG/`<img>` of the region (figure, chart, equation raster) with text preserved alongside as `<title>`/alt/`<text>`. Not reflowable, not editable, but faithful. Used for vector charts, shapes, outlined text.
Selecting `--tables auto` picks semantic when the table is tag- or ruling-derived with confidence ≥ threshold, else a visual fallback (crop + extracted text).

**Axis 3 — external assets vs inline.** `--assets none|dir|inline`: `dir` writes `assets/p{n}-{kind}{id}.{png|svg}` (stable ids from xref, enabling dedupe across pages); `inline` uses `data:` URIs (single file; size blow-up; many renderers block). Markdown text refers to assets only by relative id, so the projector never needs pixel data; the asset writer [C] can run lazily/in parallel.

**Axis 4 — faithful fallback limitations (what rich still cannot promise).** Tilings/shadings are not reconstructed as vectors (coordinator measured 31,304 fill callbacks for one repeated tiling pattern; intercepting the pattern is a Poppler-side change, `Gfx.cc:1937`); clip paths and soft masks are not carried in the IR; text rendered as outlines or Type3 has no Unicode; rotated tables are not handled; styles are lossy (colour, bold/italic only; no letter-spacing/kerning). Anything beyond a threshold becomes a *cropped image reference*, with the extracted text as alt/adjacent text, and is counted in `fallback_ratio`.

In the prototype, `--profile rich` demonstrates only: HTML table with provenance attributes and an inline `<svg><rect/></svg>` from a path record. Colspan/rowspan and `<sup>` code paths exist in the design but were not exercised by a fixture.

## 12. Fastest path and cost of quality

Ordered by (value ÷ cost). None requires interpretation beyond the existing single pass.

1. **Now, no Poppler change (days):** ship `pdftoevidence` as an out-of-tree/vendored tool like `evidence_probe.cc` (links the system library) + projector in TS/Rust. Cost ~1.8x `pdftotext -layout` on the measured doc, 98.6% struct join on tagged input. Residual: named-property gap (6a) and nested-Span truncation (coordinator) remain.
2. **Core patch 1 (small, high value; not applied, needs a build to validate):** in `Gfx::opBeginMarkedContent` resolve a Name second operand through `res->lookupMarkedContentNF` (the code already does this for `Span`, `Gfx.cc:5138-5144`) and call `out->beginMarkedContent` with that Dict; call begin for every BDC so begin/end are balanced. Experiment needed: build checkout in a private build dir, rerun `named_property_list_gap` expecting `mcBegin==mcEnd` and non-empty struct text.
3. **Core patch 2:** `TextWord` stores its frame id (4 B in `CharInfo`) set from Gfx's stack; remove my charPos-mirror hack. Expose `invisible`, line baseline/rot.
4. **Quality layer in [B]:** struct-driven tables/headings; sup/sub pre-merge; ruling→cell grid with row/col span detection; list nesting; order via Poppler blocks instead of y-sort. This is where most remaining fidelity cost lives and it iterates without C++ builds.
5. **Opt-in cost:** images need `needNonText=true` (adds more than the measured 8% only on image-heavy or pattern-heavy pages; pattern case can be 480x per coordinator: 290 ms vs 0.6 ms); gate by `--with images` and by a budget.
6. **Defer:** raster sink / page crops / OCR routing (prior review items 5, 6); parallel pages (needs XRef immutability; not tested here).

Cost-of-quality estimate [S]: items 1-2 recover most tagged-document fidelity at <2x text-extraction cost; items 4 are engineering weeks, not CPU; rich/visual fallbacks are the only part that spends raster/CPU and should be page-budgeted.

## 13. Incremental migration

1. Freeze IR v0 (the table in §4) and golden JSONL for the fixtures + 3 corpus PDFs.
2. Land patch 1 in Poppler; add a fixture-based regression (named property list) under `test/`.
3. Move the probe into `utils/` as `pdftoevidence`, built when `ENABLE_UTILS` is on; keep it additive (no change to existing tools).
4. Reimplement projector in DocSlurp's language; run both profiles against evidence goldens; track `fallback_ratio` as a quality metric.
5. Only then consider folding `pdftotext -tsv/-bbox-layout` and glib/qt text walks onto the IR (prior review item 4); not needed for Markdown.

## 14. Competing approaches rejected

* **`pdftohtml -xml` → Markdown.** Gives font specs, colours, links, but it is a second layout engine (`HtmlOutputDev.cc`), loses MCIDs and shapes, measured 2.7 s vs 1.4 s (AAPL, best). Usable as a *fallback comparison*, not the core. [F measurement; claims about missing MCIDs by source reading]
* **`pdftotext -bbox-layout` (+ Python).** No fonts, no styles, no links, no paths; 2.4 s; fixing requires the same internals I use.
* **`pdftotext -layout` → Markdown (what DocSlurp does today per coordinator).** Fastest, but destroys alignment info and has no styles/links/tables; fine as a last-resort fallback.
* **`StructElement::getText` per leaf for tagged PDFs.** Re-interprets pages per MCID (`StructElement.cc:819-869`); my single walk + join avoids it entirely.
* **`pdftocairo -svg`/page-level SVG as primary output.** Faithful visually but not semantic; text becomes glyph paths unless text mode is kept; large. Kept only as an optional visual fallback sink [S, not measured].
* **ML layout (ONNX models under `~/.cache/docslurp/layout-onnx-runtime`).** Out of scope for "extremely fast" and for this checkout; viable as an optional per-page escalation when `fallback_ratio` is high [S, not tested].
* **One giant C++ `MarkdownOutputDev`.** Couples heuristics to Poppler release cycle and to C++ build; rejected by the boundary argument.

## 15. Limitations of this review

* The checkout was not built; all runtime numbers use the system 26.10.0 library, while source line numbers refer to `e658c5562` (26.10.90). Line numbers cited as verified were read in the checkout; behaviour was observed on the system build. They match for the paths I exercised but this was not proven in general.
* Timings are single-machine, un-quiesced, 3 runs; probe includes JSON output; projector is unoptimised Python. Only two documents (one tagged 10-K, one untagged statute) plus tiny synthetic fixtures. No scanned, RTL/CJK, rotated or math-heavy PDFs tested; math handling is only designed.
* Struct join percentage measures *joined*, not *correct*; I spot-read the fixture, not the 10-K.
* The `md.py` quality is a demonstration of the seam, not a quality claim; AAPL headings are visibly over-triggered.
* No external docs were consulted (no browsing); claims about renderer sanitisation (GitHub stripping `style`/inline SVG) are from general knowledge [S] and must be verified against the target renderers before the rich-profile defaults are fixed.

## 16. Artifact index (all under `docslurp-review/markdown-20261025/architect/`)

* `report.md` (this file)
* `src/evidence_probe.cc`, `src/mkfixtures.py`, `src/md.py`, `src/bench.py`, `src/stats.py`
* `fixtures/tagged.pdf`, `untagged.pdf`, `tagged_inline_only.pdf`
* `results/`: `bench.txt`, `named_property_list_gap.txt`, `aapl.struct-join-stats.json`, `tagged.evidence.jsonl`, `untagged.evidence.jsonl`, `tagged.gfm.md`, `tagged.rich.md`, `untagged.gfm.md`, `aapl-10k-fy2025-as-filed.{evidence.jsonl,gfm.md,rich.md}`, `USCODE-2024-title1.{evidence.jsonl,gfm.md,rich.md}`

Note: the AAPL/USCODE evidence files derive from DocSlurp sample PDFs in `~/.cache/docslurp`; they are public SEC/US Code filings, but delete `results/aapl-*` and `results/USCODE-*` if you do not want corpus text kept in the repo (≈15 MB). Compiled probe binary lives in `/home/dan/tmp/agents/evidence_probe` (scratch, not preserved).
