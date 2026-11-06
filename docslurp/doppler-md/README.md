# doppler-md

Markdown from DocSlurp doppler `pdftotext -bbox-layout -wordmeta -rules` XHTML. Started as a
byte-for-byte port of `doppler_md.py` (DocSlurp app, `performance/doppler-2026-10-26/`, v24).

```sh
doppler-md [--html] [--stream | --window N] [--params JSON] [--report] [FILE|-]
pdftomarkdown [--html] [--strict] [--stream | --window N] [--report] [--params JSON] [-f N] [-l N] [--opw PW] [--upw PW] [--] FILE.pdf
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

## Streaming

Both binaries read their input in 64 KiB pieces and parse pages as they complete; the raw
XHTML is never held whole. Two decisions look past one page: the body font size and colour
(the most common in the document) and running headers and footers (margin lines repeated on
`furniture_min_share` of the pages). By default both see the whole document, so every page
waits for the last one; output is unchanged.

`--window N` decides them from a window instead: the body size from the first N + 1 pages,
fixed after that, and headers and footers from the N pages either side of each page. Each
page is written (and flushed) once the next N have arrived, so at most N + 1 parsed pages, and
the margin keys of N more, are held at a time. `--stream` is `--window 16`. A document of at
most N + 1 pages converts exactly as without a window. The library API is
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
