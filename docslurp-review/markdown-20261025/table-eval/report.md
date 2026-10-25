# Table extraction evaluation

Implemented scorer: [usage, schema and metric definitions](../../../test/docslurp/table_scoring/README.md).

The retained batch has **26 comparisons: 17 pass, 9 fail, 0 invalid**. It includes the original outputs and separately labelled header-coverage variants. Paired interpretations of the ambiguous ruled fixture are sensitivity checks, not independent extraction runs. This is a small development set, not a held-out quality estimate.

## What the results mean

- Simple ruled and borderless synthetic outputs pass, including wrapped descriptions, bold headers and repeated labels. The architect rich header text contains literal Markdown emphasis; header words still satisfy semantic coverage, but presentation quality is not scored.
- Radical rich preserves the shared Revenue header through colspan. Strict has identical values but omits Revenue for the last column: header coverage is 2/3. Repeating Revenue in that column is an accepted representation, covered by regression tests and separately labelled coverage outputs. Original outputs remain available for comparison.
- Fidelity's no-repair rich output matches the drawn rules. Its repair output matches the explicitly annotated alternative. Each fails the other interpretation. The fixture cannot support a single unquestioned gold topology, so both results remain visible.
- Fidelity GFM omits the repeated shared title and loses rowspan/merged-cell content when flattened. It fails both interpretations. The lack of HTML spans alone is not a semantic failure.
- Minimalist's real AAPL tax table has seven columns instead of four, with year titles shifted left. Data F1 is 66.7%, but header coverage is zero. This is incorrect column association, not an alignment penalty.
- Fidelity's AAPL operations table merges three numeric columns into one and retains a footnote/footer inside the table. Data F1 is 37.8%; header coverage is 1/9. The header and shape failures remain visible even though many row-label words survive.
- The performance prototype excerpt contains no table syntax. It scores as a missing table. The excerpt is truncated, so this is not a full-page content-retention measurement.

## Results

Header coverage is per-column label recall. Data F1 uses exact normalized values in their aligned row and original column. The semantic summary is the minimum of header F1, data F1, shape similarity and empty-cell accuracy. It is a conservative engineering score, not a calibrated probability. Exact topology is reported separately in JSON and does not gate semantic acceptance.

| Candidate / reference | Gate | Header coverage | Data F1 | Semantic score |
|---|---|---:|---:|---:|
| minimalist-synthetic | PASS | 100.0% | 100.0% | 100.0% |
| architect-tagged-gfm | PASS | 100.0% | 100.0% | 100.0% |
| architect-tagged-rich | PASS | 100.0% | 100.0% | 100.0% |
| architect-untagged-gfm | PASS | 100.0% | 100.0% | 100.0% |
| radical-ruled-strict | PASS | 100.0% | 100.0% | 100.0% |
| radical-borderless-strict | PASS | 100.0% | 100.0% | 100.0% |
| radical-span-strict | FAIL | 66.7% | 100.0% | 80.0% |
| radical-span-strict-coverage | PASS | 100.0% | 100.0% | 100.0% |
| radical-ruled-rich | PASS | 100.0% | 100.0% | 100.0% |
| radical-borderless-rich | PASS | 100.0% | 100.0% | 100.0% |
| radical-span-rich | PASS | 100.0% | 100.0% | 100.0% |
| fidelity-borderless | PASS | 100.0% | 100.0% | 100.0% |
| fidelity-rich-repair-vs-drawn | FAIL | 100.0% | 86.7% | 86.7% |
| fidelity-rich-repair-vs-repaired | PASS | 100.0% | 100.0% | 100.0% |
| fidelity-rich-rules-vs-drawn | PASS | 100.0% | 100.0% | 100.0% |
| fidelity-rich-rules-vs-repaired | FAIL | 100.0% | 86.7% | 86.7% |
| fidelity-gfm-vs-drawn | FAIL | 75.0% | 88.9% | 85.7% |
| fidelity-gfm-coverage-vs-drawn | FAIL | 100.0% | 86.7% | 86.7% |
| fidelity-gfm-vs-repaired | FAIL | 75.0% | 81.5% | 81.5% |
| fidelity-gfm-coverage-vs-repaired | PASS | 100.0% | 100.0% | 100.0% |
| minimalist-aapl-tax | FAIL | 0.0% | 66.7% | 0.0% |
| minimalist-aapl-tax-coverage | PASS | 100.0% | 100.0% | 100.0% |
| minimalist-aapl-ops-coverage | PASS | 100.0% | 100.0% | 100.0% |
| minimalist-aapl-products-coverage | PASS | 100.0% | 100.0% | 100.0% |
| fidelity-aapl-operations | FAIL | 11.1% | 37.8% | 16.7% |
| performance-aapl-products-excerpt | FAIL | 0.0% | 0.0% | 0.0% |

## Provenance and reproducibility

- [Batch manifest](manifest.json), [all scores and mismatch details](results.json), [interactive report](report.html).
- [Reference annotations](references/) and [explicit source annotation script](build_references.py). The script contains hand annotations; it never reads candidate Markdown to create gold. Synthetic annotations were checked against retained PDF generators. AAPL tax and operations annotations were checked against page renders; the no-table products excerpt uses retained source layout text.
- Source PDF hashes, candidate/reference hashes and the scorer hash are retained. Original candidate outputs are retained alongside separately labelled coverage variants. Reference ambiguity and missing-table excerpts are explicitly labelled.
- [CTest validation](evidence/ctest.txt): scorer suite passed. The suite contains 33 tests, including 80 deterministic spacing/alignment variants and negative controls for missing headers, copied labels, wrong values, shifted cells, missing/extra rows, limits and invalid input.
- [Fresh tax-table crop](evidence/aapl-tax-closeup.png), [full page](evidence/aapl-page28.png), and [source layout text](evidence/aapl-pages26-28.txt). The source PDF is the cached 2025 AAPL 10-K identified by the annotation hash.

From repository root, with the scorer dependencies installed:

```sh
python test/docslurp/table_scoring/scorer.py \
  --manifest docslurp-review/markdown-20261025/table-eval/manifest.json \
  --output docslurp-review/markdown-20261025/table-eval/results.json
# Exit 1 is expected: the saved extraction outputs include quality failures.
python docslurp-review/markdown-20261025/table-eval/render_report.py
python docslurp-review/markdown-20261025/table-eval/check_results.py
```

No model judge, external recognition service or remote compute was used. References have been checked by the coordinator, not independently double-annotated. Further corpus work should include held-out human-labelled tables; these results do not establish production extractor accuracy. The scorer intentionally rejects unsupported nested/visual/CSS-driven table cases instead of assigning misleading quality scores.
