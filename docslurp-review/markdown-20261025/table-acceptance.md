# Markdown table acceptance and scoring

User requirement, 2026-10-25: judge table meaning and column header coverage generously. Variable whitespace, visual alignment, repeated header labels and flattened merged column titles are acceptable. Every source-provided header must be represented at least once for each column it describes. Use best effort; do not invent absent source labels.

## Normalize representation before judging

Parse the output table into logical cells before normalizing text. Ignore padding around pipes, runs of ordinary whitespace, line wrapping within a cell, separator widths, and left/center/right alignment markers. Decode escapes and entities through the relevant Markdown/HTML parser. Do not split on escaped pipes or pipes inside supported inline constructs. Do not collapse delimiters or remove empty cells: those can change which column owns a value.

Compare visible cell text and header associations, not Markdown byte strings or screenshot spacing. Preserve meaningful distinctions such as minus signs, decimal separators, units, dates and footnote markers. Whitespace normalization must not concatenate distinct numbers or erase meaningful code formatting.

## Header coverage is per logical column

Build the expected header path for each data column from the source: its group title, leaf label, and applicable units or period. Accept these equivalent representations:

- An HTML header spanning several columns, with its coverage expanded to each child column for judging.
- A group title repeated in each applicable Markdown column header.
- Combined headers such as `Revenue — Q1` and `Revenue — Q2`.
- Multiple header rows that clearly associate the group title and leaf label with every applicable column.

Repeated group titles, repeated column labels, and repeated table headers at page boundaries receive no penalty. Count a required label once per applicable column for coverage; extra legitimate repetitions neither lower the score nor inflate it. Repetition of a header in unrelated prose does not establish column coverage. An empty header cell beneath a clear spanning parent is acceptable; an unlabeled column without such an association is not.

For example, a source header `Revenue` spanning `Q1` and `Q2` may be exported as:

```markdown
| Region | Revenue — Q1 | Revenue — Q2 |
| :-- | --: | --: |
| EMEA | 1204.5 | 1311.0 |
```

Extra padding, different alignment markers, or a rich HTML equivalent with `colspan="2"` all pass the same semantic check. A single `Revenue` label attached only to Q1, with no group relationship to Q2, misses Q2 group-header coverage.

## Score separate dimensions

Report header coverage as covered source header-to-column associations divided by expected associations, after accepted normalization. Require full coverage for the header acceptance gate when the source labels are known; report unknown/ambiguous source associations separately rather than guessing a denominator. Source columns without labels do not require fabricated labels.

Independently assess retained data values, row/column assignment, reading order and table boundaries. Missing values, shifted columns, invented labels and incorrect associations remain errors. A table cannot pass solely by repeating all headers everywhere: labels must apply to the correct columns.

Report exact merged-cell topology and visual resemblance as separate fidelity dimensions. A GFM table that flattens spans while preserving header coverage and data associations passes the semantic table requirement; do not fail it merely because it lacks HTML spans. Do not silently mix visual-layout penalties into the semantic score. No aggregate weights or numerical quality claims are established by this review.

## Export behavior

Prefer a readable rectangular Markdown table when it can preserve data associations. Repeat shared labels or combine group and leaf labels to ensure coverage for every column. Use rich HTML spans where useful and supported. Preserve empty data cells and flag uncertain assignments. Repeated headers are allowed; avoid suppressing the only available label during deduplication or joining tables across pages.

## Required future scorer cases

Equivalent spacing/alignment variants; repeated group labels; combined group/leaf titles; HTML colspan versus flattened GFM; repeated page headers; escaped pipes; empty cells; units and signed numeric values. Negative controls: missing group label for one column, swapped values, a dropped empty cell shifting values, and unrelated repeated labels assigned to all columns.

Implemented in [the table scorer](../../test/docslurp/table_scoring/README.md), with [retained-output evaluation results](table-eval/report.md). The scorer supports textual GFM/HTML tables and reports unsupported visual or CSS-dependent cases separately.

## Exporter status (2026-10-25, second pass)

The GFM writers of the radical and fidelity prototypes now repeat a spanning cell's text in every column it covers. The fidelity GFM path also uses the repaired cell grid. The minimalist `xml2md.py` prototype now:

- places cells by x-position rather than by their order within a row;
- attaches `$` signs to their amounts;
- keeps one-line section labels (`Net sales:`) inside a table;
- folds wrapped header lines into one header row;
- repeats a centred group title (`Years ended`) under each value column.

New retained outputs score full header coverage (precision and recall 1.0) in [the evaluation](table-eval/report.md): `*-coverage` cases. The AAPL tax (p28), income statement (p32, 24×4) and products (p26) tables pass exactly. The historical outputs are kept unchanged beside them for comparison.

The scorer additionally treats `_` as a header separator (`Revenue_Q1`), and ignores a space before `%` after a digit, `)` or a dash.
