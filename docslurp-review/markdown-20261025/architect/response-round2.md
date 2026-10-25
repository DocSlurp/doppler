# Architect response, round 2

Date 2026-10-25. Checkout HEAD `a5a724ec0` (poppler sources unchanged since round 1). Runtime is still **system poppler 26.10.0** (`/usr/bin/pdftotext -v`, `/usr/lib/libpoppler.so.165`). The checkout has no build. No tracked file was modified. No subagents.

**[F]** means I reproduced it or read it in source. **[S]** means speculation. **[U]** means unverified.

New artifacts, all under `architect/round2/`:

| file | what |
|---|---|
| `mkfix2.py` → `form-reuse-actualtext.pdf` | Page `MCID 0`, the same `MCID 0` inside a Form XObject invoked twice, and three ActualText cases |
| `scope_probe.cc` | Out-of-tree probe against the system library, with a form-scope key and a binary-search run lookup |
| `results-scope.txt` | Fixture output |
| `results-aapl-scope-stats.txt` | Stats mode on the public AAPL 10-K (0.87 s wall) and US Code title 1 |

Build: `g++ -std=c++20 -O2 scope_probe.cc -I/usr/include/poppler -lpoppler`. Run it as `scope_probe file.pdf [stats]`.

---

## 1. Objections, one by one

### (1) `(page, MCID)` is not a sufficient key; how ActualText attaches

**Finding [F].** The key needs a content-stream scope.
- Poppler's own matcher already knows this: `MarkedContentOutputDev::contentStreamMatch()` compares the top of a `formStack` to the struct element's `stmRef`, and requires an empty `formStack` when there is no `Stm` (`poppler/MarkedContentOutputDev.cc:~72-80`). `StructElement::parseChild` reads `/Stm` from the MCR dictionary (`StructElement.cc:1073`). `/StmOwn` is an explicit TODO (`StructElement.cc:1057`).
- The OutputDev hooks exist: `beginForm/endForm(Object*, Ref)` (`OutputDev.h:329,331`), called from `Gfx.cc:4176-4178`.
- The round-1 probe did not override them. Every form's MCIDs were joined as if they belonged to the page stream.

Fixture result, `results-scope.txt`:
```
word     naive(mcid)   scoped(formRef, invocation, mcid)
PAGE0    mcid=0        form=0 inv=0 mcid=0
FORM0    mcid=0        form=6 inv=1 mcid=0
FORM0    mcid=0        form=6 inv=2 mcid=0
```
Under the round-1 key, all three words collide on MCID 0. The scoped key separates them.

**Corrected key.** `scope = (pageNo, streamId, mcid)`, where `streamId` is 0 for the page content stream or the Form XObject's object number. This is the key to use when joining against `MCR/Stm`.

**Invocation serial.** An invocation serial (`inv`) is a different thing: it is a producer-side instance id.
- It is needed so the asset and geometry records of two placements of one form stay distinct.
- It can **not** be resolved against the structure tree, because MCR carries `Stm` (the form object) and not a placement. That is true of what Poppler parses.
- I did not check the ISO text on whether tagged content in a reused form is conformant [U].

**Rule.** If a form is invoked more than once and has MCR leaves, mark those leaves `join: ambiguous_reuse` and demote the blocks to geometry. Do not guess an instance.

**Other drops in the round-1 walk (`evidence_probe.cc:111-133`) [F].**
- `OBJR` leaves return early (`isObjectRef → return`), so figures and link annotations referenced by OBJR are never joined.
- `getPageRef` failing gives `pg=0`. I did not verify whether Poppler inherits `/Pg` from the parent element [U].

**AAPL [F].** 46 of 42,111 words fall inside a Form XObject (`results-aapl-scope-stats.txt`). The effect is small there, but the key is still wrong without the scope.

**A worse defect I found while testing named lists [F].** `fixture` line `/Span /MCA BDC (zz) Tj EMC ( after) Tj`, inside `MCID 2`:
- The begin for the Span is not delivered, because `Gfx.cc:5166-5169` forwards only inline dictionaries.
- The `EMC` is delivered (`Gfx.cc:5191`), so the stack pops the **enclosing** `MCID 2` frame early.
- The word ` after` is joined to `mcid=-1` instead of 2 (`begins=9 ends=10`).

So the named-property gap does not just lose one frame. It corrupts the frame that follows, silently. Round 1 said a misjoin cannot be detected downstream. The only signal is `ends != begins` per page.
- AAPL has 3,736 begins against 3,937 ends, with end-without-begin on all 80 pages.
- The 98.6% "joined" figure from round 1 measured **joined**, not **correct**.
- The contract below makes the gap a gate.

**ActualText: three separate mechanisms, only one of which touches `charPos`.**

| mechanism | who handles it | how to attach |
|---|---|---|
| **(a)** Content-stream `/Span <</ActualText>> BDC`, inline or named (`Gfx.cc:5138-5151` resolves named lists for Span only) | `ActualText` class in `TextOutputDev.cc:5569-5607` | The glyphs inside the span are swallowed. At `EMC`, one `addChar(..., nBytes = total, uni = replacement text)` is emitted. |
| **(b)** `StructElement /ActualText` (`getActualText()`) | No Poppler device | Apply it in the projector over the words joined to that element's leaves. Never use `charPos` for it. |
| **(c)** `/Alt` | Public API | Image and figure alt text. Not text replacement. |

**Why the charPos mirror stays correct for (a) [F, `form-reuse-actualtext.pdf`].**
- `TextPage::charPos` advances once, at span end, by the summed `nBytes`.
- The mirror advances per glyph.
- The two are equal at every word boundary, because no word is created inside a span. The fixture shows `fi` at cp 15 and `tail` at cp 18, both joined to `MCID 1`.
- Named ActualText (`/MCA`) also works: `YY` joined to `MCID 2`.

**Where the mirror is wrong or lossy [F].** Case `lig + <ActualText>(ab + MCID4:(cd))`. Result: one word `ligX` at cp 30, joined to `MCID 3`.
- The nested `MCID 4` never gets a word. It has no word to join, but the struct tree still has a leaf for it.
- `getCharLen()` is `last.charPos - first.charPos` and excludes the last char's length (`TextOutputDev.h:209`).
- A word with `firstKey != lastKey` is therefore detectable only approximately. The stats mode counts 3 of 42,111 AAPL words with first/last char in different keys. This is a lower bound, because middle changes are invisible.

**Rule.**
- Join per word on the first char, and record `straddle` when a run boundary falls inside `[charPos, charPos+len]`.
- Do not try to split ActualText words.
- Struct leaves with no joined word are counted as `leaf_unjoined`.
- The real fix is per-char `charPos` in the public API, or frame ids stored on `TextWord` (round 1 core patch 2).

### (2) One interpretation versus exact visual crops: tiers

I withdraw "lazy assets after layout" as a blanket claim. It conflated three different needs.

| asset kind | needs replay? | notes |
|---|---|---|
| Embedded image by xref (no mask, no non-trivial clip) | **no**, re-open the stream by xref | The stable id comes from `drawImage` placement. The `Object*` ref is non-null only for XObject images. |
| **Inline image** (`BI/ID/EI`) | **yes, or capture in pass 1** | There is no xref, and the callback `Stream*` is positioned in the content stream. The bytes must be consumed in the callback, or the page replayed. |
| Image with SMask/Mask/Decode, clipped, or transformed placement | partial | `drawSoftMaskedImage` carries the mask stream. The clip path is not in the IR (not exported). Exact visual means the pass-1 mask and clip must be captured, or a replay used. |
| Vector figure, chart, rule-based diagram | **yes** | There is no asset until something renders it. Export is either path records → SVG (lossy: clips, patterns, shadings, blend, soft masks absent) or a raster crop. |
| Type 3 text, outlined glyphs, tiling pattern text | **yes** | No Unicode and no faithful vector without the Gfx state. |

**Contract.** There are two named tiers, and the CLI says which one produced each artifact.
- **Tier N (native one-pass).** Facts come from a single Gfx interpretation and nothing replays. Includes: words + styles, MCID-with-scope join, path records, image placement records, inline-image bytes (captured in-callback, size-budgeted), link annots, struct walk.
- **Tier R (conditional replay).** A second interpretation (a Poppler-internal `displayPageSlice` or a recording device) per page or region, triggered only by evidence or request. It produces raster crops and pixel-exact figures.
- Alternative: a **recording device** (display list) captured in pass 1, then replayed to SVG/raster without re-parsing. This is **[S]**: Poppler has no public display list. Cairo and Splash could be used, but I did not test it. Cost and memory are unknown.

The CLI flag is `--figures none|vector|raster-on-demand`. `none` means Tier N only.

**What "one parse" legitimately means:** one parse of the document and one interpretation for text + semantics + placement records. It does not mean exact figures cost nothing. Any visual-fidelity promise beyond Tier N is paid for by Tier R, and the report must show its cost per page.

### (3) "Never silently drops" is not preservation

Agreed. Retract the sentence. A comment saying a thing was dropped is accounting, not fidelity.

**Corrected contract: four outcome classes, counted separately and never merged into one "preserved" number.**
- **P (preserved):** a Markdown/HTML/asset representation that a reader can use in place of the source. It is checked, for example, by text round-trip, table cell count, or a pixel diff on a crop.
- **A (approximated):** a heuristic reconstruction with a confidence value and `source` (`tag|ruling|alignment|size`).
- **R (referenced only):** a bbox/page/xref pointer or a comment. **Not preservation.** Example: a bbox comment in place of a vector chart.
- **L (lost):** no representation, with an area and count in the loss ledger.

The ledger is part of the output: `{class, kind, page, bbox, area_ratio, reason}`. Front matter carries the sums.

**Gates the caller can set:**
- `--fail-on R,L` exits nonzero if any such item exists on a selected page.
- `--max-R-area N%`.
- `--max-fallback-ratio` keeps its original meaning but is not the fidelity gate.

A document with a lot of R and L is reported as a partial export. It is never called complete.

### (4) Prototype restrictions in the round-1 probe

All three are restrictions of the probe, not of the design.

- **`mcidAt` linear scan:** `evidence_probe.cc:108` is O(runs) per word. Fixed in `scope_probe.cc` with `upper_bound`. Not rebenchmarked.
- **First-char font only:** `evidence_probe.cc:170` uses `getFontInfo(0)`. `TextWord::getFontInfo(int idx)` is public per char (`TextOutputDev.h:181`) and `getEdge(i)` gives per-char extents (`.h:213`). So per-char style runs are available now. Per-char `charPos` and per-char font size/colour are **not** public. Mixed-style words (superscript, bold prefix) need a patch or the projector must treat a word as one style.
- **Style reliability:** non-embedded base-14 fonts reported `b=0,i=0` in round 1. Embedded fonts were not tested. Keep the font-name fallback.

### (5) Inline HTML/SVG renderer claims

I remove these claims from `report.md` §11 and replace them with checkable statements.

- **Removed (not supportable by me):** "GitHub strips `style` and inline SVG", "typical sanitisers allow colspan/rowspan". These were from memory.
- **Cited, primary source: GFM spec <https://github.github.com/gfm/>**
  - §1.1: "GitHub.com and GitHub Enterprise perform additional post-processing and sanitization after GFM is converted to HTML." So HTML survival is renderer-specific by design.
  - §4.10: "Block-level elements cannot be inserted in a table", and the table extension text mentions no rowspan/colspan. So merged cells and multi-paragraph cells cannot be expressed in pipe tables.
  - §4.6: an HTML block is treated as raw HTML, not escaped.
  - **Not read:** I did not read the tagfilter section (§6.11) because the fetch truncated; so I make no claim about which tags it filters. I did not verify SVG handling in any renderer.
- **Consequence for the CLI:** the profile defaults are `gfm` (pipe tables, no raw HTML) and `rich` (raw HTML tables). `rich` makes no portability promise. It must be tested per target renderer. DocSlurp's own ingestion and chunker (HTML-aware or not) are the only target I can name from the coordinator notes. Whether inline SVG is accepted there is **[U]**.
- **Safe-by-construction rule for `rich`:** every HTML fragment carries its full text content, so that a renderer that strips tags still shows the text. This is a property of our output that a test can check (strip tags, compare text), not a claim about renderers.

### (6) The pattern probe

Agreed with the coordinator's wording. The 290 ms vs 0.6 ms figure compares full expansion of 31,304 fills with a callback that does nothing with the pattern. It is not a quality-equivalent speedup, and I retract "can be 480x" as a performance claim in `report.md` §12 item 5.

What survives is the **hazard**: with `needNonText=true`, tiling patterns expand to many fill callbacks. The exporter needs a policy, one of:
- **(i)** intercept the pattern and emit a single `pattern` record (id, bbox, resource ref) → class R unless a Tier R crop is made;
- **(ii)** interpret the pattern cell once and instantiate it by transform → class A;
- **(iii)** let it expand, under a per-page callback budget, and fall back to (i) if the budget is hit.

No policy is tested here. A semantic probe must also confirm that text inside a pattern cell is not lost, which is untested.

### (7) Round-1 nonshipping artifacts

The following are labelled **non-shipping demonstration output**. They must not be used as quality evidence.
- `results/*.rich.md` and `src/md.py --profile rich`: raw rich-profile tables still leak Markdown (`**`) into HTML cells.
- `results/aapl-10k-fy2025-as-filed.gfm.md`: geometric heading rule over-triggers (checkbox glyphs and fragments became `##`, 19 `#` lines, many wrong).
- The projector reading order is a `y0` sort, not Poppler's block order.
- The rect→`<svg>` fragment is a demonstration that a path record can reach an HTML fragment. It is not a claim that the fragment is faithful or portable.
- The struct-driven path (headings and tables from tags) was **not implemented** in `md.py`. "Tagged advantage" is unrealised.

---

## 2. Corrected final architecture contract

1. **Producer (C++).** One document parse. One Gfx interpretation per page that produces **PageEvidence v1**, scoped by `(page, streamId, mcid)`.
   - Includes: words with per-char style where available; frame stack with `begin/end` balance counters per page; path records; image placement records; inline-image bytes within a budget; links; one struct walk; metadata.
   - Records `needNonText`, `needCharCount`, and what was not delivered (`capabilities`).
   - Does not emit Markdown.
2. **Join layer.** Given PageEvidence, produce blocks with `join ∈ {exact, straddle, ambiguous_reuse, unbalanced_frames, none}`. Tag-driven structure is used **only** when the page has `ends == begins`, no ambiguity and no `leaf_unjoined` above threshold. Otherwise geometry drives the block, and the join class is kept.
3. **Projector (separate module).** `gfm` and `rich` profiles. No Poppler types in its signature. Emits P/A/R/L classes per block.
4. **Tier R (optional).** Page/region replay for crops and exact figures. Per-page cost reported. Triggered by evidence or flag.
5. **Output.** Markdown + sidecar `evidence.jsonl` + `ledger.json` + `assets/`.

**CLI.** Round-1 names stay placeholders. Corrections:
```
pdftomd [--profile gfm|rich] [--figures none|vector|raster-on-demand]
        [--fail-on R,L] [--max-R-area PCT] [--evidence-out F] [--ledger-out F]
        [--assets dir|none] [--pattern-policy ref|cell|expand-budget=N] file.pdf [out.md|-]
```
`--assets inline` (data: URIs) is dropped from the contract: its renderer support was never established, and it would need Tier N image bytes anyway.

**Fallback table.** The §10 table of round 1 holds with these changes: image row, vector-figure row, and math row now state their class (P/A/R/L) and a condition, and "omit with a count" is class L.

---

## 3. Strongest objections that survive

1. **Per-char provenance is not public.** `TextWord` exposes only the first `charPos`. Every straddle and absorbed-leaf case is detected, not resolved. A real fix needs a Poppler change (frame id on `CharInfo`), and until then mixed-MCID words are demoted to geometry.
2. **Named-property BDC corrupts the frame that follows**, not just itself (confirmed here). Until `Gfx.cc:5166` is patched, the system library gives wrong joins that look valid. A checkout build must prove the patch; I did not build it.
3. **Reused tagged forms are not resolvable** against what Poppler parses (`Stm` only). The correct behaviour is to demote, which loses tag information for those blocks.
4. **Tier R has no cheap implementation yet.** There is no display list. Exact figures mean a second interpretation or a Cairo/Splash route, and nothing here measures that cost.
5. **Quality lives in the projector,** which is unwritten. Round 1 shows its failure modes (headings, tables, order). The architecture does not make that work cheap, only possible to iterate without a C++ build.
6. **Struct-driven structure** depends on tag quality. A document with 98% "joined" words and wrong tags yields confident wrong Markdown. Tag trust needs a check (e.g. text-order consistency between the struct order and geometry order), which is not designed here.
7. **All timings are on the system library** with an un-quiesced machine. Nothing here is a checkout-build number.

---

## 4. Minimal benchmark and acceptance gates (no new big runs)

Gates are pass/fail on fixtures plus two corpus files, and each can run in under a minute.

**G1 Scope join.** `form-reuse-actualtext.pdf`: three `FORM0/PAGE0` words separated by `(streamId, inv)`; `after` joined to `MCID 2`. **Currently fails on the system library** (`after → -1`), and passes only after the named-property patch. The pass condition on a checkout build is `begins == ends` on that fixture.

**G2 Balance.** On each tagged corpus page: `ends - begins` is reported, and any page with a nonzero value is marked `unbalanced_frames`. Acceptance: the exporter's tag-driven path is disabled on those pages. After the patch, AAPL should drop from 80/80 pages unbalanced to the pages that genuinely have `/OC` or unmatched EMC. That count is the metric.

**G3 Join honesty.** Report `joined`, `straddle`, `ambiguous_reuse`, `leaf_unjoined` separately. A single "joined %" is not accepted.

**G4 Class accounting.** Fixture `tagged.pdf`: every shape, image, link and table appears in the ledger with a class. The red rectangle in `gfm` is class R or L (never P). In `rich` with `--figures vector` it is A until it is pixel-checked. No class P without a check.

**G5 Evidence cost.** On AAPL (80 pp) and USCODE (21 pp): Tier N wall time versus `pdftotext -layout`, same machine, 5 runs, report median and min, with the library version printed. Acceptance is a ratio ceiling set by the product owner. Round-1 point estimate was 1.8x for the probe (`needNonText` off), 2.0x on. This is a ratio to improve, not a proven target.

**G6 Tier R budget.** For a page that needs a raster crop, report time per crop. No acceptance number is proposed, because none has been measured.

**G7 Pattern policy.** `pattern-grid.pdf` (coordinator): with policy `ref`, one record and class R; with `expand-budget=1000`, falls back to `ref`; with `cell`, instantiates. All three must report the class and not claim P without a pixel diff. The time ratio is reported but is not a pass criterion.

**G8 Renderer-agnostic text check.** For `rich` output, stripping all tags from each fragment gives the same text as the source blocks (text round-trip). That is the only portability test that does not need a renderer.

---

## 5. Open items for a build-capable round

- Build the checkout in a private build dir. Apply the named-property patch (`Gfx.cc:5138-5169`) and rerun `named-properties.pdf` and G1.
- Check `getPageRef` inheritance and OBJR handling (`StructElement.cc`) [U].
- Measure Tier R candidates (Cairo and Splash region render) on one AAPL page and one vector-chart page.
- Check ISO 32000 text for tagged content in reused forms [U].
