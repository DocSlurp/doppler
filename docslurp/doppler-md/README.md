# doppler-md

Markdown from DocSlurp doppler `pdftotext -bbox-layout -wordmeta -rules` XHTML. Started as a
byte-for-byte port of `doppler_md.py` (DocSlurp app, `performance/doppler-2026-10-26/`, v24).

```sh
doppler-md [--html] [--math] [--stream | --window N] [--body-pages N] [--body-size PT] [--body-color C] [--params JSON] [--report] [FILE|-]
pdftomarkdown [--html] [--math] [--strict] [--stream | --window N] [--body-pages N] [--body-size PT] [--body-color C]
              [--report] [--params JSON] [-f N] [-l N] [--context N] [--estimate-body] [--opw PW] [--upw PW] [--] FILE.pdf
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

- **Pictures.** `![image](doppler-image:pN.M)`: `N` is the PDF page number and `M` the picture's
  position in pdftotext's list for that page, so the id does not change with the layout around
  it. A picture between paragraphs is a paragraph of its own, in reading order; a text-sized
  one inside a line of text is in that line, among the words. Neighbouring images (within 0.75
  body sizes, transitively) are one figure with one marker, at its first image. No marker for
  a page background under a text layer (one picture, or a figure of pieces such as the strips
  or tiles of a scan, covering 85% of the page), an image in the header or footer band, a sliver, a
  duplicate, or the text-sized pictures of a page that draws its words as pictures (more than
  12 inside its lines). The report's `images` lists every image the producer reported with
  `id`, `page`, `bbox` (page points, y down, as the words' coordinates), `px`, `kind`
  (`picture` or `mask`), `placed` (`block`, `inline`, `grouped`, `background`, `furniture`,
  `small`, `duplicate`, `fragment`), and `members` (the union `bbox` of a figure) or `group`
  (the figure a grouped image belongs to). A caller crops the page at `bbox` and swaps the
  marker for it. A page that runs out of layout work is written as plain lines with its block
  markers after the text, top to bottom, and those images' entries gain `"degraded": true`.
  Placing pictures and drawings has a page budget of its own (`max_page_work`, charged to
  `max_doc_work`); a page that used it up is listed in `images_budget_pages`, and its remaining
  pictures are marked between paragraphs and its remaining drawings left `small`.
- **Vector figures.** `![image](doppler-image:pN.gK)`: a chart or diagram drawn with paths.
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

The markers are ordinary Markdown and comments, so text of the PDF could imitate them, however
its words are joined (glued words, a hyphen across lines, table cells, math rebuilt from glyph
records). With `--markers`, the converter's own markers carry a random per-run seal while a page
is laid out; in the written page, every `doppler:` and `doppler-image:` (in any case) without the
seal gets a zero-width space after `doppler`, and then the seal is removed. A caller should
parse markers only from runs made with `--markers`, match them case-sensitively, and still
trust only ids that the report lists.

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
XHTML is never held whole. Two decisions look past one page, and they are set separately:

- **Furniture** (`--window N`): running headers and footers (margin lines repeated on
  `furniture_min_share` of the pages), repeated watermark words and backdrop pictures, and with
  `--markers` repeated header and footer pictures, are judged over the N pages either side of
  each page. Each page is written (and flushed) once the next N are complete.
- **Body size and colour** (the most common, by characters): heading levels, emphasis and table
  decisions are relative to them, so they are fixed once, before the first page is written.
  `--body-pages N` estimates them from the pages up to the N-th with text (at most 2N pages
  read, so a scan without a text layer is not held). `--body-size PT` and `--body-color C`
  give them instead (the values `--report` prints as `body_size` and `body_color`).

Without `--window` both see the whole document, so every page waits for the last one. With a
window the body sample defaults to 16 pages with text. `--stream` is `--window 4` with that
sample. The converter holds the larger of the window + 1 and the body sample (plus the page
being parsed, and the margin keys of N written pages); a document no longer than both converts
exactly as without a window (a longer one with a window covering it does not: the body comes
from the sample). The sample is bounded in size too: it also ends once its pages hold 262,144
words, rules and glyph records (`BODY_SAMPLE_MAX_ITEMS`), so the worst case held before the
first page is written is about that many items (tens of MB), not 64 maximal pages. `--window`
is at most 64 and `--body-pages` at most 256. The report always has `body_size`, `body_color`,
`body_sample_pages` (pages read when the estimate was fixed), `body_sample_text_pages` and
`body_given`.

**Choosing a window.** Measured on 15 long PDFs (FURNITURE.md), with no difference in the
judged golden cases for windows of 4 or more. `--stream` uses 4 pages either side: it leaves
about 15% fewer running heads and footers than 16 (8 and 16 keep chapter-specific running
heads that do not reach the share within the larger window). 16 differs least from the
whole-document run (1.45% of lines against 2.00% at 4, and 0.096% against 0.116% of the whole
run's words missing) and changes one golden input against three; use `--window 16` when
agreeing with the whole document matters more than removing chapter heads. 2 pages also drops
lines that appear on two of five nearby pages (it changed 10 judged golden cases), and the
whole document keeps more running heads still. Most of what used to differ between small windows and the
whole document was the body size taken from front matter (the old sample was the first
window + 1 pages), not furniture. `--window 0` sees one page at a time and removes no repeated
content at all (only page numbers and pale or diagonal watermarks, which a page shows by
itself). Cross-page continuation markers (`--markers`) need only the previous page.

The library API is `model::Parser`, `model::decode_lossy` and `Converter` (`Converter::with`
takes a `Window`: furniture window, `BodySample`, a given body, and context pages;
`to_markdown_report_with` is a `Converter` without a window).

On the 258-input golden set `--stream` (window 4) changes three documents: the 477-page
Ghostscript manual (running chapter headers on under 40% of the document's pages drop within
their chapter, as they also do at window 16), and a 10-K (a company name over the statements drops)
and a US Code title (where "Page N" and section running heads sit). On the Ghostscript manual the first Markdown arrives about 0.05 s after the
start instead of when pdftotext finishes, and peak memory is that of pdftotext (about 25 MB)
instead of 69 MB; the converter alone stays at 16 MiB with a 32-page body sample.

With a window, pages already written stay written when pdftotext later fails: the exit
status, not the output, says whether the conversion finished.

### Page ranges and sharding (`--context`, `--body-size`, `--estimate-body`)

A host that converts a long PDF as page ranges in parallel can get the same bytes as one run
over the whole document with the same window:

1. Estimate the body once: `pdftomarkdown --stream --estimate-body FILE.pdf` prints
   `{"body_size":10,"body_color":"#000000","body_sample_pages":33,...}` and stops after the
   sample (it reads only the sample pages: about 0.1 s; a document shorter than the sample, or
   a run without a window, is read to the end and the estimate is over every page read).
   `--report` of any run that starts at page 1 prints the same values. Give them back exactly
   as printed (not rounded or within a tolerance); every estimate is a value `--body-size` and
   `--body-color` accept.
2. Convert each range with the same options (window, `--math`, `--markers`, `--html`), the body
   given, and context at least the window (the default `--context` is the window, at most
   64): `pdftomarkdown --stream --body-size 10 --body-color '#000000' -f 65 -l 128 FILE.pdf`.
   pdftotext reads pages 49 to 144 (clamped to the document); the context pages count towards
   repetition but are not written or reported (no Markdown, image notes, math or degraded
   entries). They are not laid out, except with `--markers`, where the pages before the range
   are laid out for continuation markers and their work is charged to `max_doc_work` (and
   `doc_work`), as in a whole run. The report's `pages`, `page_closes`, `lines`, `words`,
   `rules_truncated_pages`, `glyphs_truncated_pages` and `page_info` cover only the range; the
   parser's skip counters (`words_missing_coord`, `words_bad_number`, `words_outside_line`,
   `words_unclosed_line`, `rules_skipped`, `images_skipped`, `graphics_skipped`,
   `glyphs_skipped`, `fonts_skipped`, `unclosed_line_at_eof`) cover every page read, context
   included, so summed over ranges they can count a context page twice.
3. Join the ranges' pages with form feeds.

The report of a range has `"range":{"first_page":65,"context_before":4,"context_after":4,
"identity":[...],"doc_work_limit":50000000}`. `identity` lists why its pages may still differ
from the whole run:

- `body`: the body was estimated from the range's own pages (no `--body-size` and
  `--body-color`, and the range does not start at page 1, or ends before the sample is
  complete).
- `furniture`: no window, so repetition is judged over the whole document, which a range
  cannot see.
- `context`: fewer context pages than the window on a side where the document goes on.
- `markers`: with `--markers`, a table that may run on from before the context (continuation
  markers and inherited table headers follow it page to page; the leading context pages are
  laid out, unwritten, to carry it).
- `doc_work`: always for a range after page 1. The document-wide work budget (`max_doc_work`)
  is spent page by page, and a range cannot see what the pages before it spent. The ranges are
  identical when the sum of their `doc_work` stays within `doc_work_limit` (a whole run would
  then degrade no page for it); otherwise compare the whole run's `degraded` entries.

Measured locally on eight long PDFs (3,306 pages, 64-page ranges, `--context 4`, window 4 and
the estimated body), every page was byte-identical to the whole run. Context costs pdftotext
time for the extra pages (layout is skipped without `--markers`; these figures are without
it): with 64-page ranges, `--context 4` adds 6 to 12%
CPU over the same ranges without context and `--context 16` 31 to 39%; with 16-page ranges
29 to 38% and 92 to 121%; with 256-page ranges at most a few percent (FURNITURE.md).

## Extracting images and figures (`--images-dir`)

    pdftomarkdown --images-dir out/ --report paper.pdf > paper.md
    pdftomarkdown --images-dir out/ --images-dpi 200 --images-repeats link --vector-format webp paper.pdf

`--images-dir DIR` (implies `--markers`) writes a file for each marked picture (`pN.M`) and
vector figure (`pN.gK`) to DIR, rendered by `pdfregions` (`$PDFREGIONS`, else beside the
executable, else on `PATH`; protocol: `docslurp/extraction/PROTOCOL.md`). The Markdown is
byte-identical to `--markers` without it and is written first; the folder is filled after.

| Flag | Default | Meaning |
|---|---|---|
| `--images-dir DIR` | off | DIR must not exist; created 0700 |
| `--images-min-side N[px\|pt]` | `24px` | no file when the smaller side is under N (px: output pixels at the dpi; pt: page points) |
| `--images-repeats skip\|link\|keep` | `skip` | a repeat of an earlier marked figure: no file (`skipped: repeat`) / no file, and `repeat_of` names the first figure, which has the file / a file of its own |
| `--raster-format webp\|png` | `webp` | pictures: lossy WebP, lossless when the crop holds text lines |
| `--raster-quality N` | `90` | lossy WebP quality |
| `--raster-lossless` | off | lossless WebP for every picture |
| `--raster-method N` | `4` | lossy WebP encoder effort (libwebp `method`), 0 (fastest) to 6; lower is faster with slightly larger files |
| `--lossless-method N` | `4` | lossless WebP encoder effort (libwebp `method`), 0 to 6 |
| `--lossless-quality N` | `50` | lossless WebP effort (libwebp `quality`), 0 to 100; with `--lossless-method` it trades encode time for file size, never pixels |
| `--vector-format svg\|webp\|png` | `svg` | vector figures; an SVG refused by the allowlist or over `--svg-max-bytes` becomes lossless WebP (`fallback`) |
| `--svg-precision 1\|2` | `1` | decimals of a point in SVG coordinates |
| `--svg-max-bytes N` | `512KiB` | largest compacted SVG kept |
| `--images-dpi N` | `150` | raster resolution, 36..600 |
| `--images-max N` | `500` | most files; more fails the extraction |
| `--images-max-bytes N` | `256MiB` | most bytes written; more fails the extraction |
| `--images-max-file-bytes N` | `8MiB` | one file larger is not written (`error: file_bytes`; an SVG first falls back to a raster); the rest go on |
| `--images-timeout S` | `120` | time for everything after the Markdown is written (rendering, compaction, encoding, fallbacks); at the deadline pdfregions' process group is killed |
| `--images-pages A-B` | all | only figures on pages A..B (1-based, inclusive; `N` for one page) are rendered; others get `skipped: pages`. The Markdown and markers still cover every converted page (context pages around a range) |
| `--images-confine require\|auto\|off` | `auto` | passed as pdfregions `-confine`; the report's `images_confined` says whether it was confined |

**Two passes with an images plan.** A run that converts can also write the images plan
(`--images-plan-out FILE`, with `--markers` or `--images-dir`; FILE must be new and its folder
is reached without following symbolic links), and a later `--images-dir` run extracts from it
(`--images-plan FILE`) without running pdftotext or converting again: the same files and
manifest as extraction after the conversion. A plan names the PDF it was written for (SHA-256
and size), its `-f`/`-l`, a digest of the conversion settings (`--params`, `--strict`,
`--stream`/`--window`, the body options, `--context`, `--math`, `--html`) and the version; the extracting run must be given the
same PDF and options, and refuses any difference, or a malformed plan (non-finite coordinates,
invalid or repeated ids, an id on another page than its line or outside the range), with exit 2.
A plan that cannot be written gives exit 5 after the Markdown.

**What is byte-identical.** With the same options, pdftomarkdown's Markdown, report, image
files and manifest are the same bytes as the release before the shared SVG pass and the
decoder changes. Raw `pdfregions` SVG records may differ only in cairo's `source-N` ids (and so
in record length); pages with damaged fonts or streams render one job at a time. stderr is not
covered: poppler's syntax-error offsets and how often a repeated font warning is printed may
differ on damaged files.

Sizes take `K`/`KiB`, `M`/`MiB`, `G`/`GiB` suffixes. Bad values, an existing DIR, a symbolic
link anywhere in its path, or `-` (stdin) as input exit 2 before pdftotext runs.

**What gets a file.** Only marked images (`placed` `block` or `inline`) at least the minimum
side. No file for `watermark`, `furniture`, `duplicate`, `background`, `fragment`, `small`,
`text` and `grouped` (grouped images are inside their figure's file: the figure is rendered
at its union box). A repeat is a marked figure with the same box size (to half a point), pixel
size and kind as an earlier one and either the same image object (`ref`, not rendered again)
or, without a `ref` (vector figures, inline images), byte-identical rendered output. Same-size
charts with different content are therefore never treated as repeats.

**Files.** `DIR/<id>.webp|png|svg`, named only from the ids (checked against
`^p[0-9]{1,9}\.g?[0-9]{1,6}$`). SVG is parsed (roxmltree, no DTD), checked against an allowlist
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
groups, unreferenced definitions, empty groups and whitespace removed. WebP (libwebp) is a RIFF
with one `VP8 `/`VP8L` chunk; PNG has only IHDR/IDAT/IEND. Files are written to an `O_EXCL`
temporary name in a directory opened without following links, then renamed with
`RENAME_NOREPLACE`.

**manifest.json** (written last; the same input gives the same bytes): `version`, `dpi`,
`min_side_pt`, `repeats`, then `images`, sorted by page, pictures before figures, by index, one
per line: `id`, `page`, `bbox` (points, y down), `kind` (`picture`, `mask`, `vector`), `fate`,
`file` (or null), `format`, `bytes`, `sha256`, `width`, `height` (pixels; for SVG the box in
points, rounded up; 0 without a file), and when they apply `repeat_of` (link mode),
`fallback` (`svg_allowlist`, `svg_size`, `svg_input` for raw SVG over 8 MiB),
`skipped` (`fate`, `min_side`, `repeat`, `pages`), `error` (renderer `pixels`, `render`, `empty`, `page`, `svgsize`;
`timeout` when the deadline passed before the image was encoded; `file_bytes`; `repeat_target`
when a repeat's first placement got no file and no other placement of it has one — a
`repeat_of` always names an entry with a file;
`mismatch` when a record does not match its request in id, size within 1 px of box × dpi, or
byte count; `encode`, `utf8`, `svg_size`). pdfregions also gets `-max-svg-bytes 8MiB` (larger:
`err:svgsize`, raster fallback) and `-max-output-bytes` of 8 × `--images-max-bytes` (64 MiB..2 GiB). Then `totals` (`images`, `marked`, `written`,
`bytes`, `repeats`, `small`, `fallbacks`, `errors`). `--report` gains `file` per image and
`images_written` (and `images_error` on failure).

**Work bounds.** An SVG with more than 200,000 elements or references is not compacted (raster
fallback); compaction is linear in elements and references (100,000 `<use>` of one glyph:
0.19 s; 1,000,000 refused in 0.67 s) and stops at the `--images-timeout` deadline.

**Failures and exit status.** One image failing is recorded (`error`) and the rest go on. The
renderer failing (cannot run, crash, any non-zero exit including 3 limit, 4 write failure and 5 cannot confine,
broken framing, timeout), a cap
(`--images-max`, `--images-max-bytes`) or a write error removes DIR; the Markdown is still
written and the exit status is **5**. pdftotext failing exits as before and removes DIR.

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

## Exit status and report

- 5 (`pdftomarkdown --images-dir` or `--images-plan-out`): the Markdown was written but image
  extraction failed, or the images plan could not be written.
- 0: converted. 2: bad arguments, or the input has no `<page>` (it is not doppler XHTML); the
  message goes to stderr. 1: an I/O error.
- `pdftomarkdown`: the same, with 2 when pdftotext's output has no `<page>` (the message names
  the pdftotext used, which is probably not the DocSlurp fork), and 1 when pdftotext is not
  found, cannot be run or exits non-zero. pdftotext runs with `-q`, so the message names its
  exit status instead: 1 could not open or decrypt the PDF, 3 refused by the PDF's permissions,
  99 bad arguments (a first page past the last page of the document, or not the fork). Page
  numbers outside 1 to 2147483647 and `-f` after `-l` exit 2 before pdftotext runs.
- `--report` prints one line of JSON to stderr: pages found and unclosed, lines and words kept,
  words skipped by reason (`missing_coord`, `bad_number` for a malformed or non-finite number,
  `outside_line`, `unclosed_line` for words of a `<line>` that never closed on a page), whether
  the input ended inside a line, rules skipped, the work used, the pages read as plain
  lines with the reason, the body size and colour used (see Streaming), and for a page range
  the `range` object. The library returns the same as `to_markdown_report`.
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
