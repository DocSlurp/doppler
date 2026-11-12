# Page facts (`doppler-page-facts/1`)

Compact per-page facts for a page router: whether the text layer can be used as is, or the
page needs OCR, a vision model, table handling or a human. They are computed by the text
extraction pass that `pdftomarkdown` already runs, so they cost little on top of a
conversion, and by `pdfpagefacts`, a facts-only command for callers that need them before or
instead of Markdown.

```sh
pdfpagefacts [-f N] [-l N] [-level basic|full] [-include LIST] [-timing] FILE.pdf|-  > facts.jsonl
pdftomarkdown --stream --page-facts facts.jsonl [--page-facts-level full] FILE.pdf    > out.md
pdftomarkdown --stream --page-facts-fd 3 FILE.pdf 3>facts.jsonl                       > out.md
```

- **One computation, one serializer.** Facts are computed in C++ (`poppler/PageFacts.{h,cc}`)
  on the text pass and serialized once. `pdftotext -page-facts-fd N` writes them to an
  inherited descriptor; `pdftomarkdown --page-facts` passes it one and never reads, parses
  or rewrites a line; `pdfpagefacts` drives the same code without laying out text. For the
  same input, page range, level and include list, all three write **byte-identical** lines
  (tested), whatever else pdftomarkdown is asked for (`--markers`, `--math`, `--stream`).
- **Never on stdout** for pdftomarkdown: Markdown stays on stdout, unchanged by facts.
- The JSON Schema is [`doppler-page-facts-1.schema.json`](doppler-page-facts-1.schema.json).
  The man page is `utils/pdfpagefacts.1`. Benchmarks: [PERF.md](PERF.md).

## Lines

One JSON object per line, ASCII only (`\uXXXX` escapes; `<`, `>` and `&` are always
escaped), keys in a fixed order:

1. one `doc` line;
2. one `page` line per page of the range, ascending;
3. one `run` line. A missing `run` line means the run was cut short (killed, out of memory).

Every line has `"schema":"doppler-page-facts/1"` and `"type"`. **Fields are added, never
repurposed or removed** within `/1`; consumers must ignore unknown fields and unknown enum
values. **Absent is not zero:** a field that was not computed (its family was not asked
for, or a ratio's denominator is zero) is left out.

Numbers: counts are non-negative integers; ratios are in 0..1 with at most 4 decimals;
points have at most 2 decimals and are clamped to ±10^9 (`out-of-range`); every number is
finite (a non-finite intermediate becomes 0 and adds `non-finite`). Trailing zeros are
dropped (`0.5`, `612`).

## Levels and families

`-level basic` (the default) computes the basic fields; `-level full` adds every family;
`-include LIST` adds the named families to basic (`pdfpagefacts -include`,
`pdftotext -page-facts-include`, `pdftomarkdown --page-facts-include`):

| family | adds |
|---|---|
| (basic) | page `geometry`, `text` counts, layer and coverage, `images` totals, `scan`, `parseWarnings`, `unusualFeatures`; doc `pageCount`, version, linearization, xref state, encryption |
| `fonts` | page `fonts` |
| `oddtext` | page `text` ratios (control, private use, U+FFFD, math, vertical, rotated, text over images) |
| `images-detail` | page `images.items` (32 per page), mask and inline counts |
| `vectors` | page `vectors` (paths, coverage, rules, grid score, shadings, patterns, clips, clusters) |
| `forms` | page `forms`; doc `acroForm`, `needAppearances`, `xfa` |
| `annots` | page `annots` |
| `ocg` | page `layers`; doc `ocgCount`, `ocgOffByDefault` |
| `structure` | doc `incrementalUpdates`, `tagged`, `markedContent`, `lang`, `outlineCount`, `pageLabels`, `embeddedFiles`, `javascript`, `openAction`, `launchAction`, `producer`, `creator` |

A flag of `unusualFeatures` appears only when its family was computed (`type3-font` needs
`fonts`, say). The doc line says which: `level` and, for basic plus families, `include`.

## The doc line

| field | meaning |
|---|---|
| `level`, `include` | what was computed |
| `scanThresholds` | only when a scan threshold is not its default: `minCoverage` (0.85), `maxVisibleWords` (20) and `layerOverlap` (0.5) as every page's scan test used them (below); on error lines too |
| `error` | `open-failed` (cannot read the file) or `damaged` (not a PDF poppler can repair): the run ends, exit 1 |
| `locked` | `true`: encrypted and the password is needed; the line holds only `locked`, `encryption` and `unusualFeatures`; exit 2 |
| `pageCount` | the page count poppler processes (the root `/Count`, not the walked tree: bill and limit by `pdfpagecount`) |
| `pdfVersion` | `"M.m"` from the header and catalog `/Version`, absent when not 1.0-2.9 |
| `linearized` | a linearization dictionary is present |
| `xref` | `ok`, or `repaired` when poppler rebuilt the cross-reference table while opening |
| `encryption` | `none`, `empty-user-password` (opens without a password), `user-password-required` (a password was needed, and given) |
| `permissions` | with encryption: `print`, `printHighRes`, `modify`, `copy`, `annotate`, `fillForms`, `accessibility`, `assemble` as the document declares them |
| `incrementalUpdates` | cross-reference sections (tables or streams) chained by `/Prev`, minus one (and minus the first-page section of a linearized file); read with a separate parser, at most 1,000 sections, 30 million tokens and twice the file's bytes, absent past that or when the table was rebuilt (`xref: repaired`) |
| `tagged` | the catalog has `/StructTreeRoot` (the tree is not parsed) |
| `markedContent` | `/MarkInfo /Marked true` |
| `lang` | the catalog `/Lang`, only when its bytes (ASCII, or UTF-16BE of ASCII) are a BCP 47-like tag (`^[A-Za-z]{2,8}(-[A-Za-z0-9]{1,8}){0,4}$`); nothing is dropped to make it one |
| `outlineCount` | outline items (`/First`/`/Next`), at most 10,000 (`truncated` names it past that) |
| `pageLabels` | the catalog has `/PageLabels` |
| `embeddedFiles` | entries of the `/EmbeddedFiles` name tree, walking at most 10,000 nodes and counting at most 1,000,000 entries (`truncated` either way); an array several nodes share is counted once |
| `javascript` | a `/JavaScript` name tree, or a JavaScript `/OpenAction` or catalog `/AA` action (presence only; page and annotation actions are not searched) |
| `openAction` | the catalog has `/OpenAction` |
| `launchAction` | the `/OpenAction` or a catalog `/AA` action is a Launch action |
| `producer`, `creator` | **untrusted, informational** strings from the Info dictionary: decoded (UTF-16BE, UTF-8 or PDFDocEncoding), control characters dropped, at most 128 code points. Never used to set any other field. The only document text in the output |
| `truncated` | which of `embeddedFiles`, `outlineCount`, `strings` hit a cap |
| `acroForm` | `none` or `present` (a catalog `/AcroForm` dictionary) |
| `needAppearances` | `/AcroForm /NeedAppearances true` |
| `xfa` | `none`, `static`, or `dynamic` when the XFA config packet sets `<dynamicRender>required` (at most 1 MiB of it is read, and only through Flate, LZW, ASCII hex/85 or RunLength filters: a packet behind an image codec is not decoded and reads as `static`) |
| `ocgCount`, `ocgOffByDefault` | optional content groups, and those off in the default configuration |
| `unusualFeatures` | the document-level flags below |

## The page line

`page` is the PDF page number (so a `-f`/`-l` range names real pages). A page that could not
be displayed has `"error":"failed"` and `page-failed`, with no other facts.

Coordinates (`bbox`, page width and height used for coverage) are page points in the same
space as pdftotext's and pdftomarkdown's word and image boxes: the page's MediaBox, rotated
by `/Rotate`, y down, origin top left. Coverage ratios are shares of that MediaBox.

**Coverage grids.** Union areas (`textCoverage`, `imageCoverage`, `vectors.coverage`,
`uncoveredRatio`, `textOverImageRatio` and the scan test) use a 64 x 64 occupancy grid per
kind: a box marks the cells whose centres it contains, so each ratio is quantised to 1/4096 of
the page and a box smaller than a cell may mark nothing. It is deterministic and costs one OR
per grid row a box spans, however many boxes a hostile page draws.

### `geometry` (basic)

| field | meaning |
|---|---|
| `widthPt`, `heightPt` | the CropBox size, swapped for a 90 or 270 degree rotation (in units of `userUnit`) |
| `rotation` | `/Rotate`, normalised to 0, 90, 180, 270 |
| `userUnit` | `/UserUnit` (default 1; 0.0001 to 1,000,000, 4 decimals) |
| `mediaBox`, `cropBox` | `[x0,y0,x1,y1]`, both present only when they differ |

### `text` (basic; ratios with `oddtext`)

Characters are the code points text extraction keeps (whitespace and NUL break words and are
not counted; characters outside the page are dropped). Words are the words extraction builds
**before** duplicate text (fake bold, shadows) is removed by layout, so they can be a few more
than pdftomarkdown's report counts; this is what lets `pdfpagefacts`, which does not lay
out lines, write the same values.

| field | meaning |
|---|---|
| `charCount` | characters |
| `visibleCharCount`, `invisibleCharCount` | invisible: render mode 3 (no paint), or the glyph's centre outside the clip. White-on-white is not detected |
| `wordCount`, `visibleWordCount`, `invisibleWordCount` | words; a word is invisible when none of its glyphs is visible (render mode 3, or centre outside the clip), as for characters |
| `textLayer` | `none` (no words), `invisible` (all invisible: an OCR layer), `visible`, `mixed` (the issue's three values plus `mixed`, as `pdftomarkdown --page-info` reports) |
| `textCoverage` | grid union of word boxes |
| `clippedCharCount` | characters whose centre is outside the clip (included in `invisibleCharCount`) |
| `controlCharRatio` | C0/C1 control characters (U+0000-001F, U+007F-009F) per character |
| `privateUseRatio` | U+E000-F8FF and planes 15-16 |
| `replacementCharRatio` | U+FFFD |
| `mathGlyphRatio` | U+2200-22FF, 27C0-27EF, 2980-29FF, 2A00-2AFF, 1D400-1D7FF, and every character of a math or symbol font (CMMI, CMSY, CMEX, MSAM, MSBM, Symbol, STIX, Euler, ... by name) |
| `verticalCharRatio` | characters of vertical-writing fonts |
| `rotatedTextRatio` | characters not upright on the page as displayed (rotated by a multiple of 90 degrees, or diagonal); vertical writing counts as upright. Text of a `/Rotate 90` page that is upright in the page's own coordinates is rotated as displayed |
| `textOverImageRatio` | characters whose centre lies in an image grid cell |
| `invisibleOverFullPageImage` | the page has a full-page image and at least half of its invisible characters lie over images (an OCR layer over a scan) |

### `fonts` (`fonts`)

Fonts that drew characters on the page (at most 256 tracked: `truncated`).

| field | meaning |
|---|---|
| `count` | distinct fonts |
| `smallestPt` | the smallest font size drawn, in points after the text matrix |
| `type3`, `nonEmbedded`, `cid`, `noToUnicode` | fonts of each kind (a Type 3 font counts as embedded) |
| `glyphCount` | glyphs drawn (including those without Unicode) |
| `unmappedGlyphRatio` | glyphs that mapped to no Unicode at all |
| `ocrFontHint` | a font named like an OCR tool's invisible font (`GlyphLessFont`) |
| `truncated` | more than 256 fonts |

### `images` (basic; `items`, `maskCount`, `inlineCount` with `images-detail`)

Every visible image and stencil mask, from the image dictionary only (the data is never
read or decoded); tiny ones (under 2 points a side after clipping) are skipped, as for
`pdftotext -images`. At most 2,000 per page are kept (`truncated`, and `limit-hit`).
pdftotext's run-wide budget of 100,000 images (and rules) limits only its XHTML: the facts
and the scan test of a page never depend on the pages before it.

| field | meaning |
|---|---|
| `imageCount` | images |
| `imageCoverage` | grid union of their visible boxes |
| `largestImageCoverage` | the largest one's box over the page |
| `fullPageImage` | one image, or the union of several, covers at least 0.85 of the page (`-scan-min-coverage`), whatever the words |
| `scan` | `none`, `single`, `tiled` or `layered` (below) |
| `scanImage` | with a scan: the 1-based position of the covering image in the page's list (the `M` of pdftomarkdown's `pN.M` ids) |
| `maskCount`, `inlineCount` | stencil masks; inline images |
| `items` | the first 32 images (`itemsTruncated` beyond) |
| `truncated` | the image list hit its cap |

Each item: `index` (1-based), `bbox`, `coverage`, `px` `[w,h]` (samples), `dpi` `[x,y]` (samples
per inch along the image's own axes), `filter` (the codec, the last `/Filter`: `dct`, `jpx`,
`jbig2`, `ccitt`, `flate`, `lzw`, `rl`, `none`, `other`), `bpc` (absent when not given),
`colorSpace` (`gray`, `rgb`, `cmyk`, `indexed`, `lab`, `icc1`, `icc3`, `icc4`, `sep`, `devn`,
`other`, `none`; CalGray/CalRGB as gray/rgb), `imageMask`, `softMask` (`/SMask` or
`/SMaskInData`), `mask` (`/Mask`), `decode` (`/Decode` present), `inline`, `obj` (the image
XObject's object number: the same picture on several pages has the same `obj`; callers
aggregate across lines, a page line never depends on other pages), and `copy`, how its
pixels can be taken out without rendering: `jpeg` (copy the stream: sole DCT filter, 8 bits,
gray or RGB, no `/Decode`, no `/DecodeParms` that could set `/ColorTransform`, no mask),
`bilevel` (1-bit CCITT, JBIG2 or gray, no `/Decode`, no soft mask), `decode` (a single image
to decode at native size), `render` (inline, masked or soft-masked images, unusual
encodings, an XObject whose data is an external file (`/F`): render the region). The class
reads the dictionary only: a copier must still check the bytes (for `jpeg`: a baseline SOF
whose component count matches the colour space and whose size matches `/Width` and
`/Height`), since a hostile dictionary can say anything.

**Scan detection** (shared with `pdftotext -images`, which writes `scan="..." scanImage="K"`
on `<page>`): `fullPageImage`, and at most 20 visible words. Then `single` when one image
covers at least 0.85 of the page by itself; `layered` when that image has a `/SMask` or
`/Mask`, or another image (a stencil text layer, say) covering at least half the page
overlaps it by at least a quarter of the page (mixed raster content); `tiled` when only the
union reaches 0.85 (strips, tiles), `layered` again if two half-page images of the union
overlap.

Three thresholds can be changed, in `pdfpagefacts` and `pdftotext` (both the facts and
`scan=`; `pdftomarkdown --scan-max-words N` passes the word threshold on). A changed threshold
is recorded on the doc line (`scanThresholds`, all three values); with the defaults the
output is unchanged, byte for byte.

| option | default | meaning |
|---|---|---|
| `-scan-min-coverage F` | 0.85 | the share of the page one image, or the union, must cover (also `fullPageImage`); in (0, 1] |
| `-scan-max-words N` | 20 | a page with more visible words is not a scan |
| `-scan-layer-overlap F` | 0.5 | the share of the page a second image must cover to count as a layer (it must still overlap the scan by a quarter of the page); in (0, 1] |

A share is a plain decimal (`0.8`, `.8`, `1`; no exponent, sign, `inf` or `nan`); anything
else, or a value outside (0, 1], is a usage error (exit 99).

### `vectors` (`vectors`)

| field | meaning |
|---|---|
| `paths` | painted paths (fills and strokes shown; a fill-and-stroke counts twice), at most 100,000 per page |
| `filled`, `stroked` | of them |
| `coverage` | grid union of the paths' clipped boxes |
| `uncoveredRatio` | vector grid cells with no word on them, over vector cells: high for charts and diagrams, low for text decoration (absent without vector cells) |
| `hRules`, `vRules` | horizontal and vertical rules (thin axis-aligned strokes and fills, as `pdftotext -rules` finds them; image-mask rules excluded) |
| `ruleGridScore` | distinct rule rows (1 pt apart) and columns that cross, over rows x columns, from the first 512 of each: 1 for a fully ruled table, 0 without two rows and two columns |
| `shadings` | `sh` operators and shading-pattern paints |
| `patterns` | tiling-pattern paints |
| `clips` | clipping operators (`W`, `W*`) |
| `figureClusters` | clusters of curved or diagonal drawing (the raw material of pdftomarkdown's vector figures, before its size and density tests) |
| `clusters` | the first 32: `bbox`, `paths`, `curves`, `segments` (`clustersTruncated`) |
| `limitHit` | the 100,000-path cap or the per-page rule cap (20,000) was hit: counts are lower bounds (`drawing-limit`) |

### `forms` (`forms`)

Widget annotations of the page and their fields (`/FT`, `/Ff` and `/V` inherited through at
most 32 `/Parent` levels). Values are never written.

| field | meaning |
|---|---|
| `fields` | widgets with a field type |
| `text`, `checkbox`, `radio`, `choice`, `signature`, `pushButton` | by type |
| `filled`, `empty` | text and choice: a non-empty value; check boxes and radio buttons: the value names this widget's on state |
| `valueAppearanceMismatch` | the value and what the appearance draws disagree: a text or choice value whose appearance drew no glyph (or a glyph without a value); a button whose `/AS` disagrees with the value |
| `signed`, `unsigned` | signature fields with and without a `/V` signature dictionary (no validation) |

### `annots` (`annots`)

| field | meaning |
|---|---|
| `count` | annotations, at most 10,000 (`truncated`) |
| `bySubtype` | counts by subtype: `text`, `link`, `freeText`, `line`, `square`, `circle`, `polygon`, `polyLine`, `highlight`, `underline`, `squiggly`, `strikeOut`, `stamp`, `caret`, `ink`, `popup`, `fileAttachment`, `sound`, `movie`, `widget`, `screen`, `printerMark`, `trapNet`, `watermark`, `threeD`, `richMedia`, `other` (Redact and unknown subtypes); only subtypes present |
| `linkUri`, `linkInternal`, `linkOther` | links to a URI; to a destination in the document (GoTo, named actions, `/Dest`); anything else (GoToR, Launch, JavaScript...). The URI itself is never written |
| `visibleAppearance` | annotations whose appearance drew something (paint operators or glyphs) when the page was displayed: content a text-only extraction of the page stream would miss |

### `layers` (`ocg`)

| field | meaning |
|---|---|
| `hiddenContent` | painting operators ran while optional content was hidden (BDC `/OC` off, or an XObject with a hidden `/OC`) |
| `visibleContent` | painting operators ran inside a shown optional-content section, or a shown XObject has `/OC` |
| `hiddenOps`, `visibleOps` | those operator counts |

Hidden content is not text: its characters are not counted in `text`.

### `parseWarnings` (basic)

Sorted, unique; what went wrong while the page was processed, by poppler's error category
and, for a few fixed message prefixes, the stage. Message text is never kept.

| value | meaning |
|---|---|
| `syntax-warning`, `syntax-error`, `io-error`, `unimplemented`, `internal-error` | poppler error categories |
| `bad-content-stream` | content-stream errors (unknown operators, wrong operands, unbalanced state) |
| `missing-font` | text shown without a usable font |
| `broken-image` | bad image parameters or codec errors |
| `limit-hit` | a cap was reached (images, fonts, paths, rules, annotations) |
| `non-finite`, `out-of-range` | a number was not finite, or beyond ±10^9 points |
| `xref-repaired` | the cross-reference table was rebuilt while this page was read |
| `page-failed` | the page could not be displayed (`"error":"failed"`) |

Errors raised while loading a shared resource (a font, an image dictionary) are reported on
the first page of the run that loads it, so `parseWarnings` (unlike the other fields) can
differ between a whole run and a single-page run.

### `unusualFeatures`

Sorted, unique, fixed names. Page lines:

| name | when (family) |
|---|---|
| `huge-page` | a side times `userUnit` is over 14,400 pt (200 in) (basic) |
| `rotated-page` | `rotation` is not 0 (basic) |
| `scan-page`, `layered-scan` | `scan` is not `none`; is `layered` (basic) |
| `rotated-content` | `rotatedTextRatio` at least 0.2 with at least 10 characters (`oddtext`) |
| `type3-font` | a Type 3 font drew text (`fonts`) |
| `no-tounicode` | a font without a ToUnicode map whose Unicode is a guess (CID, Type 3 or symbolic) drew text (`fonts`) |
| `drawing-limit` | `vectors.limitHit` (`vectors`) |
| `acroform-filled`, `acroform-empty` | a filled, an empty field (`forms`) |
| `annotation-text`, `annotation-freetext`, `annotation-highlight` | an annotation of that subtype (`annots`) |
| `annotation-other` | any other subtype except Link, Widget and Popup (`annots`) |
| `link-uri`, `link-internal` | `linkUri`, `linkInternal` > 0 (`annots`) |
| `ocg-hidden`, `ocg-visible` | `layers.hiddenContent`, `layers.visibleContent` (`ocg`) |

Doc line: `encrypted-empty-password`, `encrypted-user-password` (basic), `xfa-present`
(`forms`), `javascript-present`, `embedded-files` (`structure`).

## The run line

| field | meaning |
|---|---|
| `pagesEmitted`, `pagesFailed` | page lines written, and those with `"error":"failed"` |
| `stop` | `max-pages` when `pdfpagefacts -max-pages` stopped the run (exit 3) |
| `version`, `commit` | this build (`PACKAGE_VERSION`, the git commit at build time) |
| `totalMs`, `peakRssKiB` | with timing only |

## Determinism and page locality

- The same input, flags and build give byte-identical lines. Timing (`factsMs` on page lines,
  `totalMs` and `peakRssKiB` on the run line) is written only with `-timing`
  (`-page-facts-timing`, `--page-facts-timing`).
- A page line depends only on that page and the resources it shares with others, never on
  which other pages were processed: `-f p -l p` gives the same line as a whole run (tested).
  The exceptions are documented above: `parseWarnings` of shared resources, and the
  run-wide caps (100,000 images, 200,000 rules), past which `limit-hit` is set.

## Hostile input

- Every list and string is capped (`truncated`, `itemsTruncated`, `clustersTruncated`).
- No document path, URI, field name or value, file name, font name or page text is ever
  written; the only document strings are `producer` and `creator` (sanitised, capped,
  escaped, marked untrusted) and `lang` (only when it is a language tag).
- A page that fails yields its row; a document that cannot be opened yields one doc line
  with `error` (or `locked`).
- `pdfpagefacts` writes nothing but stdout; diagnostics are fixed strings (poppler's messages
  are dropped and fd 2 points at /dev/null while the document is processed); it caps its
  address space (`-max-memory`, 4096 MiB) and run time (`-timeout`, 600 s, exit 3), closes
  descriptors above 2 at start, and confines itself after opening the file and before
  parsing a byte of it: no file writes or creation, no network, no exec (`-confine
  require|auto|off`, default `require`: a kernel without Landlock or seccomp is exit 5;
  `auto` falls back with a fixed-string warning; Landlock + seccomp, shared with pdfregions in
  `utils/Confine`). It must still run inside the credential-free, network-blocked VM boundary
  DocSlurp requires for customer files.
- pdftomarkdown's `--page-facts FILE` must not exist, is reached without following symbolic
  links, is created mode 0600 under a temporary name and renamed into place as soon as
  pdftotext has finished (before image extraction); a FILE without a run line is incomplete.
  The descriptor is inheritable only while pdftotext starts; a caller's `--page-facts-fd`
  descriptor cannot be marked close-on-exec from safe Rust, so pdfregions (and pdfpagefacts)
  close every descriptor above 2 at start.
- The structure and form walks fetch each shared object once: widgets' indirect `/Parent`,
  `/V`, `/FT`, `/Ff`, `/AP` through a per-document cache, a name tree's shared `/Names` and
  `/Kids` arrays once per walk, and the `/Prev` walk within a byte budget.
- A facts reader that closes its end does not stop the conversion: pdftotext blocks SIGPIPE
  around facts writes, finishes the XHTML and exits 4; pdftomarkdown keeps the Markdown and
  exits 5.
- Tests: a mutation fuzz of the fixtures (`test/docslurp/page-facts/test_mutation_fuzz.py`,
  320 cases in ctest; `PAGE_FACTS_FUZZ_CASES=2000` locally): no signal, exit 0-3, every line
  valid JSON and valid against the schema, pdftotext agreeing with pdfpagefacts.

## Exit status of pdfpagefacts

0 ok; 1 cannot open (a doc line with `error`), no pages, or stdout cannot be written; 2
encrypted and the password is needed (a doc line with `locked`); 3 limit (`-max-pages`, then
the run line has `stop`; out of memory; or `-timeout`); 5 `-confine require` (the default)
could not confine; 99 usage
(including a first page past the last).

## What is not here (deferred)

- A static, minimal `pdfpagefacts` build for faster cold start (a packaging change; cold
  start numbers are in PERF.md).
- Pixel statistics, thumbnails, signature validation, PDF/A checks (out of scope in #32).
- White-on-white text detection; JavaScript in page or annotation actions.
