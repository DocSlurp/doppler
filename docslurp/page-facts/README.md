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
| `images-detail` | page `images.items` (32 per page, with words inside and bytes per pixel), mask and inline counts |
| `vectors` | page `vectors` (paths, coverage, rules, grid score, ruled grids, shadings, patterns, clips, clusters) |
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
| `xfa` | `none`, `static`, or `dynamic` when the XFA config packet sets `<dynamicRender>required` (at most 1 MiB of it is read, each packet stream once, and only through no filter or a single Flate, LZW, ASCII hex/85 or RunLength filter: a packet behind an image codec or stacked filters is not decoded and reads as `static`) |
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
`DeviceGray` or `DeviceRGB`, no `/Decode`, no `/DecodeParms` that could set `/ColorTransform`,
no mask), `bilevel` (1-bit CCITT, JBIG2 or `DeviceGray`, no `/Decode`, no soft mask), `decode`
(a single image to decode at native size through its colour space: ICC-based and calibrated
images, whose profile or gamma can change what the page shows, are never copied as stored),
`render` (inline, masked or soft-masked images, unusual encodings, an XObject whose data is an
external file (`/F`), or resources that define `DefaultGray`, `DefaultRGB` or `DefaultCMYK`:
render the region). The class
reads the dictionary only: a copier must still check the bytes (for `jpeg`: a baseline SOF
whose component count matches the colour space and whose size matches `/Width` and
`/Height`), since a hostile dictionary can say anything.

Each item also has (added within `/1`; absent from earlier builds):

- `visibleWordsInside`, `invisibleWordsInside`: the page's words whose box lies inside the
  image's `bbox` (each side may pass it by at most 1 point), visible and invisible as
  `visibleWordCount` and `invisibleWordCount` count them. An OCR'd scan has about the page's
  word count invisible inside its image; a picture with a caption drawn over it has a few
  visible words; a plain photo none. A word straddling the image's edge counts for neither.
  The box is axis-aligned: words in the corners of a rotated or skewed image, or outside a
  clip within its box, count as inside; and, as for `visibleWordCount`, paint over text is
  not tracked, so text drawn before an image that covers it still counts as visible.
- `visibleWordsCentredInside`, `invisibleWordsCentredInside` (issue 43): the page's words whose
  box *centre* lies inside the image's `bbox` (within the same 1 point), split the same way, for
  the same 32 listed images. A caption line straddling the image's edge counts when its centre is
  inside, which the whole-box counts above never do; a reader that tests word centres (the
  layout walker's `wordsInside`) should read these. Against that walker: the walker's box is the
  image's unclipped placement rounded to 0.01 point and it has no slack, while `bbox` here is the
  part inside the clip and the page; a word centre within 1 point outside the box, or an image
  clipped by its page or clip path, can count differently.
- `bytesPerPixel`: the image stream's encoded bytes over `px` width x height, 6 decimals. The
  bytes are the stream's declared `/Length` (direct or by reference), and only when it is
  consistent: a non-negative integer below 2^31 as written in the file, within the file, with
  `endstream` right after the data it spans (past at most 32 whitespace bytes; the 9 bytes
  there are read, the image data never is). A missing, negative, non-integer, oversized or
  wrong `/Length` leaves the field absent, never a guess: the length the parser falls back on
  for a wrong one depends on whether the cross-reference table was rebuilt earlier in the
  run, which would make a page's value depend on the pages read before it. It counts the
  stored bytes, whatever the filters (an ASCII85 wrapper inflates it; an encrypted stream
  counts its AES IV and padding; a JBIG2 image's `/JBIG2Globals` are not counted). With
  `filter` and `dpi` it tells a photographic scan (DCT, a few tenths of a byte per pixel) from
  flat or line art (Flate, hundredths or less; bilevel scans thousandths). Absent for inline
  images (their data is in the content stream) and for image data in an external file (`/F`).
  It is what the file says, not a measurement of the picture: a crafted stream can pad its
  data, or end its `/Length` at a later `endstream`, to claim a larger value.

**Scan detection** (shared with `pdftotext -images`, which writes `scan="..." scanImage="K"`
on `<page>`): `fullPageImage`, and at most 20 visible words. Then `single` when one image
covers at least 0.85 of the page by itself; `layered` when that image has a `/SMask` or
`/Mask`, or another image (a stencil text layer, say) covering at least half the page
overlaps it by at least a quarter of the page (mixed raster content); `tiled` when only the
union reaches 0.85 (strips, tiles), `layered` again if two half-page images of the union
overlap.

Three thresholds can be changed, in `pdfpagefacts` and `pdftotext` (both the facts and
`scan=`) and in `pdftomarkdown` (`--scan-min-coverage`, `--scan-max-words`,
`--scan-layer-overlap`, passed on to pdftotext). A changed threshold
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
| `figureClusters` | clusters of curved or diagonal drawing, and bar charts drawn only with filled boxes (the raw material of pdftomarkdown's vector figures, before its size and density tests). Bar charts follow doppler-md `vector::bar_regions` (below the table); one overlapping drawing clusters is joined with all of them (which can merge drawing clusters, so their count and order are not getGraphics' alone), otherwise it is listed after the drawing clusters, at most 64 a page |
| `clusters` | the first 32: `bbox`, `paths`, `curves`, `segments` (`clustersTruncated`). A bar cluster has `curves` and `segments` 0 (unless joined into a drawing), and its `paths` count its boxes plus the stroked rules near it |
| `grids` | ruled grids (below), at most 64: `bbox`, `rows`, `columns`, `coverage` (added within `/1`), and `hRules`, `vRules`, `outerSides`, `connected` (issue 43, below) |
| `gridsTruncated` | more than 64 grids, or the grid search (or the thin-rule search) hit its rule or work cap: the list is incomplete |
| `unlistedRuledGrids` | walker-style ruled grids (connected thin rules) that no counted grid stands for (issue 43, below) |
| `limitHit` | the 100,000-path cap or the per-page rule cap (20,000) was hit: counts are lower bounds; or the run's figure-merge budget (2^27 box comparisons) ran out and `figureClusters` is merged only so far, or the page's bar-chart search hit its 64-run cap, its word cap or its budget (8M comparisons a page, 2^28 a run; never the run's figure budget) (`drawing-limit`) |

**Bar charts** (doppler#38): a run is at least 3 filled boxes whose bases (bottom edge, or left edge for horizontal
bars) are within 0.5 pt, with gaps of at most 3 times the widest bar; its lengths differ (longest at least 1.15 times
the shortest); at most a third of the bars hold a word's centre; and a stroked axis lies within 2 pt of the baseline and
spans at least 80% of the run; and a scale: at least 3 numbers beside the run (left or right of vertical bars, below or
above horizontal ones) whose values increase along the value axis. Redactions and blank form fields have no scale; colour
does not decide (they come in every colour, and black bar charts are common). White fills (backgrounds), boxes over a
quarter of the page and the 9% header and footer bands are never bars. At most 4,000 boxes, 100,000 words (past it
`limitHit`) and 20,000 numbers a page are read. Each page has its own budget of 8M comparisons and the run at most 2^28
in all. The same rule and constants as doppler-md `vector::bar_regions`, at doppler-md's default `Params`.

**Ruled grids** are the grids pdftomarkdown's ruled-table search (`doppler-md`
`tables::ruled_tables`) starts from, found from the rules alone with its default parameters,
so a router can see where a table is and how big it is without a layout pass:

- Rules are the page's rules as `pdftotext -rules` writes them (image-mask rules included),
  dark ones only (luminance at most 0.97): horizontal ones (at least as wide as tall) at least
  half a body size wide, vertical ones (taller than wide) at least 0.8 of it tall; a shaded box
  (a filled rectangle under 0.95 of the page's width and half its height, whatever its colour)
  adds its top and bottom edges as horizontal rules. Coordinates are used as computed;
  pdftomarkdown reads them (and word sizes) rounded to 2 decimals, which can flip a threshold
  at its edge.
- Collinear horizontal segments that touch (within 1 point vertically and 2 along) are
  joined, and those shorter than 3 body sizes dropped. Down the page, a rule joins the first
  group whose last rule is at most 8 body sizes above it and that it overlaps by 0.8 of the
  narrower of the two; a group is split where the gap between its rule rows (rows 1.5 points
  apart are one) is more than 2.5 times their median, given at least three gaps.
- `bbox` is the group's span by its first and last rule row; `rows` the bands between its
  rule rows, which are not always table rows (shaded row boxes with gaps between them give two
  bands a box: the box and the gap); `columns` one more than its inner column lines: vertical
  rules more than 2 points inside its sides crossing at least half its height, 2 points apart.
  A group is a grid with at least three rule rows, or two and a column line. `coverage` is the
  box over the page. Three or more parallel rules (form lines, lined paper, a letterhead) make
  a grid of one column too: treat `columns` 1 as weak evidence of a table.
- The body size is the page's most common word size (half points, by characters, invisible
  words included; the first seen on a tie), or 10 without words; pdftomarkdown takes it over
  the document, so on pages whose text size differs from the document's the two can differ. A
  word size outside 0 to 4,095.5 points is not counted and sets `gridsTruncated` (the body
  size, and so the grids, may be wrong). Text much larger than the page's real body text that
  outnumbers it (invisible text included) raises every threshold and can hide a table.
- What pdftomarkdown then does with the text is not done here: it does not split two tables
  at a single-cell caption band between them, add a header row above the first rule, or find
  columns from word gutters, so a table ruled only horizontally has `columns` 1 here.
- The order is the search's: by the first rule row of the group each grid was split from,
  then down that group. Coordinates are the page as displayed: on a `/Rotate 90` page a table
  upright in the file's own coordinates has its rules turned, and is not found (as in
  pdftomarkdown).
- Cost: at most 4,096 horizontal rules (a shaded box counts two) and 4,096 vertical ones are
  taken, in drawing order, and at most 2^20 open-group and column-rule visits are made a page;
  past either, or past 64 grids, the grids found so far are listed and `gridsTruncated` is set
  (grouping cut short lists none). Rules past the page's rule cap (20,000, `limitHit`) also
  set it. `gridsTruncated` means the list may be incomplete; a decoy that fills a cap before a
  real table is drawn hides the table but not the flag. It adds nothing to `limitHit` or
  `limit-hit`.

**Thin rules per grid** (issue 43, added within `/1`; absent from earlier builds). The grid
search above also builds grids from shaded boxes, and finds column lines without outer sides;
`hRules`, `vRules`, `outerSides` and `connected` on each grid, and `unlistedRuledGrids` on the
page, say which of its lines are thin ruled lines, the way a reader that finds ruled grids from
connected thin rules (DocSlurp's layout walker, `pdf_layout_facts.ruled_grids`) counts them:

- A thin rule is a `<rule>` of `pdftotext -rules` (a stroked line, or a fill at most 1.5 points
  thin; never a shaded `<box>`), of any colour (white rules included, as the walker reads them),
  taken at the 0.01 point the XHTML writes (rounded exactly as
  its `%.2f` is), at most 3 points thick and at least 8 points long (horizontal: height ≤ 3,
  width ≥ 8; vertical the other way round). Shaded fills thicker than 1.5 points and lines
  stroked thicker than 3 are not thin rules; dashes and underlines 8 points or longer are.
- Across the page, a horizontal and a vertical thin rule whose boxes come within 2.5 points join
  one set (the walker's test); a set with at least 2 distinct horizontal lines and 3 vertical
  ones is a walker grid.
- A grid's rules are the thin rules whose centre line lies in its `bbox` grown by 2.5 points and
  whose span overlaps that box. When some of them belong to a set of both directions, the grid
  stands for the set with the most rules there (on a tie, the one holding the topmost horizontal
  rule), and `connected` is true: a stray underline in a cell, or a separate checkbox, is not
  counted. If that set reaches outside the box (a column line running on to a rule above or
  below, a table the grid search splits in two) or another grid stands for it too, the grid
  gets none of the four fields: its counts would not be the walker's. When no rule in the box
  belongs to such a set, all of them count and `connected` is false.
- `hRules` and `vRules` are the distinct lines counted: centres more than 2.5 points apart,
  clustered from the lowest value as the walker does. A table drawn cell by cell has one line
  per row border. A fully ruled table has `hRules` = rows + 1 and `vRules` = columns + 1 (its
  walker grid's rows and columns); a zebra table of shaded rows with no rules of any kind has 0
  and 0.
- `outerSides` (0-4): the sides of `bbox` that the counted rules (centre within 2.5 points of
  the side, each assigned to the nearer side) cover for at least 80% of its length. A fully
  ruled table has 4; column lines only, 0.
- `unlistedRuledGrids` (page): walker grids that no grid with `connected` true stands for,
  because the grid search found no grid there (rules lighter than luminance 0.97, which it
  skips; rows more than 8 body sizes apart) or the grid was left uncounted.
- **Reading them as the walker's grids:** the page's walker grids are known only when
  `gridsTruncated` is false, `unlistedRuledGrids` is 0 and every grid has `hRules`. They are
  then the grids with `connected` true, `hRules` ≥ 2 and `vRules` ≥ 3, of `hRules` - 1 rows and
  `vRules` - 1 columns. Otherwise treat the page's ruled grids as unknown. On the walker's public
  fixtures and the grid fixtures here (`test_walker_parity.py`), every page this reads as known
  equals the walker, as long as the facts and the XHTML the walker reads come from the same
  pdftotext flags: thin image-mask rules are rules only when `pdftotext -glyphs` collects them
  (`pdfpagefacts` and pdftotext without `-glyphs` never do), so facts from one and XHTML from the
  other can disagree on a table drawn with stencil masks. The two runs must also cover the same
  page range (the run's rule budget depends on where it starts), and a reader should treat a
  page with `xref-repaired` among its `parseWarnings` as unknown too: a rebuild after the
  page's vector facts were taken is not seen by them. Pages the grid search cannot see
  (`/Rotate 90` and `270` turn a table's rules, rules lighter than luminance 0.97) are unknown
  through `unlistedRuledGrids`, never known.
- Absent from every grid of the page (with `unlistedRuledGrids`, and `gridsTruncated` set) when
  the page's rule list is partial (the page's rule cap); the walker would read fewer rules than
  the page has (more than 5,000 non-box rules, its cap, or any past the run's 200,000-rule
  budget, which the XHTML leaves out: a run-wide exception to page locality, like the other
  run-wide caps); the cross-reference table was rebuilt before the page's facts were taken (at
  open, by the doc line's reads of outlines, names or page labels, which pdftotext alone never
  makes, or by an earlier fetch: an object may then resolve to another body than a separate
  pdftotext run reads); a thin rule's extent or centre is not finite or past 10^9 points; more than
  4,096 thin rules run either
  way, or the search makes more than 2^20 rule visits a page (the touch tests between rules of
  the two directions, found by a sorted sweep, and each grid's candidate rules): counts from
  part of the rules could be too low.

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
| `truncated` | the document's form budget (65,536 distinct indirect objects, or 64 MiB of string values, fetched for widgets) was spent before this page's widgets were all read: the rest are not counted, and `limit-hit` is set |

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
and, for a few fixed prefixes of the message's format string, the stage. Messages are never
formatted for facts (their arguments are not read) and never kept.

| value | meaning |
|---|---|
| `syntax-warning`, `syntax-error`, `io-error`, `unimplemented`, `internal-error` | poppler error categories |
| `bad-content-stream` | content-stream errors (unknown operators, wrong operands, unbalanced state) |
| `missing-font` | text shown without a usable font |
| `broken-image` | bad image parameters or codec errors |
| `limit-hit` | a cap was reached (images, fonts, paths, rules, annotations, figure merging, the image colour-space budget when one of this page's images needed a refused lookup, an optional-content expression past 10,000 operands or the page's 200,000 optional-content operands, after which content counts as visible, marked content nested past 4,096 levels, a stream with more than 16 filters) |
| `non-finite`, `out-of-range` | a number was not finite, or beyond ±10^9 points |
| `xref-repaired` | the cross-reference table was rebuilt while this page was read |
| `xref-repaired-earlier` | an earlier page of the run rebuilt the table: an object the file holds twice (two bodies under one number) can now resolve to the other one, so this page's facts (any of them read from such an object, an image's `px` or `bytesPerPixel` say) may differ from a run of this page alone. A rebuild while the document was opened does not set it (every range sees that one; the doc line says `xref: repaired`) |
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
  The exceptions are documented above: `parseWarnings` of shared resources (and of a damaged
  file whose cross-reference table an earlier page rebuilt: the stream-length errors an intact
  table raises are not raised again), a page marked `xref-repaired-earlier` (any of its facts
  read from an object the file holds twice can differ; a router wanting page-local facts of
  such a document should run the page alone), and the
  run-wide caps (100,000 images, 200,000 rules, the form budget, the figure-merge and image
  colour-space budgets), past which `limit-hit` is set; past the 200,000-rule budget a page's
  grids also lose their thin-rule counts (`gridsTruncated`), as do a page's grids once the
  cross-reference table was rebuilt. One more, older than issue 43: at levels that read the
  catalog's outlines, names or page labels (`full`), a reference there to a missing or broken
  object rebuilds the table before the first page is drawn, so `pdftotext -page-facts-fd` can
  write different XHTML than the same run without facts on such a file.

## Hostile input

- Every list and string is capped (`truncated`, `itemsTruncated`, `clustersTruncated`,
  `gridsTruncated`).
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
  `utils/Confine`). seccomp also denies, on any kernel, what Landlock leaves open or covers
  only from a later ABI: chmod, chown, utimes and their variants, truncate and ftruncate,
  fallocate, extended attributes, unlink/rename/mkdir/link/symlink/mknod, signals to other
  processes (kill and tgkill reach only this process; tkill, sigqueue and pidfds never),
  System V and POSIX IPC and memfd_create, resource limits, priority, scheduling and memory
  placement of any other process (prlimit64, setpriority, sched_set*, ioprio_set,
  migrate_pages and move_pages only for pid 0 or this process), sessions and process groups,
  namespaces, keyrings, flock, new processes (fork, vfork, clone without CLONE_THREAD;
  clone3 is ENOSYS, threads still work) and every ioctl outside TCGETS, TIOCGWINSZ,
  FIONREAD, FIONBIO, FIOCLEX and FIONCLEX (neither tool forks; a strace of both on
  Ghostscript.pdf shows no clone, fork or ioctl after confinement). Landlock ABI 1 (Linux 5.13) is enough for
  `require`; ABI 2 adds cross-directory link and rename checks, ABI 3 truncation, ABI 4 TCP,
  ABI 5 device ioctls and ABI 6 signal and abstract-socket scoping, all but device ioctls also
  denied by seccomp (tested with a probe, `test/docslurp/page-facts/confine_probe.cc`). It must still run inside the credential-free, network-blocked VM boundary
  DocSlurp requires for customer files.
- pdftomarkdown's `--page-facts FILE` must not exist, is reached without following symbolic
  links, is created mode 0600 under a temporary name and renamed into place as soon as
  pdftotext has finished (before image extraction), holding whole lines only (an
  unterminated last line is dropped); a FILE without a run line is incomplete.
  The descriptor is inheritable only while pdftotext starts; a caller's `--page-facts-fd`
  descriptor cannot be marked close-on-exec from safe Rust, so pdfregions (and pdfpagefacts)
  close every descriptor above 2 at start.
- The structure and form walks fetch each shared object once: widgets' indirect `/Parent`,
  `/V`, `/FT`, `/Ff`, `/AP` through a per-document cache that keeps only what the facts test
  (a string value is never copied per widget, and generation variants of a compressed object
  are one entry) and stops fetching at its budget (forms `truncated`), a name tree's shared
  `/Names` and `/Kids` arrays once per walk, and the `/Prev` walk within a byte budget. The
  XFA config read takes each packet stream once, through at most one filter.
- Drawing paths cost about what the text pass costs (tested in
  `test/docslurp/page-facts/test_cost_bounds.py`: facts CPU within the pass + 3 s and 3x
  the pass on each hostile case): an image XObject's dictionary is read once per document,
  and a colour space (or an ICC profile's `/N`) reached by reference is classified once per
  document without copying its lookup string, within a budget of 65,536 fetches (then
  `other`, with `limit-hit` on each page that draws an image whose own lookup was refused,
  including when that image is drawn again); poppler keeps optional-content visibility by
  reference (an OCMD and each indirect `/VE` sub-expression) until a group's state changes,
  so an OCMD named by many `BDC /OC` or XObject `/OC` entries is evaluated once; it evaluates
  a `/VE` expression visiting each shared indirect level once, at most 10,000 operands (then
  visible, `limit-hit`), and never reuses a sub-expression cut short by the 50-level
  recursion limit; what is not cached (a direct OCMD) is charged to a budget of 200,000
  operands per page, after which uncached `/OC` counts as visible, with `limit-hit`. Whether
  content is hidden is O(1) per `BMC`/`BDC`/`EMC`, and marked content nested past 4,096
  levels is not tracked (`limit-hit`; content inside takes the visibility of the deepest
  tracked level). A stream's `/Filter` or `/DecodeParms` given by reference is fetched once
  per document (an XObject is fetched again on every `Do`, so a large shared filter array
  was parsed per draw), and a chain of more than 16 filters is refused (an empty stream,
  `limit-hit`). These hold for plain pdftotext too. Poppler's errors are
  classified from their unformatted format strings; figure clusters merge by union-find over
  a sweep, within a run budget; the rule-grid score is a sweep over x with a Fenwick tree.
- pdftotext has no time limit by default; `-timeout S`, `-timeout-per-page P` and
  `-timeout-max M` bound the run to S + P x pages of the range, at most M seconds (exit 6,
  no run line). Facts lines written are whole, except the last one when the limit falls while
  a facts write is blocked (a reader that stalled): it can end without its newline, so a
  reader must drop an unterminated last line; pdftomarkdown drops it before committing
  `--page-facts FILE` (and removes a FILE left with no whole line). pdftomarkdown always passes `-timeout 60 -timeout-per-page 1
  -timeout-max 3600` (with or without page facts; `--text-timeout`, `--text-timeout-per-page`
  and `--text-timeout-max` override them).
- A facts reader that closes its end does not stop the conversion: pdftotext blocks SIGPIPE
  around facts writes, finishes the XHTML and exits 4; pdftomarkdown keeps the Markdown and
  exits 6.
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
