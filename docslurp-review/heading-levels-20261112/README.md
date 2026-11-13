# Heading levels by rank, measured

Evidence for `--heading-levels rank` in `docslurp/doppler-md` (2026-11-12). Results:
[FURNITURE.md](../../docslurp/doppler-md/FURNITURE.md#heading-levels-by-rank-2026-11-12).
Reproduction scripts for this machine; the long PDFs are a local corpus and are not committed,
and only aggregate numbers are recorded.

- `score_headings.py DOPPLER_MD OUT.json` (`DOPPLER_GOLDEN` set): size and rank, whole and
  `--stream`, on the 254 judged golden cases with the v3.6 judge (`../html-spans-20261030/judge36.py`),
  plus level-aware heading measures against the authored references. Run with
  `~/.cache/docslurp/raster-eval-venv/bin/python -I`.
- `agreement.py DOPPLER_MD XHTML_DIR OUT.json`: heading agreement of `--stream` and
  `--stream --body-pages 2` with the whole run, per heading mode, over saved
  `pdftotext -bbox-layout -wordmeta -rules -compact-layout` XHTML.
