# Doppler bar charts and tables

## Decision

Use repeated interior grid crossings, stable thickness, and aligned ticks to recover labeled bars. Use repeated lane separators together with adjoining populated cells to keep in-cell bars inside a table. Both detectors apply the same rules under their existing work budgets. This candidate retains the baseline requirement for a drawn axis, a minimum bar length spread, and three tick values.

## Direct PDF replay

| Corpus | Baseline 2ecc25b | Combined candidate |
|---|---:|---:|
| Synthetic charts detected | 1 of 17 | 9 of 17 |
| Negative cases emitted as figures | 3 of 14 | 0 of 14 |
| Synthetic HTML tables emitted | 7 | 7 |
| Markdown and PageFacts presence disagreements | 0 of 31 | 0 of 31 |
| Public-report pages compared in an offline VM | 72 | 72 |
| Public-report Markdown and PageFacts changed | — | 0 pages |

The baseline and candidate public outputs contained 49 vector report records (including grouped members) and 31 HTML tables. Economic Report PDF pages 49 and 50 are visually verified stacked-bar examples with a line series. Page 400 has a dense numerical table. The public sample demonstrates preservation for its pages. It contains no confirmed instance where the candidate recovers an otherwise missed chart, and the synthetic counts measure constructed stress cases.

## Experiments and decisions

Individual variants ran as the fixture matrix grew. Compare each variation against the same named input wherever possible. Use the frozen 31-case replay for aggregate comparisons.

| Experiment | Observation | Decision |
|---|---|---|
| Continuous crossings | Figures recovered with gridlines above and behind fills, with both bar orientations and rounded tips landing on gridlines | Keep |
| Repeated dividers alone | Rejected table-shaped false figures and also rejected a genuine chart with category dividers | Require adjoining populated cells |
| Repeated dividers with occupied cells | Recovered the chart with dividers. Ruled tables with data bars retained cells and emitted no figures | Keep |
| Stable thickness | Removing the check recovered a chart with a large thickness spread and accepted thin decorative marks | Keep on the labeled-bar recovery path |
| One crossing | Recovered a sparse-crossing case. There is insufficient evidence to replace two independent crossings on this sample | Keep two crossings |
| Tick alignment | Recovered a chart with a stray number. A monotonic sequence scattered across rows failed the alignment test | Keep on the labeled-bar recovery path |
| Affine tick fit and zero anchor | Coordinate probe found a close three-tick fit and a zero anchor both for a true chart and for in-cell bars. The irregular chart also admitted a locally near-linear triplet | Use as supplementary evidence, particularly for endpoint validation |
| No grid requirement | Recovered gridless and broken-grid positives. The expanded matrix includes a separate near-ruler negative of nearly identical geometry | Require stronger independent evidence before broadening |
| Join interrupted grid strokes by position | Recovered the broken-grid chart and accepted a decorative collection of fragments | Decline blind joining. A future variant can verify that fills explain interruptions |
| Relax minimum number of ticks | A two-tick chart is recoverable. Two numbers always determine a linear scale, so this offers limited protection from decoration | Keep three |
| Accept equal lengths | Recovered an equal-length chart. Equal-size status fills remain an ambiguity to test beside a plausible scale | Keep the spread gate |
| Infer an undrawn baseline | Geometry probe found a zero anchor for all four bar starts, also present in a true table | Keep the drawn-axis requirement for this candidate |
| Validate printed endpoint values | A position-to-value probe matched four of four tips on a gridless chart and zero of four after moving the printed values off calibration | Most specific gridless follow-up. It needs a real labeled example, more units and benign rounding controls |
| Mixed layouts | A figure and five table rows survived on the same page, and in-cell graphics stayed in their table | Keep candidate-local decisions |

## Next trial

Replay real labeled and gridless bar charts. Predeclare labels and chart bounds before scoring. Compare a position-to-value fit corroborated by at least three printed endpoints against the current candidate. Include a table with data bars and printed values, where the enclosing cell matrix should still determine ownership. For interrupted gridlines, verify that observed gaps line up with fill interiors before allowing segment joins.

## Real source provenance

Original bytes were reused from the local public-report corpus and processed only by the local offline QEMU guest. It had no network device or host mounts. Copies of these PDFs were excluded from the branch.

| Source | PDF pages | SHA-256 |
|---|---|---|
| [2025 Economic Report of the President](https://www.govinfo.gov/content/pkg/ERP-2025/pdf/ERP-2025.pdf) | 45–95, 395–405 | `ee118f2ee9138c33c3b7973880424b52a89bc5c053d1f40f12499dc4b4985d42` |
| [1974 Government Printing Office annual report](https://www.govinfo.gov/content/pkg/GOVPUB-GP-b90272bc30eea97e403b4f8c99f68d3f/pdf/GOVPUB-GP-b90272bc30eea97e403b4f8c99f68d3f.pdf) | 18–22 | `12062d9d0be9f646fd980a31fc32c243c2c35e4cbf4c5d32f057c17d6caf1772` |
| [NASA Human Integration Design Handbook, revision 1](https://www.nasa.gov/wp-content/uploads/2023/03/human-integration-design-handbook-revision-1.pdf) | 243–247 | `a6a8758b9116984d62fe4b7bc31fa9cf5da2479d57b7da9a3e31a00385ed303f` |
