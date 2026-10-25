# Coordinator probes

Observed 2026-10-25. Poppler checkout `e658c5562`; probes link to **installed libpoppler 26.10.0**, not a checkout build. The existing build directory has no built utilities. These are mechanism tests, not a Markdown implementation or a speed comparison of finished products.

## Reproduce

Run `python3 make_fixtures.py` from any directory. Create a scratch directory with `mktemp -d /home/dan/tmp/agents/markdown-review.XXXXXX`, set TMPDIR/TMP/TEMP to `/home/dan/tmp/agents`, and compile:

```sh
g++ -std=c++20 -O2 callback_probe.cc -I/usr/include/poppler -lpoppler -o "$probe_dir/callback-probe"
"$probe_dir/callback-probe" named-properties.pdf
"$probe_dir/callback-probe" named-properties.pdf marked
"$probe_dir/callback-probe" nested-marked.pdf marked
"$probe_dir/callback-probe" pattern-grid.pdf
"$probe_dir/callback-probe" pattern-grid.pdf opaque
```

`probe_dir` must point to the directory returned by mktemp. Raw observed results are in `callback-results.json`. PDF sources are deliberately tiny and preserved in `make_fixtures.py`.

## Expectations and observations

* `named-properties.pdf`: two marked-content sequences, MCID 7 using a named property dictionary and MCID 8 using an inline dictionary. Both should emit begin/end pairs. Observed: only MCID 8 begins, but both end callbacks fire. Selecting MCID 7 yields no text. Source: `poppler/Gfx.cc:5166` only forwards inline dictionaries or one-argument BMC; named-property BDC is omitted. The fixture is a content-callback test, not a fully tagged StructTreeRoot document.
* `nested-marked.pdf`: outer MCID 7 contains **Before inside after**, with a nested MCID-less Span around **inside**. Selecting MCID 7 should yield all 19 characters. Observed: **Before inside **, losing the trailing five characters. Source: `poppler/MarkedContentOutputDev.cc:80` ignores unnumbered begin events but `:97` pops a numbered frame at their end.
* `pattern-grid.pdf`: one repeated two-point tiling pattern over a rectangle. Default handling invokes **31,304 fill callbacks**; intercepting the tiling pattern invokes **one pattern callback**. Seven-run median display time was **290.127 ms expanded** and **0.604 ms intercepted**, with very high noise during concurrent council experiments. The probe merely acknowledges the pattern: it does **not** preserve its graphics or text. This is evidence of an expansion hazard, not a claimed 480× exporter speedup. A real implementation must retain pattern/resource identity and appearance dependencies, or declare a visual fallback, and inspect pattern text if needed.

The PNGs are actual rasterizations of the synthetic PDF inputs using system pdftoppm 26.10.0. No generated illustration substitutes for these observations.

## Source findings relevant to the design

* `poppler/StructElement.cc:819–869` calls displayPages from text retrieval for a content element. Traverse tags once and join by content scope/MCID rather than calling getText on every leaf.
* `poppler/OutputDev.h:88–110` makes capability choices affect interpretation. A combined renderer/extractor is not a blind callback fan-out: Type 3 text, forms, patterns, shadings and coordinate conventions require explicit policy.
* `poppler/Gfx.cc:1937` skips patterns for text-only devices. Turning needNonText on changes the cost model dramatically.
* `poppler/TextOutputDev.cc:5696` runs coalescing at page end. Preserve source runs before that irreversible grouping if several consumers need independent layout decisions.
* `poppler/TextOutputDev.h:446` tableId is a private grouping hint, not a row/column/span structure. `TextOutputDev.cc:3469` finds aligned block clusters; this does not constitute a full table exporter.

## DocSlurp integration inspection (read only)

Adjacent checkout `/home/dan/code/oss/docslurp`, HEAD `5d4d21fc`, inspected without edits:

* `worker/src/native-pdf/poppler-engine.ts:17–28`: layoutTextToMarkdown collapses whitespace and preserves lines. This eliminates table alignment; it is a plain-text fallback, not a rich layout engine.
* `worker/src/native-pdf/types.ts:100–114`: scan() has runs, image placements and glyph totals. Extend this mechanism rather than forcing consumers to re-read the page for every artifact.
* `NativePdfTable` only has bbox and string rows; add cells with row/column spans, header associations, source runs, confidence and fallback references.
* Image placements lack source/resource identity, transform/quads, clips, mask/decode identity and asset references. Drawing output contains counts, not paths. Spans lack character/quad provenance, marked-content ownership, text-source trust and full style information.
* `worker/src/extractors/local-artifact-adapters.ts:103–145`: separate utilities produce page images, text, HTML, metadata and embedded images. A bundle producer can satisfy these artifact demands once, with lazy optional rendering. Existing adapter costs are planner values, not measured elapsed time.

These contracts make Markdown a projection of shared page evidence a practical migration direction. The new API should advertise actual feature availability and return unsupported/fallback status explicitly.


## Follow-up with newly installed perf

Command: `perf record -e cpu-clock:u -F 199 -g -o scan-perf.data -- build/utils/pdfimages -f 1 -l 8 -list /home/dan/.cache/docslurp/pricing-samples-2026-10-21/scanned-document/inauguralofourfi00newy.pdf`. Completed successfully; 434 samples, zero lost. See `scan-perf-report.txt`. The measured operation lists images, not a full Markdown export. No system settings were changed.
