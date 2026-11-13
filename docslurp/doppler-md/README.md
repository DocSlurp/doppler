# doppler-md

Markdown from DocSlurp doppler `pdftotext -bbox-layout -wordmeta -rules` XHTML. Started as a
byte-for-byte port of `doppler_md.py` (DocSlurp app, `performance/doppler-2026-10-26/`, v24).

```sh
doppler-md [--html] [--math] [--stream | --window N] [--params JSON] [--report] [FILE|-]
pdftomarkdown [--html] [--math] [--markers] [--page-markers] [--strict] [--stream | --window N] [--report] [--params JSON] [-f N] [-l N] [--opw PW] [--upw PW] [--text-timeout S] [--text-timeout-per-page P] [--text-timeout-max M] [--images-dir DIR ...] [--] FILE.pdf
```

Pages are written to stdout joined by form feeds. `--params` overrides the tuning parameters in
`src/params.rs` (the same keys as `PARAMS` in `doppler_md.py`). `--html` writes tables as HTML
`<table>` blocks (escaped cells, `<thead>` only for a detected header row, `dir="rtl"` for
right-to-left tables) instead of GFM pipe tables; prose stays Markdown. The library takes the
same choice as `Options` in `to_markdown_report_with`.

## pdftomarkdown

`pdftomarkdown` is the supported way to get text out of a PDF; running `pdftotext` yourself,
or piping its XHTML into `doppler-md`, is deprecated (see the table in
[DOCSLURP.md](../../DOCSLURP.md#pdf-to-markdown)). `doppler-md` remains for converting XHTML
you already have.

`pdftomarkdown FILE.pdf` is PDF to Markdown in one step. It runs
`pdftotext -q -bbox-layout -wordmeta -rules -compact-layout` with `-f`, `-l`, `-opw` and `-upw`
passed through (`--opw`/`--upw` are accepted too) and converts its XHTML as it arrives.
pdftotext's stderr is passed through. pdftotext is `$PDFTOTEXT` if set, else a `pdftotext` next
to the `pdftomarkdown` executable, else `pdftotext` on `PATH`; it must be the DocSlurp fork.
Passwords are visible in the process list, as with pdftotext. `--` ends the options.

`-compact-layout` writes word coordinates at 0.001-point precision, which saves pdftotext time
(see PERF.md for the measurement and the quality check). A word that lands on the other side of
a layout threshold can change the output, so `--strict` drops it: `pdftomarkdown --strict
FILE.pdf` writes the same bytes as `pdftotext -bbox-layout -wordmeta -rules FILE.pdf - |
doppler-md`, the form the golden set and `parity.sh` use.

## Math (`--math`)

`--math` (library: `Options::math`) reads typeset math and writes it as LaTeX: `$...$` inline,
`$$...$$` for displays (with their equation numbers after them). `pdftomarkdown --math` runs
pdftotext with `-glyphs`, which adds a per-page font table and per-glyph records (font, code,
glyph name, where the Unicode came from, per-glyph baseline and size) after each word, named
glyphs that extraction drops as whitespace (`<xg>`), and Ghostscript's image-mask rules; see
`pdftotext(1)`. Without `--math` nothing changes: glyph records are skipped and the output is
byte-identical.

- ActualText that holds LaTeX (axessibility, accsupp) is used as is.
- Otherwise glyphs are math evidence by font (CMMI/CMSY/CMEX, AMS, Euler, txfonts/newtx,
  OpenType math fonts, Symbol) or code point; runs on lines and clusters of math lines become
  formulas, read as a baseline layout tree (scripts, limits, fractions and radicals from
  rules, accents, matrices/cases/aligned rows) from `src/math/`.
- Equation numbers separated from the formula (including split words or a separate PDF line)
  stay as text after it. Touching numeric parentheses remain formula content. Closely spaced
  barless stacks are read within their equation; derivation rows retain relation alignment.
- A formula with an unknown glyph (U+FFFD, private use, a raw-code guess or a Latin letter in a
  math font), too much doubt, or over a work cap keeps its words. Its page is listed in the
  report's `mathUncertainPages` (one-based), so a caller can send only those pages to a vision
  model. A formula never gets a symbol that no glyph drew.
- Incomplete glyph evidence (producer truncation, consumer caps, or rejected glyph records)
  preserves the page’s original words and marks it uncertain. Partial words are never rewritten
  from the surviving glyphs alone.
- Caps: pages over 5,000 lines, 5,000 rules or 60,000 glyphs are not read for math (listed in
  `math.skippedPages`); a formula has at most 1,500 glyphs, 200 bars and 200,000 work units.

The report gains `math` (`inline`, `display`, `actualText`, `uncertain`, `mathGlyphs`,
`glyphsLeft`, `skippedPages`, `glyphsSkipped`, `glyphsTruncatedPages`) and
`mathUncertainPages`. `scripts/math_eval.py` and `scripts/math_compile.py` measure robustness
and LaTeX validity on real PDFs; `DOPPLER_MATH_DEBUG=1` (or `2`, `3`) traces formulas to stderr.

## Restitching markers (`--markers`)

Pages are written separately, so a caller that wants the pictures back, or whole tables and
paragraphs that cross a page, needs to know where they were. `--markers` (library:
`Options::markers`) says so inline; `pdftomarkdown --markers` runs pdftotext with `-images`
(image dictionaries only: the image data is never decoded). Without it the output is
byte-identical.

- **Pictures.** `![image](images/page-0012/image-003.webp)`: the link is the path the picture
  gets under `--images-dir` (see [Image links](#image-links)). Its id `pN.M` (`p12.3` here) has
  `N`, the PDF page number, and `M`, the picture's position in pdftotext's list for that page,
  so the id does not change with the layout around it. A picture between paragraphs is a paragraph of its own, in reading order; a text-sized
  one inside a line of text is in that line, among the words. Neighbouring images (within 0.75
  body sizes, transitively) are one figure with one marker, at its first image. A full-page
  scan is a `page-scan` (see [Page scans](#page-scans)). No marker for
  a page background under a text layer (one picture, or a figure of pieces such as the strips
  or tiles of a scan, covering 85% of the page), an image in the header or footer band, a sliver, a
  duplicate, or the text-sized pictures of a page that draws its words as pictures (more than
  12 inside its lines). The report's `images` lists every image the producer reported with
  `id`, `page`, `bbox` (page points, y down, as the words' coordinates), `px`, `kind`
  (`picture` or `mask`), `placed` (`block`, `inline`, `page-scan`, `grouped`, `background`,
  `furniture`, `small`, `duplicate`, `fragment`), `scan` (how a scan's file is made: `jpeg`,
  `bilevel`, `decode`, `render`) on the images of a scanned page, and `members` (the union `bbox` of a figure) or `group`
  (the figure a grouped image belongs to). A caller crops the page at `bbox` and swaps the
  marker for it. A page that runs out of layout work is written as plain lines with its block
  markers after the text, top to bottom, and those images' entries gain `"degraded": true`.
  Placing pictures and drawings has a page budget of its own (`max_page_work`, charged to
  `max_doc_work`); a page that used it up is listed in `images_budget_pages`, and its remaining
  pictures are marked between paragraphs and its remaining drawings left `small`.
- **Vector figures.** `![image](images/page-0012/figure-001.svg)` (id `pN.gK`): a chart or
  diagram drawn with paths.
  `pdftotext -images` also writes `<graphic>` clusters of the drawing that is not axis-aligned
  (curves and diagonal segments; rules, boxes, grids, underlines and frames are left out).
  Clusters within a body size are one drawing; a drawing is a figure when it has at least 20
  diagonal or curved segments with 24 curves or diagonals in 3 of 8 directions (hatching and a
  lone slash point one way), is at least 2.5 body sizes long and one thick (a thin one, such as
  a row of arrows, counts when it has 6 paths and the rules it touches make it 2.5 thick:
  dotted leaders and braces do not), and lies outside the header and footer band. Its region
  then grows by the rules that touch it (not frames or panels around it, nor rules much longer
  than it), by short lines (at most 4 words) within a body size (tick labels, axis titles,
  legends) and by thin drawing it reaches, a step at a time; figures whose regions overlap are
  one. `K` is the position of its first `<graphic>` in pdftotext's list for the page. The
  marker is a paragraph of its own in reading order, and the text inside the region follows it
  as one paragraph, top to bottom (it leaves the page's flow). No marker for a figure under
  2.5 body sizes a side or 0.5% of the page, drawing among prose (its words cover more than 35%
  of it, or it holds two lines of 8 words or more: forms, worksheets, panels), or drawing over
  a larger picture (`grouped` with the picture); pictures inside a larger figure are `grouped`
  with it. Report entries have `kind: "vector"`, `px: [0, 0]`, `placed` (`block`, `grouped`,
  `small`, `furniture`, `text`) and `members` (the graphics marked together). Drawing made only
  of axis-aligned shapes (bar charts, box-and-line diagrams without diagonals) is covered only
  where it touches curved or diagonal drawing.
- **Tables across pages.** A table that goes on from the last table of the previous page (the
  page starts with it, past an optional "(continued)" note; the columns sit in the same places)
  gets `<!-- doppler:table-continued header=repeated -->` above it when its first row repeats
  the earlier header, `header=inherited` when the earlier header was copied on top so the page
  stands alone, `header=none` otherwise. Drop the first row and its separator when merging
  `repeated` or `inherited` tables.
  Notes do not hide a continuation (`stitch_past_notes`, on by default): the last table of a
  page counts when only notes follow it (up to four: small print, smaller than the body by
  `aside_size`, or a footnote mark; at most 400 characters each, no heading), such as a footnote
  or a page footer that repeats with different text, and up to two one-line notes may sit
  above the next page's table. More small print above a table is its title block, and the
  table is new.
- **Paragraphs across pages.** `<!-- doppler:paragraph-continued -->` above a paragraph that
  goes on, in lowercase, from a last paragraph with no end of sentence. Join with a space, or
  without one after a hyphen.
- **Page boundaries** (`--page-markers`, library `Options::page_markers`, independent of
  `--markers`). Each page starts with `<!-- doppler:page N -->` and a blank line (an empty page
  is the marker alone), `N` being the PDF page number (`-f`/`-l` runs name their real pages;
  without the producer's `number=`, `Options::first_page` plus the page's position). The report
  gains `page_numbers`, the pages written in order, so a caller can check a range or window run
  is whole. The form-feed contract holds with or without it: pages are joined by `\f`, chunk `k`
  (0-based) is PDF page `first + k`, and no page's Markdown holds a form feed (word text drops
  C0 controls).

### Image links

A link is the image's path relative to the images folder, after a prefix:

| Image | Path | Link (default prefix) |
|---|---|---|
| picture `p12.3` | `page-0012/image-003.webp` (`.png` with `--raster-format png`) | `images/page-0012/image-003.webp` |
| vector figure `p12.g1` | `page-0012/figure-001.svg` (`.webp`/`.png` with `--vector-format`) | `images/page-0012/figure-001.svg` |
| page scan `p12.1` | `page-0012/image-001.jpg` (a JPEG copied unchanged), `.png` (bilevel), else as a picture | `images/page-0012/image-001.jpg` |

Numbers are 1-based and zero-padded to at least 4 digits for the page and 3 for the index
(page 12345 is `page-12345`). Only `src/extract/layout.rs` maps ids to paths. The link depends
only on the id, on the producer's attributes and on options known when the Markdown is written
(`--images-link-prefix`, `--raster-format`, `--vector-format`, `--images-repeats`,
`--scan-pages`), never on what extraction later does,
so the Markdown is final before any image exists and is byte-identical with or without
`--images-dir`: a caller can index the text first and extract the images later (or never).
`--images-link-prefix P` (default `images/`) may be empty, or up to 128 bytes of
`A-Z a-z 0-9 . _ ~ / -`, relative (no leading `/`), with no empty or `..` segment, ending in
`/`; anything else exits 2. These five options are valid with `--markers` alone.

A picture that draws the same image object (`ref`) at the same size as an earlier marked one
(`--images-repeats skip` or `link`, the default `skip`) links to the earlier one's path and gets
no file of its own (manifest: `reason: repeat`, `repeat_of`), so a repeat never dangles. Both
placements must be plain, or their renders could differ: pdftotext's `orient="0"` (not turned
or mirrored) and `overlaid="0"` (nothing painted over it, no clip or page edge cutting it, no
soft mask, blend mode or opacity), no `smask=` and not an image mask (painted in the fill
colour). Any other placement keeps its own path and file. Repeats are found within the
converted pages: with `-f`/`-l` the first placement is the first one in that range, so a
repeat links into its own range's folder, and a caller that stitches ranges together merges
each range's images folder (the manifest's `obj` and `sha256` find repeats across ranges). With
`keep` every placement has its own path (a page scan repeats another only with the same
extension, and a rendered scan never does). The report's images give `path` (and `repeat_of`).
Whether a linked file was written is the manifest's to say: a link whose image was not extracted
(too small, over a budget, outside `--images-pages`) is listed `not-extracted` with the reason,
so the caller can drop the link or keep only its alt text.

The markers are ordinary Markdown and comments, so text of the PDF could imitate them, however
its words are joined (glued words, a hyphen across lines, table cells, math rebuilt from glyph
records). With `--markers` or `--page-markers`, the converter's own markers carry a random
per-run seal while a page is laid out. In the written page, first every `doppler:` and
`doppler-image:` (in any case) without the seal gets a zero-width space after `doppler`, and
every `page-` followed by four or more digits and `/image-` or `/figure-` (in any case, whatever
the prefix) gets one after `page`; then the converter's sealed picture markers are expanded into
their links, and the seal is removed. Since a Markdown renderer decodes escapes, entities and
percent signs in a link (`page&#45;0001`, `page\-0001`, `page%2D0001`, `./`), the link syntax
of the PDF's text is broken as well: a zero-width space goes between `]` and `(` and between
`]` and `:` (inline links, images, reference definitions), and after the `<` of every HTML tag
(any element can load a URL: `<img src>`, `<table background>`, `style="...url(...)"`,
`<mglyph src>`) and of every `<!` and `<?` (a comment, CDATA section, declaration or processing
instruction would hide the converter's links after it), except table markup as the converter
writes it (`<table>`, `<tr>`, `<td>`, ... with no attribute but `dir="rtl"`, `rowspan` and
`colspan`, which loads nothing) and the converter's own sealed comments. Only the converter can
therefore put a link, an image path or a `doppler:` comment into the Markdown. A marker whose
note was dropped (past 100,000 images in a document) keeps the neutral `doppler-image:<id>`
form: it has no manifest entry and no file. A caller should parse markers only from runs made with
`--markers`, match them case-sensitively, and still trust only ids and paths that the report or
manifest lists.

### Page scans

A page is a scan when pdftotext says so (`scan="single|tiled|layered" scanImage="K"` on
`<page>`, from the classifier it shares with page facts: images cover at least 85% of the page
(`--scan-min-coverage`) on a 64 x 64 grid and at most `--scan-max-words` (20) words are
visible; render mode 3 OCR text is invisible). Its scan image `pN.K` is placed `page-scan` before anything else (no
`background`, `watermark` or `block` for it), and the page's Markdown starts with

    ![page 12 scan](images/page-0012/image-001.jpg)

followed by a blank line and the page's text as usual (an invisible OCR layer included, so the
page stays searchable). A tiled scan (strips, tiles) or a layered one (a masked image, or a
layer on it covering at least half the page, `--scan-layer-overlap`: mixed raster content)
groups the page's other images of at least 1% of the page with the scan image (`grouped`), and
its box is their union. Other pictures on a scanned page keep their own placement.

The link's extension is decided at conversion from the image's dictionary and placement
(pdftotext's `copy=`, `orient=`, `overlaid=`, `copyable=`; the data is never read). A scan is
copied only when its pixels are what the page shows, decided by an **allow-list**, not by
looking for what might cover it: pdftotext writes `copyable="1"` on a single scan's image only
when, on that page, the one thing that paints is that image and everything else the page runs
is known not to paint or change how it looks (`poppler/ScanCopyAudit.h`):

- allowed: `q`/`Q`/`cm`; path construction ended by `n`; clips that are axis-aligned
  rectangles containing the scan; `BT`/`ET` and text state and positioning; text in render
  mode 3 (invisible), or 7 after the scan, in a font that is not Type 3; marked content
  (`BDC`/`BMC`/`EMC`/`MP`/`DP`) unless optional content hides the scan; colour, line and
  rendering parameters; `gs` with only `LW LC LJ ML D RI FL SA SM Font Type OPM TK`, `OP`/`op`
  false, `CA`/`ca` 1, `BM` Normal, `SMask` None, `AIS` false; form XObjects without a
  transparency group whose content follows the same rules;
- the scan itself: one image XObject drawn once, and one Gfx really draws (counted where it
  would be drawn: an image with a non-boolean `/ImageMask` or a missing, real or out-of-range
  `/BitsPerComponent` renders blank, so it is never copied), shown (no `/OC`), upright, wholly
  inside the clip in force and the CropBox (a rounding slack of at most a point and less than
  half an image pixel on each axis, so a tall strip squeezed onto the page cannot hide rows
  under a clip), full opacity, Normal blend, no transfer function or overprint,
  no mask or soft mask, `DeviceGray`, `DeviceRGB`, `DeviceCMYK` or indexed directly over them
  (the base named in the array, not by reference; a stencil mask only in black);
- anything else, at any depth (forms, annotation appearances), makes the page not copyable:
  any fill, stroke, shading, pattern or inline image, a second image, visible text in any
  mode, a Type 3 font at all, a clip that is not such a rectangle, a soft mask, a blend mode,
  an opacity below 1, a transparency group or page `/Group`, an annotation that can draw an
  appearance or a border in view or print mode (a `Print` `NoView` one included; only a link
  or popup without an appearance or border passes), an ICC-based, calibrated, Lab, separation or `Default` colour space,
  `/OutputIntents` on the catalog or the page (any at all: a viewer may show the page through
  their profile), or a `gs` key not listed above.

pdfregions runs the same allow-list again on its own pass before every copy job (`err:copy`
otherwise), so both must agree; the earlier overpaint estimate (`overlaid="0"`) stays as one
more refusal, never as the permission. A copyable, upright (`orient="0"`) scan whose stream is a
JPEG to copy (sole `DCTDecode`, 8 bits, `DeviceGray` or `DeviceRGB`, no `/Decode`, masks or
colour transform) is `.jpg`; a bilevel one (CCITT, JBIG2 or 1 bpc with an integer
`/BitsPerComponent 1`, or a stencil, `DeviceGray`, no `/Decode`; never JPEG 2000, which
decodes to 8 bits whatever its dictionary says)
`.png`; any other with a reference the raster format.

A document whose cross-reference table poppler rebuilt from a scan of the file (a broken
`xref` entry, at open or when a fetch meets it) copies no scan: one reference may then resolve
to another object body than a page draws. pdftotext marks the page where it noticed
(`xrefRebuilt="1"`, no `copyable="1"` from there on) and the end of the document
(`<xref rebuilt="1"/>`); scans to decode are rendered instead (same link), and a JPEG or
bilevel scan linked before the mark is not extracted (`reason: copy`, `error: xref`).
pdfregions refuses a copy job (`err:copy`) in a document whose table it rebuilt. Every other scan (tiled, layered,
turned, clipped, inline, covered by anything, or from a producer without those attributes) is
rendered at `--scan-dpi`, in the raster format, so the file shows what the page shows,
redactions included.

**Accepted cost:** a scan with anything visible beside it is rendered, not copied, even when
that thing is harmless: a printed page number or header, a Bates stamp, a white background
rectangle drawn first, a link with a border. Only invisible text (an OCR layer) may share a
copied scan's page. `--scan-pages` says what to do with scans:

| `--scan-pages` | Markdown | File |
|---|---|---|
| `image` (default) | `![page N scan](...)` first | the JPEG stream without its metadata (`source: copy`; Exif, ICC, XMP and comments dropped, so viewers do not turn it; an Adobe segment kept without JFIF, so viewers decode the colours as poppler does); bilevel rows as a 1-bit PNG (`decode`); other samples decoded once at native size (at most 16,777,216 pixels, else rendered) and scaled to `--scan-max-dpi` (300) before WebP q90 or PNG (`decode`); tiled, layered, turned and covered scans rendered at `--scan-dpi` (`render`) |
| `render` | the same, always the raster format | rendered at `--scan-dpi` (200, 36..300) |
| `skip` | placed as any picture (a background under text, else a block marker or a watermark) | none (`not-extracted`, `reason: scan-pages`) |

A copy is checked twice: pdfregions answers a copy job only for an image named in that page's
resources (directly or through forms at most 4 deep) whose dictionary still gives the same
class, and only a JPEG whose frame (SOF0..2, 8-bit) has the declared size and components and
which ends in EOI (at most 64 MiB, `-max-copy-bytes`), rewritten without metadata; a JPEG 2000,
JBIG2 or CCITT stream whose codec would decode another size than the declared one is refused
before decoding (`-max-copy-pixels`, which pdftomarkdown sets to 16,777,216); pdftomarkdown
checks the size and the markers again. A copy that fails is `not-extracted` with `reason: copy` (and `error`): it is
never rendered into a file of another extension. A copied JPEG is attacker bytes, as any file
here: serve it as `image/jpeg` with `nosniff`.

Scans have their own budget per batch, apart from the pictures' and figures': `--scan-max`
(1000 files) and `--scan-max-bytes` (1 GiB). A scan over it is `not-extracted`
(`reason: budget`, the batch's `status: budget`, exit 5; the batch counts as failed toward the
two-in-a-row abort) and the rest of the batch goes on; a
picture or figure over its own budget likewise leaves the scans after it in the batch going, so
a scanned book never starves the figures nor the reverse. The batch's renderer stops early only
when both pools are spent. The document ceiling
(`--images-doc-max-bytes`) counts both. The renderer's per-batch output cap is
8 x `--images-max-bytes` + 4 x `--scan-max-bytes` (64 MiB..64 GiB): decoded scans arrive as raw
samples. `--scan-pages` and the three thresholds join the images plan's settings digest.

The scan test's thresholds are pdftotext's, passed on unchanged; they change which pages are
scans everywhere (the Markdown, the images, the plan and `--page-facts`, whose doc line records
them as `scanThresholds` when any is not its default):

| option | default | |
|---|---|---|
| `--scan-min-coverage F` | 0.85 | the share of the page one image, or the images together, must cover |
| `--scan-max-words N` | 20 | a page with more visible words is not a scan |
| `--scan-layer-overlap F` | 0.5 | the share of the page a second image overlapping the scan must cover to make it layered |

A share is a plain decimal in (0, 1] (`0.8`, `.8`, `1`; no sign, exponent, `inf` or `nan`, at
most 30 characters); anything else is a usage error (exit 2) before anything runs.

**Performance** (synthetic 500-page books, 2550 x 3300 at 300 dpi, one image object per page,
whole-document `--images-dir`, this 8-core machine under other load; wall, CPU of the process
tree, peak RSS):

| Book | `--scan-pages image` (copy) | `--scan-pages render` (150 dpi WebP) | before (scans rendered as pictures) |
|---|---|---|---|
| grey DCT, 0.94 MB a page | 4.2 s, 4.8 CPU-s, 23 MiB: 500 `.jpg` (the stored bytes), about 7,200 pages a minute | 94.5 s, 124.5 CPU-s, 114 MiB: 500 `.webp` | failed after 21.9 s (`output limit`), 0 files |
| CCITT G4, 0.11 MB a page | 8.3 s, 9.6 CPU-s, 23 MiB: 500 1-bit `.png` (decode 7.7 CPU-s, PNG 1.1 s), about 3,600 pages a minute | 91.9 s, 122.6 CPU-s, 115 MiB: 500 `.webp` | failed after 24.8 s (`output limit`), 0 files |

("Before" is the previous release with `--params '{"watermarks": false}'`, since it otherwise
called a text-less scan repeated at one size a watermark and extracted nothing.)

**The copy allow-list** (2026-11-12; a 500-page grey DCT book, 2550 x 3300, 0.65 MB a page,
45 lines of invisible OCR text a page, median of 5 interleaved runs): every page is still
copied (500 of 500 `.jpg`, byte-identical to the stored stream, as with the overpaint
estimate before it), in 4.8 s wall and 6.6 CPU-s against 4.4 s and 5.9 CPU-s before (the
allow-list on pdftotext's pass and pdfregions' own page pass before each copy job: about
1.4 ms of CPU a page). The same book with a visible page number on every page is rendered
on every page both before and after (0 of 500 copied); at the new 200 dpi default that took
149 s wall, 196 CPU-s, 225 MiB (79 MB of WebP) against 85 s, 120 CPU-s, 145 MiB at 150 dpi.
After the 2026-11-12 review fixes (images counted where Gfx draws them, the half-pixel slack,
print-mode annotations) a regenerated 500-page grey DCT book (2550 x 3300, 0.96 MB a page, 45
lines of invisible OCR text) still copies 500 of 500 pages, every `.jpg` byte-identical to its
stored stream (5.9 to 8.1 s wall, 7.1 to 8.5 user CPU-s over 3 runs, the machine under other load).

## Watermarks

Large pale or diagonal text and repeated backdrops are not content (parameter `watermarks`, on by
default; `0` restores the old output). Left in, a pale "CONFIDENTIAL" at 45 degrees became
stray one-letter headings and tables of letters, and a horizontal "DRAFT" was glued to the first
paragraph. A word goes when it is large (`watermark_size`, 1.5 body sizes) and any of:

- **pale**: a visible grey tint (luminance `watermark_min_lum` 0.72 up to, not including, white;
  channels within `watermark_max_chroma` 0.12 of each other) not on a dark box, and rotated or
  drawn across other text of the page. White text on banners and photos, light colours (a gold
  or light-blue heading) and a light grey heading on its own are kept;
- **diagonal**: three or more words of one font size on a line at least 15 degrees from both axes
  (a vertical label stays);
- **repeated**: the same word at the same place on as many pages as a running header needs
  (`furniture_min_share`, at least 2, over the same window; `--window 0` sees only the page
  itself and removes none, see [Streaming](#streaming)), when it also looks like a watermark on
  the page (a grey tint, rotated, or drawn across other text) and every word of its line does.
  A heading repeated at the same place (a short report's, a slide deck's, "(continued)") stays,
  and a line is never left in pieces.

With `--markers`, a picture of at least 5% of the page at the same place and size on that many
pages gets no marker (`placed: watermark`). Small watermarks of one page that are dark and
horizontal, and one-page picture watermarks, cannot be told from content and stay. The report
gains `watermark_words` when any were removed.

## Columns and headers

Four parameters change the default output (`--params '{"name":0}'` restores the old one):

- `join_column_breaks`: a paragraph that runs to the bottom of one column and goes on at the top of
  the next is one paragraph (a hyphenated word is made whole). The earlier paragraph must have no
  end of sentence, run to its column's right edge and go down as far as the later region's
  content (within three lines); the later one starts in lowercase, higher up the page, in the same
  size, weight and colour; neither is a heading, list or caption.
- `join_interrupted_paragraphs`: a paragraph interrupted by a figure or a footnote and going on
  after it is one paragraph, with the interrupting content after it (the text reads on; a
  figure or footnote is a float that belongs after the paragraph that cites it). Interrupting
  content (asides) is a picture marker (`--markers`) or print at most `aside_size` (0.92) times
  the paragraph's size, never larger than body size print: figure labels, a caption, a
  footnote. One aside must be strong evidence: a picture, print at most `aside_tiny` (0.7) times
  the size (figure labelling), or a footnote (a mark: up to three digits and a space, `*`, `†`,
  `‡`, `§`, `¶` or a superscript digit) at the bottom of the earlier column; smaller print alone
  (a quotation or listing in small type) is not. The text evidence is that of
  `join_column_breaks`. In one column the later half starts below, at the column's left edge (or
  up to four sizes left of it, under a margin label), within its right edge. Across regions
  (with `join_column_breaks`) asides may sit below the earlier half, in regions of their own, or
  above the later half; the earlier column's bottom includes them, and past them the later half
  may also go on below the earlier one in its column. A column that the layout cut into regions
  (another column's text, or a figure's, beside it) goes on too: when the later half is the
  column's very next line (within half a size below), regions beside the column that end in
  nothing that could go on (no plain paragraph of two lines) may come between.
- `margin_labels`: a short label in the margin beside a paragraph (the word of a note or warning
  icon, a side label) that pdftotext read as the start of one of the paragraph's lines is a
  paragraph of its own before the paragraph, and the sentence it sat in is whole again ("split
  and di- TIP rectly deposited" was the output). The label is at most three words and
  `label_max_chars` (16) characters, two letters or more and no digit (not a number, list marker
  or bullet glyph), in another size, weight or colour than the line, followed by a gap of at
  least `label_gap` (0.5) sizes and 1.8 times the line's word spacing. The rest of the line
  starts at the paragraph's text edge (past icon marks on its earlier lines) with the label left
  of it, and the label sits inside a sentence: the paragraph so far has no end of sentence and
  goes on in lowercase or after a hyphen. A label that heads its paragraph (on the first line,
  or a run-in "Note:" before a capital) is left alone.
- `merge_header_rows`: a header printed on several stacked lines comes out of row grouping as
  several sparse rows. When the first rows are tighter than the body's rows (1.3 times), hold no
  number and the body opens up below them, they are one header row (a `<thead>` with `--html`).

## Tables drawn as cell boxes

Word processors often draw a ruled table as one filled box per cell (white or a shade, sometimes
on a dark background) between thin dark border rectangles drawn cell by cell, with a further
same-colour box behind each line of text inside a cell. The ruled pass took every box edge as a
row separator, so those line boxes cut each multi-line cell (a stacked header, a cell centred
beside several rows) into one row per line. `box_cell_tables` (on by default; `0` restores the
old output) reads such tables from the boxes instead:

- Boxes at least half a body size wide and 0.8 tall count (not page backgrounds or full-width
  bands); a box inside a box of the same colour is a line's background and is dropped, and a box
  mostly covered by boxes of other colours is a background behind the cells and is dropped.
- Boxes that touch across a border (3 points, or 0.6 body sizes when that is more) are joined.
  Full-width boxes at the top or bottom of a group (a banner, a note) are left out.
- A group is a table when its boxes tile a grid: 2 columns and rows or more (at most 40 and
  200), cells starting at each column edge in two rows or more, at most a quarter of the boxes
  spanning columns (a form whose rows split the width differently has many more), boxes
  covering 60% of the grid without overlapping, every line inside lying within its width, and
  two rows with two filled cells. A slot without a box is a cell of its own, spanning the rows
  of the cell beside it, so an unfilled column lines up with its neighbours.
- Every word goes to the cell box that holds it, at the cell's first row and column; a cell's
  lines are joined as a paragraph's are. A cell spanning rows leaves the rows below it empty
  in GFM; with `--html` the table keeps its `rowspan`/`colspan`.
- An unfilled header row above the boxes, under a dark rule across their width (0.8 to 4 body
  sizes above them), is the first row when its lines split into two columns or more without a
  line running across a column edge (a title).

The pass charges the page's work budget (each box, each comparison of the two sweeps down the
page, each grid slot and word) and is skipped on pages with more than 2,048 candidate boxes or a
pile of overlapping ones; other ruled tables are found as before. On the golden set it changes 2
of 258 inputs (the Ghostscript manual's shaded tables and the Apple 10-K's striped contents
list: multi-line cells whole, words no longer glued or split at line ends). On the real-PDF
differential (40 PDFs, pages 1 to 150) it changes 10 of 1,452 pages (NASA handbook tables with
stacked headers and row-spanning cells, an ERP table, two IRS instruction pages); the IRS forms'
field boxes, charts, org-chart boxes and photo labels stay as they were.

- `section_headings_after_tables`: a minor heading right after a table used to become the table's
  `*caption*`. One that starts with a multi-level section number (`4.6.3.2`, `2.1.`) and opens
  body text is the next section's heading (`### 4.6.3.2 ...`); a caption such as
  `Table 4.6-2 ...` stays a caption.

## Streaming

Both binaries read their input in 64 KiB pieces and parse pages as they complete; the raw
XHTML is never held whole. Two decisions look past one page: the body font size and colour
(the most common in the document) and running headers and footers (margin lines repeated on
`furniture_min_share` of the pages). By default both see the whole document, so every page
waits for the last one; output is unchanged.

`--window N` decides them from a window instead: the body size from the first N + 1 pages,
fixed after that, and headers and footers from the N pages either side of each page. Each
page is written (and flushed) once the next N are complete; the parser completes a page when
the following `<page>` opens. So at most N + 2 pages are held at a time (N + 1 in the converter
and the one being parsed), with the margin keys of N more. `--stream` is `--window 16`. A document of at
most N + 1 pages converts exactly as without a window.

**Do not use `--window 0`, and avoid very small windows.** Everything that finds content by
its repetition across pages looks only inside the window: running headers and footers,
repeated watermark words and backdrop pictures (`watermarks`), and, with `--markers`,
repeated header and footer pictures. With `--window 0` the converter sees one page at a time,
so none of these is removed: every running header, footer and repeated watermark lands in the
body text (only page numbers and pale or diagonal watermarks, which a single page shows by
itself, still go). The body size also comes from the first page alone, which shifts heading
levels when that page is a cover. Use `--stream` (16 pages), or no window when latency does
not matter. Cross-page continuation markers (`--markers`) are unaffected: they need only
the previous page.

The library API is
`model::Parser`, `model::decode_lossy` and `Converter` (`to_markdown_report_with` is a
`Converter` without a window).

On the 258-input golden set, `--stream` changes one document, the 477-page Ghostscript manual,
where running chapter headers ("Chapter 6. Using") on under 40% of the document's pages now
drop within their chapter instead of joining the body text. `--window 8` also changes the
26-page inaugural scan (heading levels, from a body size taken over its OCR'd cover pages);
`--window 64` changes nothing. On the Ghostscript manual, the first Markdown arrives after
about 0.1 s instead of when pdftotext finishes (4 to 6 s on a loaded machine), and peak
memory drops from 69 MB to that of pdftotext (about 25 MB) with or without a window.

With a window, pages already written stay written when pdftotext later fails: the exit
status, not the output, says whether the conversion finished.

## Extracting images and figures (`--images-dir`)

    pdftomarkdown --images-dir out/ --report paper.pdf > paper.md
    pdftomarkdown --images-dir out/ --images-dpi 200 --images-repeats link --vector-format webp paper.pdf

`--images-dir DIR` (implies `--markers`) writes a file for each marked picture (`pN.M`) and
vector figure (`pN.gK`) to DIR at the path its link names ([Image links](#image-links)),
rendered by `pdfregions` (`$PDFREGIONS`, else beside the executable, else on `PATH`; protocol:
`docslurp/extraction/PROTOCOL.md`). The Markdown is byte-identical to `--markers` without it and
is written first; then stdout is closed (pointed at `/dev/null`) and the folder is filled.
**EOF on stdout means the Markdown is complete and its links final**; the report (stderr) and
the exit status come after extraction. `--images-plan-out` / `--images-plan` (below) remain the
two-process alternative.

**Modes.** DIR and its page folders are created 0700 and files 0600, whatever the umask:
images can be tenant data in a shared VM. Copy or `chmod` them to share.

| Flag | Default | Meaning |
|---|---|---|
| `--images-dir DIR` | off | DIR must not exist; created 0700 (page folders 0700, files 0600) |
| `--images-link-prefix P` | `images/` | put before each path in the links (see [Image links](#image-links)); also with `--markers` alone |
| `--images-min-side N[px\|pt]` | `24px` | no file when the smaller side is under N (px: output pixels at the dpi; pt: page points) |
| `--images-repeats skip\|link\|keep` | `skip` | a repeat of an earlier marked figure: no file, `reason: repeat` and `repeat_of` naming the first (`skip` and `link` are the same since manifest v2; a repeat of the same image object links to the first's file) / a file and link of its own |
| `--raster-format webp\|png` | `webp` | pictures: lossy WebP, lossless when the crop holds text lines |
| `--raster-quality N` | `90` | lossy WebP quality |
| `--raster-lossless` | off | lossless WebP for every picture |
| `--raster-method N` | `4` | lossy WebP encoder effort (libwebp `method`), 0 (fastest) to 6; lower is faster with slightly larger files |
| `--lossless-method N` | `4` | lossless WebP encoder effort (libwebp `method`), 0 to 6 |
| `--lossless-quality N` | `50` | lossless WebP effort (libwebp `quality`), 0 to 100; with `--lossless-method` it trades encode time for file size, never pixels |
| `--vector-format svg\|webp\|png` | `svg` | vector figures; an SVG refused by the allowlist or over `--svg-max-bytes` becomes a lossless PNG wrapped in an SVG (`fallback`), so the `.svg` link holds |
| `--svg-precision 1\|2` | `1` | decimals of a point in SVG coordinates |
| `--svg-max-bytes N` | `512KiB` | largest compacted SVG kept |
| `--images-dpi N` | `150` | raster resolution, 36..600 |
| `--images-batch-pages N` | `64` | pages per extraction batch: one pdfregions process each, with the budgets below |
| `--images-max N` | `500` | most files per batch; past it the batch stops (`budget`) |
| `--images-max-bytes N` | `256MiB` | most bytes written per batch; past it the batch stops (`budget`) |
| `--images-doc-max-bytes N` | `4GiB` | most bytes for the document; past it every later image is `doc_budget` |
| `--images-deadline S` | 120 + 0.5 × pages, at most 3600 | time for the whole extraction (the default scales with the converted pages: 120.5 s for 1, 170 s for 100, 3600 s from 6960 on; a given value overrides); past it every later image is `deadline`, the files written stay, exit 5; the manifest's `deadline_ms` and the report's `images_deadline_ms` give the effective value |
| `--images-max-file-bytes N` | `8MiB` | one file larger is not written (`error: file_bytes`; an SVG first falls back to a raster); the rest go on |
| `--images-timeout S` | `120` | time per batch (rendering, compaction, encoding, fallbacks); at its deadline pdfregions' process group is killed and the batch stops (`timeout`); an encode that ends past it (or past `--images-deadline`) is dropped and its image is `timeout` (`deadline`), so one encode overruns by at most its own time |
| `--images-pages A-B` | all | only figures on pages A..B (1-based, inclusive; `N` for one page) are rendered; others get `skipped: pages`. The Markdown and markers still cover every converted page (context pages around a range) |
| `--images-resume-pages A-B` | all | with `--images-resume` only: retry only the resumable images on pages A..B (`N` for one page); the others keep their entries and stay resumable. Not bound into the manifest's `settings_sha256` (unlike `--images-pages`, which a resume must repeat), so one manifest can be resumed a range at a time; exit 5 while any image is left |
| `--images-confine require\|auto\|off` | `auto` | passed as pdfregions `-confine`; the report's `images_confined` says whether it was confined |
| `--scan-pages image\|skip\|render` | `image` | full-page scans (see [Page scans](#page-scans)); also with `--markers` alone |
| `--scan-dpi N` | `200` | rendered scans (tiled, layered, turned, not copyable, `--scan-pages render`), 36..300 |
| `--scan-max-dpi N` | `300` | a decoded scan finer than this is scaled down to it |
| `--scan-max N` | `1000` | most scan files per batch; past it a scan is `not-extracted` (`budget`) and the batch goes on |
| `--scan-max-bytes N` | `1GiB` | most scan bytes per batch, likewise |

**Two passes with an images plan.** A run that converts can also write the images plan
(`--images-plan-out FILE`, with `--markers` or `--images-dir`; FILE must be new and its folder
is reached without following symbolic links), and a later `--images-dir` run extracts from it
(`--images-plan FILE`) without running pdftotext or converting again: the same files and
manifest as extraction after the conversion. A plan names the PDF it was written for (SHA-256
and size), its `-f`/`-l`, a digest of the conversion settings (`--params`, `--strict`,
`--stream`/`--window`, `--math`, `--html`, `--scan-min-coverage`, `--scan-max-words`,
`--scan-layer-overlap`, compared by value) and the version; the extracting
run must be given the same PDF and options (and the link options above: `--images-link-prefix`,
`--images-repeats`, `--raster-format`, `--vector-format`, `--scan-pages`; the plan format is
version 6, which adds each image's scan source and the converted page count that scales the default `--images-deadline`), and refuses any difference, or a malformed plan (non-finite coordinates,
invalid or repeated ids, an id on another page than its line or outside the range), with exit 2.
A plan that cannot be written gives exit 6 after the Markdown.

**What is byte-identical.** Without `--markers`, `--page-markers` and `--images-dir` the
Markdown and report are the same bytes as before image links became paths. With them, the
links, file layout, manifest (now `manifest.jsonl`, `doppler-images/3`) and exit status 5 changed meaning in that release;
otherwise rendering and encoding are unchanged, so each image file has the same bytes as
before. Raw `pdfregions` SVG records may differ only in cairo's `source-N` ids (and so
in record length); pages with damaged fonts or streams render one job at a time. stderr is not
covered: poppler's syntax-error offsets and how often a repeated font warning is printed may
differ on damaged files.

Sizes take `K`/`KiB`, `M`/`MiB`, `G`/`GiB` suffixes. Bad values, an existing DIR, a symbolic
link anywhere in its path, or `-` (stdin) as input exit 2 before pdftotext runs.

**What gets a file.** Only marked images (`placed` `block` or `inline`) at least the minimum
side, and page scans (`page-scan`, whatever their size). No file for `watermark`, `furniture`, `duplicate`, `background`, `fragment`, `small`,
`text` and `grouped` (grouped images are inside their figure's file: the figure is rendered
at its union box). A repeat is a marked figure with the same box size (to half a point), pixel
size and kind as an earlier one and either the same image object (`ref`: known at conversion,
so its link is the first placement's path, and not rendered again; both placements plain, see
above) or, without a `ref` or not plain (vector
figures, inline images), byte-identical rendered output (found only at extraction: its own link
stays and the manifest says `not-extracted`, `reason: repeat`, `repeat_of`). Same-size charts
with different content are therefore never treated as repeats. When the first placement of an
image object lies outside `--images-pages`, the first repeat inside them writes its file (at
the shared path; the manifest entry is `extracted` with `repeat_of`).

**Files.** `DIR/page-NNNN/image-MMM.webp|png` and `DIR/page-NNNN/figure-KKK.svg|webp|png`,
named only from the ids (checked against `^page-[0-9]{4,10}/(image|figure)-[0-9]{3,7}\.(webp|png|svg|jpg)$`
with no extra leading zeros). Page folders are made with `mkdirat` (0700, must not exist) under
the folder's descriptor and opened with `O_NOFOLLOW`. SVG is parsed (roxmltree, no DTD), checked against an allowlist
(`svg g defs path use clipPath mask rect image symbol linearGradient radialGradient stop pattern
filter feColorMatrix`, their presentation attributes, references only `#id`, images only
`data:image/png;base64,` holding a whole well-formed PNG (IHDR first, 1..16384 px a side,
correct CRCs, only IHDR/PLTE/IDAT/IEND and tRNS/gAMA/cHRM/sRGB/bKGD/pHYs/sBIT, nothing after
IEND); `url(...)` in any case only as a whole value `url(#id)`, and no backslash in any value; no script, style, foreignObject, event
attributes, external URLs or entities) and rewritten from the parsed tree: coordinates rounded
to the precision in final page points (more decimals under an enlarging transform, one more on
dashed strokes; image placement keeps 2 decimals and transform scale factors 6 significant
digits), relative path commands where shorter, hex colours, defaults dropped where nothing
inherits another value, identical glyph definitions merged, short ids, shared styles hoisted to
groups, unreferenced definitions, empty groups and whitespace removed. A refused SVG figure's
fallback is `<svg width height viewBox><image width height href="data:image/png;base64,..."/></svg>`
around a lossless PNG of the crop, passed through the same allowlist (about a third larger than
the PNG; rare). WebP (libwebp) is a RIFF
with one `VP8 `/`VP8L` chunk; PNG has only IHDR/IDAT/IEND. Files are written to an `O_EXCL`
temporary name in a directory opened without following links, then renamed with
`RENAME_NOREPLACE`.

**manifest.jsonl** (schema `doppler-images/3`: JSON Lines, so a consumer reads it one line at a
time with fixed memory; written last, after the files are synced, to a temporary name that is
synced and then renamed into place without replacing anything; the same input gives the same
bytes; a DIR without a whole manifest is garbage). It is the source of truth: every link of the
Markdown has an entry, and every file in DIR is listed. One JSON object per line, ASCII (anything
else escaped `\uXXXX`), each line ending in `\n` and at most 64 KiB with it, keys in a fixed
order, the lines in this order:

```
{"type":"header","schema":"doppler-images/3","source_sha256":"…","settings_sha256":"…","link_prefix":"images/","pages":null,"dpi":150,"min_side_pt":11.52,"repeats":"skip","scan_pages":"image","scan_dpi":200,"scan_max_dpi":300,"batch_pages":64,"deadline_ms":121000,"resumes":0}
{"type":"image","id":"p1.1","page":1,"bbox":[72.00,222.00,272.00,372.00],"kind":"picture","fate":"block","obj":[6,0],"path":"page-0001/image-001.webp","link":"images/page-0001/image-001.webp","status":"extracted","format":"webp","source":"render","bytes":5468,"sha256":"…","width":417,"height":312}
{"type":"batch","pages":[1,64],"status":"ok","cut_page":null}
{"type":"end","complete":true,"resumable":false,"totals":{"images":1,…,"batches":1,"batches_cut":0},"images":1,"batches":1,"sha256":"…"}
```

The **header**: `schema`, `source_sha256` (the PDF), `settings_sha256` (what a resume must
repeat: the conversion settings an images plan is bound to, `-f`/`-l`, the link options, the
formats and their encoder settings, `--images-dpi`, `--images-min-side`, `--images-pages`,
`--images-max-file-bytes`, `--svg-*`, `--scan-dpi`, `--scan-max-dpi`; not the budgets, batch
size, time limits or confinement), `link_prefix`, `pages` (`--images-pages` as `[A,B]`, null
when not given), `dpi`, `min_side_pt`, `repeats`, `scan_pages`, `scan_dpi`, `scan_max_dpi`,
`batch_pages`, `deadline_ms` (the effective `--images-deadline`) and `resumes` (how many
`--images-resume` runs rewrote it). Then one **image** line per image (below), then one
**batch** line per batch of this run (a resume lists its own): `pages` `[A,B]`, `status` `ok`,
`budget`, `doc_budget`, `timeout`, `deadline`, `renderer` or `aborted`, and `cut_page`, the
first page with an image the batch did not get to (null for `ok`). The **end** line, last:
`complete` (every batch finished and no image was left by a stopped batch), `resumable` (some
image may still be extracted by a resume), `totals`, then `images` and `batches` (the number of
image and batch lines) and `sha256`, the SHA-256 (lowercase hex) of every byte of the file
before the end line. A reader refuses a manifest whose first line is not a `doppler-images/3`
header, whose lines are out of this order, whose end line is missing or followed by anything,
whose counts disagree (with the lines or with `totals`) or whose hash fails: a truncated or
altered manifest is never taken for a whole one. The Rust reader is
`doppler_md::extract::manifest::Reader` (one line held at a time). The version 2
`manifest.json` (one JSON object) is no longer written or read.

**Image** lines, sorted by page, pictures before figures, by index, after `type`:
`id`, `page`, `bbox` (points, y down), `kind` (`picture`, `mask`, `vector`), `fate`, `obj`
(`[num,gen]` of the image object, or null: with `sha256`, a key for deduplicating across runs),
`path` and `link` (null when unlinked; a repeat's are its first placement's), `status`
(`extracted`, `not-extracted`, `unlinked` for a fate without a link), `reason` for
`not-extracted` (`min_side`, `repeat`, `pages`, `bbox`, `id`, `file_bytes`, `encode`,
`scan-pages` (`--scan-pages skip`), `copy` (a scan copy that failed its checks: `error` `jpeg`,
`size`, `kind`, or the renderer's `copy`, `copysize`, `pixels`, `decode`), `error`
for a per-image renderer or record error, and the batch stops `budget`, `doc_budget`,
`timeout`, `deadline`, `renderer`, `aborted`), `resumable: true` for the reasons a resume
retries (`budget`, `doc_budget`, `deadline`, `timeout`, `aborted`: the batch stopped, not
anything about the image), `error` with `reason: error` (renderer `pixels`,
`render`, `empty`, `page`, `svgsize`, `renderer`; `mismatch` when a record does not match its
request in id, size within 1 px of box × dpi, or byte count; `utf8`), `repeat_of`, `format`
(the link's extension), `source` (`render`; for page scans also `copy`, a JPEG written
unchanged, and `decode`, samples decoded once; null without a file), `bytes`, `sha256`, `width`,
`height` (pixels; for SVG the box in points, rounded up; 0 without a file), and `fallback`
(`svg_allowlist`, `svg_size`, `svg_input` for raw SVG over 8 MiB) when it applies. pdfregions
gets `-max-svg-bytes 8MiB` (larger: `err:svgsize`, raster fallback), `-max-copy-bytes` of
`--scan-max-bytes` up to 64 MiB, and, per batch, `-max-output-bytes` of 8 × `--images-max-bytes`
+ 4 × `--scan-max-bytes` (64 MiB..64 GiB). The end line's `totals`: `images`, `linked`,
`extracted`, `not_extracted`, `unlinked`, `bytes`, `repeats`, `fallbacks`, `scans`,
`scan_bytes`, `batches`, `batches_cut`. `--report` gains `path` and `file` per image, `images_written`,
`images_complete`, `images_deadline_ms`, `images_confined`, with `--report-timing` (they change from run to run)
`images_ms` (`render`: the renderer processes' CPU;
`webp`, `png`, `svg`: encoding and compaction on this thread; `wait`: blocked on the renderer;
`copy`: checking and writing copied JPEGs; `decode`: converting and scaling decoded scans) and
`images_cpu_ms` (the renderers' user and system
CPU from `/proc/self/stat`), and `images_error` on a hard failure.

**Work bounds.** An SVG with more than 200,000 elements or references is not compacted (raster
fallback); compaction is linear in elements and references (100,000 `<use>` of one glyph:
0.19 s; 1,000,000 refused in 0.67 s) and stops at the batch's deadline.

**Batches, partial results and exit status.** Images are extracted in batches of
`--images-batch-pages` consecutive pages (page `p` is in batch `(p-1) / N`), one pdfregions
process per batch with its own `-timeout`, `-max-output-bytes` and file and byte counters, so a
long document is never cut by a whole-document cap. One image failing is recorded and the rest
go on. A batch that reaches `--images-max` or `--images-max-bytes`, times out, or whose
renderer fails (crash, signal, any non-zero exit including 3 limit and 5 cannot confine,
broken framing) keeps the files it wrote (a record is used only once whole), marks its other
images `not-extracted` with the reason, records `cut_page`, and the next batch starts with a
fresh budget. Two failed batches in a row, or three in all, mark the rest `aborted`: a failed
batch is one whose `status` is `timeout`, `deadline`, `renderer`, or `budget` because of the
scans' pool (`--scan-max`, `--scan-max-bytes`) or the protocol's job limit, since we do not
pay for more work on bad inputs; an `ok` batch ends a run of failed ones (but not the total).
A batch stopped by the figures' pool alone (`--images-max`, `--images-max-bytes`) is not
failed: it neither counts nor ends a run, and its images are `not-extracted`, `budget`,
`resumable: true` for a later `--images-resume`. The rest is marked `aborted`
without starting another renderer; `--images-doc-max-bytes` and `--images-deadline` mark the
rest `doc_budget` or `deadline`. Then the manifest is written, DIR is kept, and the exit status
is **5** (the end line's `complete: false`). Only a hard failure removes DIR, with exit status **6**: a write
failure (disk full, I/O error), a manifest that cannot be written, or a renderer that cannot be
started. The Markdown is written in every case. pdftotext failing exits as before and removes
DIR.

**Resuming** (`--images-resume DIR`, in place of `--images-dir`): give the same PDF and either
the same conversion options or `--images-plan PLAN`, and the same image options (the budgets,
`--images-batch-pages`, the time limits and `--images-confine` may change). DIR's
`manifest.jsonl` is read one line at a time and must be whole (a `doppler-images/3` header, its
end line, matching counts and hash: a truncated or altered one is refused), name the same PDF
(`source_sha256`) and settings (`settings_sha256`) and list exactly this conversion's images at
the same paths; otherwise nothing is touched and the exit status is **2** (also for a DIR
without a manifest, and for a version 2 `manifest.json` or any other schema: extract again). Only the
images marked `resumable` are extracted, in fresh batches with their own budgets, under the
document ceilings less the bytes DIR holds; every other entry stays as it was. Files go to the
paths the Markdown already links, so the Markdown (written to stdout again when converting)
never changes; they are created new, never replacing a file (one in the way is a hard
failure). The manifest is then written whole to a temporary name, synced, and renamed over the
old one in one step, with each entry's new status, the resume's own batches and `resumes` one
more. Exit status **0** when nothing is left (`complete`), **5** when the resume
stopped too; a resume with nothing to do changes nothing and exits 0 (5 if the manifest still
lists images a stopped batch left that are not resumable, such as `renderer`). A hard failure
(exit **6**) keeps the old manifest and every file that was there, and removes only the files
and page folders the resume made. Content repeats are compared within one run only: a resumed
figure whose pixels repeat one extracted earlier gets its own file rather than
`reason: repeat`.

**Manifest-only resume.** DIR may hold `manifest.jsonl` alone: a resume never reads or needs
the files of entries it keeps (a host that keeps them in object storage hands back only
the manifest), the document ceilings count the bytes the manifest records for them, and the
folder ends up with only the files this resume wrote, beside the replaced manifest.
`--images-resume-pages A-B` limits one resume to the resumable images of those pages.

**Performance** (one core, this machine): SVG compaction of 21 arXiv vector figures (cairo
SVG, 2.33 MB; 4 more hold JPEG data URIs and fall back to WebP) took 4.4 ms per figure and
kept 35.6% of the bytes at `--svg-precision 1` (41.0% at 2). Rendered with `rsvg-convert` at
300 dpi, 14.4% of inked pixels changed by more than 16 levels at precision 1 and 1.4% at
precision 2 (antialiasing on edges shifted up to 0.05 pt; no visible change). 188 raster crops
(47.8 Mpx): lossy WebP q90 21.9 ms per crop (11.6 Mpx/s) at 28.4% of PNG's size; lossless
WebP 67.9 ms per crop (3.7 Mpx/s) at 62.8%; PNG 13.2 ms (19.3 Mpx/s). Measure with
`cargo run --release --example image_bench`. Encoder effort is configurable: on a mixed set of
papers and reports, `--raster-method 2 --lossless-method 1 --lossless-quality 25` cut the
image pass's CPU by about 15% for about 1% more WebP bytes (lossy SSIM about 0.996 against
the defaults; lossless output decodes to the same pixels).

## Page facts (`--page-facts`)

    pdftomarkdown --stream --page-facts facts.jsonl [--page-facts-level full] paper.pdf > paper.md
    pdftomarkdown --stream --page-facts-fd 3 paper.pdf 3>facts.jsonl > paper.md

Page facts (schema `doppler-page-facts/1`: a doc line, one line per page, a run line; every
field in [docslurp/page-facts/README.md](../page-facts/README.md)) are computed by pdftotext
on the conversion's own pass (`-page-facts-fd`) and written unchanged: pdftomarkdown never
rewrites a line (it only drops an unterminated last one from FILE), so they are byte-identical to `pdfpagefacts` for the same range and
level, with or without `--stream`, `--markers` or `--math`. The Markdown is the same with or
without them. Facts never go to stdout.

| option | |
|---|---|
| `--page-facts FILE` | FILE must not exist and its folder is reached without following symbolic links; it is written mode 0600 under a temporary name and appears as soon as pdftotext has finished, before image extraction (also when the conversion failed: its lines say what was read; a FILE without a run line is incomplete). It holds whole lines only: an unterminated last line (pdftotext stopped mid-write at its time limit) is dropped with a warning, and a FILE with no whole line is not created |
| `--page-facts-fd N` | write them to descriptor N (3 or more), open for writing and inherited from the caller; pdfregions closes it (and every descriptor above 2) at start. Written unchanged: the reader must drop an unterminated last line (pdftotext stopped mid-write at its time limit) |
| `--page-facts-level basic\|full` | `basic` (default) or every family |
| `--page-facts-include LIST` | families added to basic: `fonts,oddtext,images-detail,vectors,forms,annots,ocg,structure` |
| `--page-facts-timing` | add `factsMs`, `totalMs`, `peakRssKiB` (not deterministic) |
| `--scan-min-coverage F`, `--scan-max-words N`, `--scan-layer-overlap F` | the scan test's thresholds ([Page scans](#page-scans)); a changed one is recorded on the doc line (`scanThresholds`), so the lines stay byte-identical to `pdfpagefacts` given the same `-scan-...` options |

pdftotext's time limit (see [Exit status and report](#exit-status-and-report)) applies with or
without page facts.

With `--stream`, a page's facts line is written when pdftotext finishes the page, before its
Markdown: join them by `page`. Not with `--images-plan` (no pdftotext runs). The descriptor
of `--page-facts` is inheritable only while pdftotext starts; the renderer never gets it.
A facts write that fails (a reader that closed its end, a full disk) does not stop the
conversion: pdftotext finishes the XHTML (its exit status 4), and pdftomarkdown writes the
Markdown and exits 6.
Cost: PERF numbers in [docslurp/page-facts/PERF.md](../page-facts/PERF.md).

## Exit status and report

- 5 (`pdftomarkdown --images-dir` or `--images-resume`): the Markdown was written and the
  images only in part; DIR and its `manifest.jsonl` are kept, its end line says
  `complete: false`, and its image and batch lines say why (`resumable` images may be finished
  with `--images-resume DIR`). A manifest without its end line is incomplete.
- 6 (`pdftomarkdown --images-dir`, `--images-plan-out` or `--page-facts`): the Markdown was
  written but image extraction failed hard (DIR removed), or the images plan or page facts
  could not be written.
- 2 (`pdftomarkdown --images-resume`): also a refused manifest (missing, truncated, altered,
  version 2 `manifest.json`, another schema, PDF or settings); nothing is touched.
- 0: converted. 2: bad arguments, or the input has no `<page>` (it is not doppler XHTML); the
  message goes to stderr. 1: an I/O error.
- `pdftomarkdown`: the same, with 2 when pdftotext's output has no `<page>` (the message names
  the pdftotext used, which is probably not the DocSlurp fork), and 1 when pdftotext is not
  found, cannot be run or exits non-zero. pdftotext runs with `-q`, so the message names its
  exit status instead: 1 could not open or decrypt the PDF, 3 refused by the PDF's permissions,
  6 ran past its time limit (below), 99 bad arguments (a first page past the last page of the
  document, or not the fork). Page numbers outside 1 to 2147483647 and `-f` after `-l` exit 2
  before pdftotext runs.
- pdftotext always runs under a wall-clock limit: 60 s plus 1 s per page of the range, at most
  3600 s (`-timeout 60 -timeout-per-page 1 -timeout-max 3600`), with or without page facts.
  Plain pdftotext still has per-draw costs that no other limit bounds (a large direct image
  dictionary is parsed again on every `Do`: 20,000 of them take about 30 s). Past the limit
  pdftotext exits with its status 6 and pdftomarkdown fails with exit status 1 (never its own
  6, which means the Markdown was written), no Markdown (with `--window` or `--stream`, the
  pages already written stay written), and the fixed message `pdftomarkdown: pdftotext ran
  past its time limit (status 6; see --text-timeout); no Markdown`. `--text-timeout S`,
  `--text-timeout-per-page P` and `--text-timeout-max M` override the three values (whole
  seconds, 0 to 86400; 0 turns that part off, and `--text-timeout 0 --text-timeout-per-page 0`
  runs pdftotext without a limit). `--images-plan` runs no pdftotext and refuses them. Image
  extraction is bounded on its own: every pdfregions batch gets `-timeout` from
  `--images-timeout` (and `--images-deadline`).
- `--report` prints one line of JSON to stderr: pages found and unclosed, lines and words kept,
  words skipped by reason (`missing_coord`, `bad_number` for a malformed or non-finite number,
  `outside_line`, `unclosed_line` for words of a `<line>` that never closed on a page), whether
  the input ended inside a line, rules skipped, the work used, and the pages read as plain
  lines with the reason. The library returns the same as `to_markdown_report`.
- `rules_truncated_pages` lists pages whose producer marked `rulesTruncated="1"`, even when
  no rules were kept. This is incomplete upstream rule evidence, independent of layout
  degradation; it does not force plain-line fallback. An absent marker, `"0"`, or an
  unrecognized value does not enter this list. `Page::rules_truncated` preserves `"0"` as
  `Some(false)`, `"1"` as `Some(true)`, and absent/unrecognized values as `None`.
  JSON page numbers in both `rules_truncated_pages` and `degraded` are **one-based**;
  library indices in `report.parse.rules_truncated_pages` and `report.degraded` are **zero-based**.

## Work limits

Layout and table analysis are superlinear on hostile pages (a grid of one-cell lines re-follows
every column gutter through every later band). A page over `max_page_lines` or `max_page_rules`
(20,000 each), or whose layout uses more than `max_page_work` units (10 million), is read as
plain lines in poppler order, one per Markdown line, without layout, tables or headings. Once
the document has used `max_doc_work` (50 million lines plus work units), the remaining pages are
read the same way. Gutter-table detection counts too: each run of rows that reaches column
detection charges twice its word count. The largest page in the 258-input golden set has 303
lines and uses 21,052 units (20,618 without the table charge), the largest document 59,725
(54,463). Every degraded page is listed in the
report. See PERF-ALGO.md for the measurements.

## Rotated text and wide-spaced or wrapped borderless tables

Three passes go beyond `doppler_md.py` (golden-v24 does not have them):

- **Rotated text** (`rotated_frames`, on by default). Words carry the producer's `rotation`
  (quarter turns; absent reads as 0). On a page, the kept lines of one rotation are read in their
  own reading frame when every word box of them is ordered (xMin <= xMax) and within a page's
  distance of the page, there are at least `rotated_min_lines` (3) of them, and at most
  `rotated_stray_share` (0.1 of them, rounded up; 0 allows none) other lines have their centre
  inside their extent:
  word boxes and baselines are turned back, the page's width and height swap for quarter turns,
  the rules touching them come along as a copy, and the ordinary layout runs on that frame under
  the same work budget. The result is placed on the page as one block at the lines' upright
  extent, so the text around it keeps its order; the stray lines stay upright. A whole rotated
  page is the same case. With more lines inside (rotated labels among upright text) the rotated
  lines keep the upright treatment, as before.
- **Wide-spaced borderless tables** (`sparse_tables`, on by default). After the dense gutter-table
  pass, the rows it left are tried again: a run of at least `sparse_min_rows` (3) consecutive
  multi-cell rows at most `sparse_row_gap` (5) font sizes apart, evenly spaced as a whole
  (`sparse_pitch_ratio` 1.35 between neighbouring gaps, twice that for a header gap). A final row
  after a wider gap (a total, a signature line) does not spoil the rows above it: it joins the
  table if its cells fit the columns and otherwise ends the run. The columns are the gutters the
  run's rows share and a row may leave a column empty. The run becomes a table only whole (every
  word inside a column, no column holding two cells of a row, every row its own grid row, every
  column filled in two rows or more, and no multi-cell row within twice `sparse_row_gap` outside
  the run that fits its columns); otherwise its rows stay as before rather than becoming a
  partial table. Not tables:
  - a first column of list or note markers: `1.`, `a)`, `12a.`, bullets, footnote symbols
    (`*`, `†`, `‡`, `§`, `¶`), letters counting `a`, `b`, `c` beside cells without numbers, or
    bare section numbers (`4.1`) beside prose;
  - a first or last column counting 1, 2, 3 with no number in any other column (footnotes and
    short legal notes, verse or pleading line numbers beside captions); a rank table, with
    values beside its index, stays a table;
  - a table of contents: a page number or range ending every row, never decreasing, text in
    the other cells, and either a first column of section numbers or a `Page` header. A sorted
    price list or a chronology with the year last stays a table only without that evidence: one
    whose first column counts 1, 2, 3, or whose header says `Page`/`Pages` (a count of pages per
    model, say), reads as a contents list and stays as lines. So do all-text lists numbered
    1, 2, 3 without a header (`1 | Brazil | Samba`); with a header (`Rank | ...`) they stay
    tables. Content is kept either way.

  A title above such a table becomes its header only when its words split into the table's
  columns. These guards apply only to this pass; the dense pass is unchanged.
- **Borderless tables whose cells wrap** (`wrapped_rows`, on by default; `0` restores the old
  output). A cell that wraps puts its next lines under the row's first line with nothing beside
  them; read as rows of their own, they broke the column gutters, the even spacing the sparse
  pass needs, and the page's reading columns (only a row's first line shares a baseline across
  a gutter), so the columns came out as separate text blocks. Now a row with fewer cells than
  the multi-cell row above it is folded into that row when each of its cells continues a
  different cell: inside that cell's column (it starts after the cell starts, ends before the
  next cell starts), lined up with the cell's last line (left or right edge or centre, or a
  hanging indent of up to two font sizes under the first line), in the same font size, at most
  `wrap_lead` (1.6) font sizes below it. Folding needs visible row padding: the rows on either
  side of a folded row, and the multi-cell rows on the whole, are at least `wrap_padding`
  (1.15) times the wrap step apart, and a step halfway to the row gap is a row of its own (a row
  with empty cells). Tables without padding are left to the dense pass as before. A folded
  row's wrapped text joins its cell as the lines of a paragraph do (dehyphenated); a wrapped
  word between two columns stays with its cell. Its wrapped lines count against the gutters
  they cross only where the gutter is narrow (a word space that the rows' first lines happen
  to share, as when every row starts with the same words), and only when two rows or more cross
  it, so a header spanning two columns keeps them apart. Row gaps are measured from a row's
  last line, in the dense and the sparse pass. A column gutter whose right side has wrapped
  lines no longer splits the page into reading columns when, with those lines left out, the
  rows still share baselines across it, the rows that wrap have three cells or more (two
  columns of short labels beside paragraphs stay a page layout), and the two sides are not
  both prose; rows further down the same gutter stay with it.

  Not tables, as before: two-column prose (two long cells a row), bulleted and numbered lists
  with wrapped items (a list marker column), key/value text whose values wrap into prose, and
  side headings beside paragraphs. On the golden set the pass changes 12 of 258 inputs (four
  documents in their tagged, untagged and stripped forms, and two real documents): a bank
  statement row and a manual's fault table whose wrapped cells were extra rows, a menu whose
  dish descriptions sat between rows, a receipt's dish names, a 10-K exhibit index (now one
  table with its exhibit numbers) and footnote, a consent letter's numbered list (no longer a
  scrambled table), and a wrapped bullet in the Ghostscript manual. Known side effects: a bold
  section heading right after a completed table can become its caption (the caption rule), and
  prose spacing across a table no longer split from the text above can merge two lines that
  were separate paragraphs.

These are layout decisions only: a false positive or a missed table changes how text is
grouped, never which text is kept. Layouts beyond the guarded ones can still come out as
tables (or stay lines) when their geometry looks like the other.

Allowed on purpose: two-column key/value blocks (labels and values, also label/blank form
pairs) and two-column checklists or signature blocks become two-column tables when their rows
are evenly spaced. A two-column bullet list is a list when its first column is the bullets; when
each bullet is part of its item's text there is no marker column and the pair of columns can
become a table. A table split across layout regions (some columns read as separate text) can
come out as a table of the columns its region holds.

On the golden set the pass changes 12 of 258 inputs, all from run-on or interleaved lines to
tables (receipt totals, form checklists, signature blocks, a purchase order's last rows, the
Ghostscript manual's task tables). Known caveats in the Ghostscript manual: in each task table
the last rows (`DISPLAY_DEV_*`, whose name and description share one cell) stay paragraphs
after the table, and a listing of a colour-management configuration file (a name, an ICC file
and four flags per line) becomes a six-column table.

## Output is untrusted Markdown

The Markdown carries text from the PDF as is. Raw HTML (`<script>`, `<img onerror=...>`), links
and images (`[x](javascript:...)`), and other Markdown syntax in the PDF's text pass through
unchanged; only table cells are escaped (`\` and `|`; with `--html`, `&`, `<`, `>` and `"` as
HTML entities), and line and paragraph separators in word text become spaces so text cannot add
lines or table rows. Prose is not escaped in either format. A renderer must treat the output as
untrusted input: sanitize it, or disable raw HTML and unsafe link schemes. DocSlurp's chat
renderer renders Markdown with `skipHtml`, which also drops `--html` tables.

## Testing

Byte comparisons identify changes to inspect. Faster implementations may change formatting
when tolerant Markdown/table scoring and content checks confirm that quality holds; byte
identity is not itself the acceptance requirement. See the [architecture validation](../../docslurp-review/architecture-20261027/README.md)
for the measured baseline and scoring coverage.

- `cargo test --release`: units, synthetic fixtures, hostile inputs and the limits.
  `tests/pdftomarkdown.rs` runs `pdftomarkdown --strict` against the pipe when `$PDFTOTEXT` (or
  `../../build/utils/pdftotext`) is the DocSlurp pdftotext with `-compact-layout`, and is
  skipped otherwise.
- `python3 -I scripts/check_pipeline.py --pdftotext /path/to/pdftotext --converter target/release/doppler-md`:
  check producer rule completeness through the real XHTML-to-Markdown pipeline.
- `scripts/parity.sh [GOLDEN_DIR [SET]]`: byte-identical output against `doppler_md.py` on the
  258 golden inputs (default `/home/dan/data/docslurp-evals/doppler-md-golden`, set
  `golden-v24`).
- `python3 -I scripts/difffuzz.py --py PATH/doppler_md.py --seed N --count 1000`: seeded
  differential fuzz against the Python reference (Unicode case and final sigma, entities, line
  separators, malformed numbers, truncation, tight work limits).
- `scripts/make_adversarial.py`: hostile pages for timing and Rust-vs-Rust differential runs.

### Margin headings

Prominent, isolated numbered headings near body text can survive repeated-header filtering. Exact repeated running headers still drop. See [policy, measurements and confirmation limits](FURNITURE.md); `furniture_preserve_headings:false` disables the heading exemption.
