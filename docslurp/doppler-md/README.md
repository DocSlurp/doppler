# doppler-md

Markdown from DocSlurp doppler `pdftotext -bbox-layout -wordmeta -rules` XHTML. Started as a
byte-for-byte port of `doppler_md.py` (DocSlurp app, `performance/doppler-2026-10-26/`, v24).

```sh
doppler-md [--html] [--math] [--stream | --window N] [--params JSON] [--report] [FILE|-]
pdftomarkdown [--html] [--math] [--strict] [--stream | --window N] [--report] [--params JSON] [-f N] [-l N] [--opw PW] [--upw PW] [--] FILE.pdf
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
  a page background under a text layer, an image in the header or footer band, a sliver, a
  duplicate, or the text-sized pictures of a page that draws its words as pictures (more than
  12 inside its lines). The report's `images` lists every image the producer reported with
  `id`, `page`, `bbox` (page points, y down, as the words' coordinates), `px`, `kind`
  (`picture` or `mask`), `placed` (`block`, `inline`, `grouped`, `background`, `furniture`,
  `small`, `duplicate`, `fragment`), and `members` (the union `bbox` of a figure) or `group`
  (the figure a grouped image belongs to). A caller crops the page at `bbox` and swaps the
  marker for it.
- **Tables across pages.** A table that goes on from the last table of the previous page (the
  page starts with it, past an optional "(continued)" note; the columns sit in the same places)
  gets `<!-- doppler:table-continued header=repeated -->` above it when its first row repeats
  the earlier header, `header=inherited` when the earlier header was copied on top so the page
  stands alone, `header=none` otherwise. Drop the first row and its separator when merging
  `repeated` or `inherited` tables.
- **Paragraphs across pages.** `<!-- doppler:paragraph-continued -->` above a paragraph that
  goes on, in lowercase, from a last paragraph with no end of sentence. Join with a space, or
  without one after a hyphen.

The markers are ordinary Markdown and comments, so text of the PDF could imitate them: with
`--markers`, a zero-width space goes after `doppler` in any word that holds `doppler-image:` or
`doppler:`. A caller should still trust only ids that the report lists.

## Watermarks

Large pale or diagonal text and repeated backdrops are not content (parameter `watermarks`, on by
default; `0` restores the old output). Left in, a pale "CONFIDENTIAL" at 45 degrees became
stray one-letter headings and tables of letters, and a horizontal "DRAFT" was glued to the first
paragraph. A word goes when it is large (`watermark_size`, 1.5 body sizes) and any of:

- **pale**: a visible grey tint (luminance `watermark_min_lum` 0.72 up to, not including, white) not on a
  dark box; white text on banners and photos is kept;
- **diagonal**: three or more words of one font size on a line at least 15 degrees from both axes
  (a vertical label stays);
- **repeated**: the same word at the same place on as many pages as a running header needs
  (`furniture_min_share`, at least 2, over the same window; `--window 0` sees only the page
  itself and removes none, see [Streaming](#streaming)).

With `--markers`, a picture of at least 5% of the page at the same place and size on that many
pages gets no marker (`placed: watermark`). Small watermarks of one page that are dark and
horizontal, and one-page picture watermarks, cannot be told from content and stay. The report
gains `watermark_words` when any were removed.

## Columns and headers

Three parameters change the default output (`--params '{"name":0}'` restores the old one):

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
  (a quotation or listing in small type) is not. In one column, the evidence of
  `join_column_breaks` holds and the later half starts below at the column's left edge and has
  its left and right edges. Across a column break (with `join_column_breaks`) asides may sit
  below the earlier half, in regions of their own, or above the later half; the earlier column's
  bottom includes them.
- `merge_header_rows`: a header printed on several stacked lines comes out of row grouping as
  several sparse rows. When the first rows are tighter than the body's rows (1.3 times), hold no
  number and the body opens up below them, they are one header row (a `<thead>` with `--html`).

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

## Exit status and report

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

## Rotated text and wide-spaced borderless tables

Two passes go beyond `doppler_md.py` (golden-v24 does not have them):

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
