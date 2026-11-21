# Doppler bar charts and tables

Replay the same PDF bytes on private Doppler 2ecc25b and each variant. Compare chart-region count and bounds in Markdown figure reports and PageFacts. Track duplicate output, HTML table and cell retention, unrelated figure changes, and work limits. Save stdout and JSON reports so each change can be attributed to a stage.

| Experiment | Isolate | Decision |
|---|---|---|
| Baseline | Labeled bars, grouped/stacked controls, genuine tables and mixed pages | Record distinct errors and stage |
| Crossings and boundaries | Repeated rules crossing bar interiors versus long category separators between lanes | Recover labeled bars only with positive geometry and retain tables |
| Thickness | Compare with and without a stable-thickness gate | Keep only if it protects a negative without needless chart losses |
| Tick geometry | Alignment, shared scale, nearby numeric text, irregular and sparse ticks | Evaluate independently before changing ordinary charts |
| Missing grid evidence | Above/below paint order, gaps, gridless charts, equal-length marks | Use supported evidence or leave ambiguous |
| Local ownership | Data bars inside a populated matrix and a separate nearby chart | Preserve table cells and independent charts |
| Cost and parity | Pathological rule/word counts, both native paths | Preserve existing work budgets and test parity |

Original fixtures are generated from geometric parameters. Replay real PDFs separately in a credential-free VM. Keep any pre-existing held-out documents out of tuning. Treat a small real sample as a smoke test, with no population-level accuracy claim.

## Run another arm

Use the same C++ producer build and Rust checker for each arm, with both set to the intended revision. Build the private C++ tools with the options in Doppler's build documentation and build `pdftomarkdown` with Cargo. Generate the original PDFs with `python3 test/docslurp/page-facts/bar_geometry_fixtures.py <input-dir>`. The persistent experiment directory contains `experiments/replay.py`.

```sh
python3 experiments/replay.py \
  --inputs synthetic-v2 --output replay/new-arm \
  --markdown /path/to/pdftomarkdown --text /path/to/pdftotext \
  --facts /path/to/pdfpagefacts
```

Label each new case before the run. For a table, record the rows, cells, and expected table bounds. For a chart, record the plot bounds and whether labels, axes, and legends should be included. Compare the emitted boxes, normalized table cells, and duplicates. Use exact output equality as a preservation check for unrelated real pages. The current Markdown output places chart figures and neighboring tables separately. Keep representative ambiguous cases where vector geometry lacks enough evidence.

For endpoint calibration, predeclare the tolerances in PDF points and the minimum independent matching endpoints. Include rounded, abbreviated, and negative values, nonzero baselines, log scales, and chart-like data-bar tables. For stroke reconstruction, compare one uninterrupted gridline, fragments matching fill occlusion, and unrelated strokes with the same coordinates. Replay any winning arm in both native paths before promoting it.
