# Furniture window and body sample, measured

Evidence for the `--window` / `--body-pages` / `--context` change in `docslurp/doppler-md`
(2026-11-12). Results: [FURNITURE.md](../../docslurp/doppler-md/FURNITURE.md#furniture-window-and-body-sample-2026-11-12).
Reproduction scripts for this machine; the PDFs are a local corpus and are not committed, and
only aggregate numbers are recorded.

- `run_arms.py DOPPLER_MD XHTML_DIR OUT_DIR [jobs]`: every (furniture window, body sample) arm
  over saved `pdftotext -bbox-layout -wordmeta -rules -compact-layout` XHTML, with CPU and peak
  RSS per run. The `--body-pages` arms need a build with that option; the "current rule" arms
  (`bcur`) are the old windowed body rule only on a build before the default changed.
- `analyze.py XHTML_DIR OUT_DIR`: the sweep table (line agreement with the whole-document run,
  running heads/footers left by an independent geometric detector, whole-run words missing,
  body size, CPU, RSS).
- `score_windows.py DOPPLER_MD OUT.json`: the arms on the 254 judged golden cases with the v3.6
  judge. It loads `judge36.py` from its own directory, made as in
  `../pdftomarkdown-20261029/README.md`; run with `~/.cache/docslurp/raster-eval-venv/bin/python -I`.
- `shard_identity.py PDFTOMARKDOWN SHARD CONTEXT PDF... -- WINDOW_ARGS`: `--estimate-body`,
  then ranges with `--context` and the body given, joined and compared with the whole run.
- `context_cpu.py PDFTOMARKDOWN REPS PDF...` (`$PDFINFO` set): CPU of a whole PDF as ranges of
  16/64/256 pages with context 0/4/16.
- `stream_cost.py OLD NEW REPS PDF...`: `--stream` CPU, peak RSS and time to the first byte,
  two builds interleaved.
