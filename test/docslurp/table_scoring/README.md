# Table scorer

Deterministic, reference-based evaluation of textual GFM and HTML tables. This is
an evaluation tool, not a PDF extractor, an LLM judge, or a visual similarity model.
It implements DocSlurp's requirement to tolerate variable spacing/alignment and
repeated column titles while requiring source-header coverage for every column.

## Install and run

Use Python 3.10+ and an isolated environment with `requirements.txt` installed.
The parser is pinned; upgrade it together with its regression tests. The scorer
uses [markdown-it-py](https://markdown-it-py.readthedocs.io/en/latest/using.html)
in CommonMark mode with HTML, tables and strikethrough enabled. It neither
executes HTML nor fetches linked resources.

```sh
python -m pip install -r test/docslurp/table_scoring/requirements.txt
python test/docslurp/table_scoring/test_scorer.py

python test/docslurp/table_scoring/scorer.py \
  --reference reference.json --candidate extracted.md --table-index 0 \
  --output score.json

python test/docslurp/table_scoring/scorer.py \
  --manifest docslurp-review/markdown-20261025/table-eval/manifest.json \
  --output results.json
```

Exit status: **0** all scored cases pass; **1** at least one quality failure;
**2** invalid input, unsupported content, resource limit, dependency or I/O error.
Batch reports retain valid results alongside individually invalid cases. Invalid
cases do not receive invented zero quality scores. A missing extracted table is
a valid quality failure, with zero semantic score. Reports are written atomically.

`--format html` scores HTML directly. Otherwise raw HTML tables and GFM pipe
tables can coexist in Markdown. Tables inside fenced code do not count. Indices
are zero-based in document order. Choose each table explicitly in the manifest:
there is no favorable best-match selection or automatic joining across tables.
An out-of-range index reports a missing table and includes `tables_found` for
diagnosis. Paths in a manifest are relative to that manifest, not the working
directory. Reference and candidate content hashes, scorer hash/version, parser
version and Python version are retained in JSON.

CTest registers `docslurp-table-scorer` when its configured Python has the parser:

```sh
cmake -S . -B build -DDOCSLURP_TABLE_SCORER_PYTHON=/path/to/venv/bin/python
ctest --test-dir build -R '^docslurp-table-scorer$' --output-on-failure
```

When the optional dependency is missing, configure prints an explicit skip
message. Ordinary PDF regression tests do not acquire a Python package dependency.
On this workstation set `TMPDIR`, `TMP`, and `TEMP` to `/home/dan/tmp/agents`
when running tests; CLI regression tests use `tempfile` and clean their files.

## Reference format

```json
{
  "schema_version": 1,
  "id": "quarterly-revenue",
  "source": {"description": "Hand-verified PDF page 3, table 1"},
  "headers": [["Region"], ["Revenue", "Q1"], ["Revenue", "Q2"]],
  "rows": [["North", "10.5", "12.0"], ["South", "8.1", "9.7"]],
  "spans": [["header", 0, 1, 1, 2]]
}
```

Each `headers` entry defines one logical column's expected labels. A label is a
phrase, not a bag of interchangeable words. Include group titles, leaf titles
and units that actually apply; an unlabelled source column uses `[]`.
Reference labels must come from the source, never from a candidate's output.

`rows` is a rectangular array of visible strings; `""` is an explicit empty cell.
For spans, repeat the spanning cell's value in every covered logical slot. A
candidate's `rowspan`/`colspan` is expanded in the same way. Thus explicit repeated
labels and an HTML spanning cell can be semantically equivalent. Blank cells in
a flattened output are not silently forward-filled.

Optional `spans` entries are `["header"|"body", row, column, rowspan, colspan]`.
These describe physical merged cells, separately from semantics. Header row
positions are relative to the physical leading header section; body row positions
exclude identified repeated/supplemental headers. Omit `spans` when topology is
unknown. An empty list asserts that there are no merged cells. `notes` can explain
ambiguities. Unknown top-level fields, duplicate keys, invalid dimensions and
overlapping reference spans are errors.

A batch manifest has `schema_version: 1` and `cases`, each with unique `id`,
`reference`, `candidate`, optional `table_index`, `format`, and `notes`.

## Semantics and metrics

1. Parse the rendered structure. Markdown escapes, entities, inline emphasis and
   links become visible cell text. HTML text is not reparsed as Markdown: literal
   `**value**` in an HTML data cell remains literal. Header matching concerns
   label words, not typographic decoration, so literal header emphasis artifacts
   are outside its presentation scope.
2. Normalize NFC Unicode and ordinary whitespace, including nonbreaking spaces.
   Ignore padding, pipe alignment and delimiter widths. Normalize spaces between
   currency signs and numbers, or between digits and `%`. Preserve signs, decimal
   separators, numeric spelling, data case, and distinct numbers separated by
   spaces. Spaces before `%` after a digit, closing parenthesis or dash are
   ignored (`4 %`, `(6) %`, `— %`). Do not numerically round or coerce values.
3. Expand explicit HTML spans. Collect leading `th`/`thead` rows as headers;
   `th scope=row` is a data label. Recognize supplemental/repeated headers only
   when their nonempty text consists of known labels valid for their own columns.
   A row identical to a reference data row cannot be removed as a repeated header.
4. Match each required header phrase in its column after case folding and header
   punctuation/separator normalization. Underscores count as separators, so a
   flattened title such as `Revenue_Q1` covers both `Revenue` and `Q1`. Count a repeated association once. Wrong
   column associations and unknown label tokens lower precision; copying every
   title into every column cannot pass. Prose outside the table supplies no
   implicit header coverage. Duplicate source column titles, such as two distinct
   `Change` columns, are valid and assessed separately by position.
5. Align data rows monotonically by minimum cell edit cost. This permits useful
   diagnostics after a missing/extra row without sorting rows or permuting
   columns. Nonempty values match only in their aligned row and original column.

The report keeps these dimensions separate:

| Field | Definition |
|---|---|
| `headers` | Precision, recall and F1 over unique `(column, source label)` associations; unknown/wrong labels count as extras. Header coverage is recall. |
| `data` | Precision, recall and F1 over nonempty logical data slots, after order-preserving row alignment. Incorrect values do not match. |
| `shape_score` | Row-count similarity times column-count similarity, each `min/max`. |
| `empty_cell_accuracy` | Fraction of expected empty slots that remain empty in their aligned positions. |
| `topology` | Optional F1 over physical span tuples. Never gates semantic acceptance. |
| `semantic_score` | Minimum of header F1, data F1, shape score and empty-cell accuracy. A conservative bottleneck summary, not calibrated accuracy. |
| `passed` | All headers covered without extras, and normalized data grid exactly equal in shape, content and order. |

If no associations/values are expected and none are present, that dimension is
1. Missing nonempty expected content is 0. Up to 200 individual mismatches are
included; a truncation flag accompanies a capped detail list. Counts and scores
always use all cells. Repetition never increases recall above 1.

Example: `Revenue` spanning `Q1/Q2`, repeated `Revenue` in both child headers,
or `Revenue — Q1` / `Revenue — Q2` all satisfy the same header contract. GFM
flattening can pass semantics while scoring zero for merged-cell topology.

## Boundaries and limits

This is a textual grid scorer, not a full browser. It does not score font/style,
link destination correctness, CSS visibility/layout, images, or perceived visual
resemblance. Image-valued cells and explicitly hidden/CSS visibility-controlled
table content are reported as unsupported. Class-based/external stylesheets are
not applied. Whitespace-sensitive code cells require a separate verbatim adapter;
the parser rejects detected multi-whitespace code rather than silently treating
it as equivalent prose. Plain-text tables without recognized Markdown/HTML table
structure are missing tables, even if their numbers remain somewhere in prose.

Supported HTML uses explicit rows and positive integer spans. Common omitted
`td`/`tr` closing tags work. Nested tables, unclosed tables, ragged HTML rows,
overlapping spans and `rowspan=0` are rejected. GFM's supported missing trailing
cells become empty rendered cells. Excess pipe cells are rejected because GFM
would silently discard them. Parser errors are separate from extraction scores.

Limits: 8 MiB per input, 256 tables/columns, 10,000 rows, 100,000 expanded document
cells, 2 million row-alignment states and 20 million slot comparisons. Exact-grid
matches have a linear fast path. Over-budget comparisons return an explicit
error; split very large evaluations into independently annotated regions.

Reference correctness remains a human responsibility. Ambiguous sources need
separate named interpretations and sensitivity results; do not silently choose
the interpretation that gives a candidate its best score. The recent experiment
suite does this for the defective ruled-table fixture. It is a development set,
not a held-out estimate of general PDF extraction accuracy.
