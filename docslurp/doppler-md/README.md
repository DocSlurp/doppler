# doppler-md

Markdown from DocSlurp doppler `pdftotext -bbox-layout -wordmeta -rules` XHTML. Started as a
byte-for-byte port of `doppler_md.py` (DocSlurp app, `performance/doppler-2026-10-26/`, v24).

```sh
doppler-md [--params JSON] [--report] [FILE|-]
```

Pages are written to stdout joined by form feeds. `--params` overrides the tuning parameters in
`src/params.rs` (the same keys as `PARAMS` in `doppler_md.py`).

## Exit status and report

- 0: converted. 2: bad arguments, or the input has no `<page>` (it is not doppler XHTML); the
  message goes to stderr. 1: an I/O error.
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
read the same way. The largest page in the 258-input golden set has 303 lines and uses 20,617
units, the largest document 50,508. Every degraded page is listed in the report. See
PERF-ALGO.md for the measurements.

## Output is untrusted Markdown

The Markdown carries text from the PDF as is. Raw HTML (`<script>`, `<img onerror=...>`), links
and images (`[x](javascript:...)`), and other Markdown syntax in the PDF's text pass through
unchanged; only table cells are escaped (`\` and `|`), and line and paragraph separators in word
text become spaces so text cannot add lines or table rows. A renderer must treat the output as
untrusted input: sanitize it, or disable raw HTML and unsafe link schemes. DocSlurp's chat
renderer renders Markdown with `skipHtml`.

## Testing

Byte comparisons identify changes to inspect. Faster implementations may change formatting
when tolerant Markdown/table scoring and content checks confirm that quality holds; byte
identity is not itself the acceptance requirement. See the [architecture validation](../../docslurp-review/architecture-20261027/README.md)
for the measured baseline and scoring coverage.

- `cargo test --release`: units, synthetic fixtures, hostile inputs and the limits.
- `python3 -I scripts/check_pipeline.py --pdftotext /path/to/pdftotext --converter target/release/doppler-md`:
  check producer rule completeness through the real XHTML-to-Markdown pipeline.
- `scripts/parity.sh [GOLDEN_DIR [SET]]`: byte-identical output against `doppler_md.py` on the
  258 golden inputs (default `/home/dan/data/docslurp-evals/doppler-md-golden`, set
  `golden-v24`).
- `python3 -I scripts/difffuzz.py --py PATH/doppler_md.py --seed N --count 1000`: seeded
  differential fuzz against the Python reference (Unicode case and final sigma, entities, line
  separators, malformed numbers, truncation, tight work limits).
- `scripts/make_adversarial.py`: hostile pages for timing and Rust-vs-Rust differential runs.
