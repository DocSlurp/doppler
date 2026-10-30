# pdftomarkdown: compact coordinates and HTML tables, measured

Evidence for `docslurp/doppler-md` `pdftomarkdown` (2026-10-29). The results and the acceptance
gate are written up in [PERF.md](../../docslurp/doppler-md/PERF.md#pdftomarkdown-one-command-compact-coordinates-html-tables-2026-10-29).
These are reproduction scripts for this machine: they name the local judge, datasets, corpora and
build outputs by absolute path, and are not CI entry points.

- `score_arms.py`: the four arms (`--strict`/compact x GFM/HTML) on the 254 wild-v2 cases with
  authored references, scored by the judge; writes `eval36.json` (the source of `quality.json`).
  It loads `judge36.py` from its own directory, made from the DocSlurp app harness with the v3.6
  judge directory:
  `sed 's#^ARCHIVE = .*#ARCHIVE = Path("/home/dan/worktrees/docslurp/scorer-html-tables/scripts/raster-eval")#' docslurp-app/performance/doppler-2026-10-26/score.py > judge36.py`.
  Run with `~/.cache/docslurp/raster-eval-venv/bin/python -I score_arms.py`.
- `table_penalties.py`: tolerant table scores, GFM vs `--html`, per case (every HTML table case
  is exactly 0.01 lower: the flat `htmlEncoding` penalty; content F1 is unchanged).
- `compact_screen.py`: Markdown identity and pdftotext CPU with and without `-compact-layout`
  over the local real-PDF corpora (customer data: only aggregates are recorded).
- `confirm_cpu.py PDF...`: `pdftomarkdown` vs `pdftomarkdown --strict` CPU, interleaved, n=7.
- `pipe_vs_command.py PDF`, `convert_only.py BINDIR XHTML N`: the one-command and `--html`
  overhead checks.
- `quality.json`: per-arm summaries, per-level quality and per-case component rows (wild-v2
  is synthetic CC0 data).
