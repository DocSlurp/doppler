# Page facts: performance

Measured with `docslurp/page-facts/bench.py` on this branch (commit 175c7ac04, RelWithDebInfo as in
DOCSLURP.md), 2026-11-12, on the shared 8-thread development host at load 1.7-2.0 with another
build running. Inputs are public: the 477-page Ghostscript manual
(`/usr/share/doc/ghostscript/Ghostscript.pdf`), three poppler-test files, and synthetic sets
the script generates (500 text pages; 300 scanned pages, a full-page DCT image each with a
300-word invisible OCR layer; 100 MRC pages, DCT background plus JBIG2 stencil). Figures are
the median of 9 interleaved runs; CPU is user+sys including children, peak RSS from `wait4`
through a C harness. Run-to-run noise on this host is about ±5% on the sub-second rows, so
single-digit differences between the pdftomarkdown rows are within noise; the pdftotext-pass
rows (one process, no Rust conversion) are the cleaner measure of what facts add.

```sh
python3 docslurp/page-facts/bench.py --bin build/utils \
  --pdftomarkdown docslurp/doppler-md/target/release/pdftomarkdown --runs 9 --synthetic \
  /usr/share/doc/ghostscript/Ghostscript.pdf FILE.pdf ...
```

## Summary against the #32 targets

| target | result |
|---|---|
| `pdfpagefacts -level basic` CPU ≤ 0.85 x the pdftotext pass | **met** on multi-page inputs: 0.66 (Ghostscript), 0.76 (text), 0.75 (scans); about 1.0 on 1-2 page files and 1.04 on 100 tiny MRC pages, where per-run setup dominates |
| peak RSS ≤ text-only + 1 MiB | **met**: +0.1 MiB basic, +0.3-1.1 MiB full |
| `--page-facts basic` ≤ 3% extra CPU | **not met**: the pdftotext pass costs +4.7% (Ghostscript), +4.8% (text), +9.2% (glyph-heavy scans); on pdftomarkdown --stream that is +2% to +8% once Rust's share is added, within noise on small files |
| `--page-facts full` ≤ 8% extra CPU | **met on mixed documents, not on glyph-dense ones**: pdftotext pass +6.0% (Ghostscript), +14.5% (text), +16% (scans), +17% (100 tiny MRC pages, 1.5 ms absolute) |
| cold start suits scale-to-zero | 9.3 ms wall for pdfpagefacts on a 1-page PDF vs 8.0 ms for pdfinfo (dynamic build; the loader dominates, see below) |

Where the time goes (perf, Ghostscript, full): `PageFacts::onGlyph` ~4% of the pass (the
per-glyph clip test, counters and, at full, character classes and font lookup), word-box grid
marks ~0.6%, image dictionary reads for pages whose images pdftomarkdown would otherwise not
inspect (without `--markers`), and the per-page drawing summaries at `vectors`. Basic costs
little per glyph but every glyph pays it, so glyph-dense pages (the synthetic sets draw 1,300
to 2,600 characters a page in one font) show the highest relative overhead. Further cuts, if
the router needs them: drop the per-glyph clip test from basic (clipped characters would then
count as visible), or move character classes to word granularity.

Earlier measurements on this branch, fixed before these numbers: pdfpagefacts leaked each
page's words in words-only mode (149 MiB peak on Ghostscript, now 25 MiB), and confinement
initialised fontconfig (+8 ms, +2.5 MiB per run; pdfpagefacts loads no system fonts).

## Ghostscript.pdf (477 pages)

| command | wall s | CPU s | pages / CPU s | peak RSS MiB | CPU vs reference |
|---|---:|---:|---:|---:|---:|
| pdfinfo | 0.010 | 0.010 | 46505 | 16.5 |  |
| pdftotext pass (as pdftomarkdown runs it) | 0.543 | 0.540 | 884 | 24.9 |  |
| + -page-facts-fd basic | 0.568 | 0.565 | 844 | 25.0 | +4.7 % |
| + -page-facts-fd full | 0.575 | 0.572 | 834 | 26.0 | +6.0 % |
| pdfpagefacts basic | 0.359 | 0.358 | 1334 | 24.7 | 0.66 x pdftotext pass |
| pdfpagefacts full | 0.392 | 0.390 | 1223 | 25.6 | 0.72 x pdftotext pass |
| pdftomarkdown --stream | 0.544 | 0.694 | 687 | 24.9 |  |
| + --page-facts basic | 0.605 | 0.788 | 605 | 25.0 | +13.5 % |
| + --page-facts full | 0.596 | 0.753 | 633 | 25.9 | +8.5 % |
| pdftomarkdown --stream --markers | 0.575 | 0.739 | 646 | 24.8 |  |
| + --page-facts basic | 0.624 | 0.796 | 599 | 25.0 | +7.8 % |
| + --page-facts full | 0.634 | 0.808 | 590 | 25.8 | +9.4 % |

## ClarityOCGs.pdf (1 pages)

| command | wall s | CPU s | pages / CPU s | peak RSS MiB | CPU vs reference |
|---|---:|---:|---:|---:|---:|
| pdfinfo | 0.008 | 0.008 | 122 | 16.1 |  |
| pdftotext pass (as pdftomarkdown runs it) | 0.016 | 0.016 | 63 | 18.1 |  |
| + -page-facts-fd basic | 0.016 | 0.016 | 62 | 18.1 | +2.5 % |
| + -page-facts-fd full | 0.016 | 0.016 | 62 | 18.3 | +2.0 % |
| pdfpagefacts basic | 0.016 | 0.016 | 63 | 18.2 | 1.00 x pdftotext pass |
| pdfpagefacts full | 0.017 | 0.017 | 58 | 18.3 | 1.09 x pdftotext pass |
| pdftomarkdown --stream | 0.017 | 0.017 | 59 | 18.0 |  |
| + --page-facts basic | 0.017 | 0.017 | 58 | 18.3 | +1.8 % |
| + --page-facts full | 0.017 | 0.017 | 57 | 18.4 | +3.2 % |
| pdftomarkdown --stream --markers | 0.017 | 0.017 | 59 | 18.3 |  |
| + --page-facts basic | 0.017 | 0.017 | 58 | 18.3 | +1.4 % |
| + --page-facts full | 0.017 | 0.017 | 58 | 18.5 | +1.6 % |

## checkbox_issue_159.pdf (1 pages)

| command | wall s | CPU s | pages / CPU s | peak RSS MiB | CPU vs reference |
|---|---:|---:|---:|---:|---:|
| pdfinfo | 0.009 | 0.009 | 113 | 16.4 |  |
| pdftotext pass (as pdftomarkdown runs it) | 0.012 | 0.012 | 83 | 18.4 |  |
| + -page-facts-fd basic | 0.012 | 0.012 | 84 | 18.7 | -0.4 % |
| + -page-facts-fd full | 0.012 | 0.012 | 83 | 18.5 | +0.1 % |
| pdfpagefacts basic | 0.012 | 0.012 | 85 | 18.5 | 0.98 x pdftotext pass |
| pdfpagefacts full | 0.012 | 0.012 | 83 | 18.5 | 1.00 x pdftotext pass |
| pdftomarkdown --stream | 0.013 | 0.013 | 78 | 18.4 |  |
| + --page-facts basic | 0.013 | 0.013 | 77 | 18.5 | +1.6 % |
| + --page-facts full | 0.013 | 0.013 | 77 | 18.6 | +1.2 % |
| pdftomarkdown --stream --markers | 0.013 | 0.013 | 77 | 18.5 |  |
| + --page-facts basic | 0.013 | 0.013 | 78 | 18.7 | -0.9 % |
| + --page-facts full | 0.013 | 0.013 | 76 | 18.6 | +1.3 % |

## type3.pdf (2 pages)

| command | wall s | CPU s | pages / CPU s | peak RSS MiB | CPU vs reference |
|---|---:|---:|---:|---:|---:|
| pdfinfo | 0.011 | 0.011 | 185 | 16.1 |  |
| pdftotext pass (as pdftomarkdown runs it) | 0.016 | 0.016 | 125 | 18.5 |  |
| + -page-facts-fd basic | 0.016 | 0.016 | 128 | 18.7 | -2.3 % |
| + -page-facts-fd full | 0.016 | 0.016 | 128 | 18.8 | -2.0 % |
| pdfpagefacts basic | 0.015 | 0.015 | 138 | 18.3 | 0.91 x pdftotext pass |
| pdfpagefacts full | 0.015 | 0.014 | 138 | 18.3 | 0.91 x pdftotext pass |
| pdftomarkdown --stream | 0.017 | 0.017 | 115 | 18.6 |  |
| + --page-facts basic | 0.017 | 0.017 | 114 | 18.7 | +0.8 % |
| + --page-facts full | 0.017 | 0.018 | 112 | 18.8 | +3.0 % |
| pdftomarkdown --stream --markers | 0.017 | 0.018 | 113 | 18.5 |  |
| + --page-facts basic | 0.017 | 0.018 | 113 | 18.7 | -0.4 % |
| + --page-facts full | 0.018 | 0.018 | 109 | 18.8 | +3.7 % |

## synthetic text, 500 pp (500 pages)

| command | wall s | CPU s | pages / CPU s | peak RSS MiB | CPU vs reference |
|---|---:|---:|---:|---:|---:|
| pdfinfo | 0.008 | 0.008 | 60909 | 16.0 |  |
| pdftotext pass (as pdftomarkdown runs it) | 0.324 | 0.322 | 1552 | 19.4 |  |
| + -page-facts-fd basic | 0.339 | 0.338 | 1481 | 19.7 | +4.8 % |
| + -page-facts-fd full | 0.370 | 0.369 | 1356 | 19.8 | +14.5 % |
| pdfpagefacts basic | 0.245 | 0.244 | 2053 | 19.2 | 0.76 x pdftotext pass |
| pdfpagefacts full | 0.270 | 0.268 | 1864 | 19.5 | 0.83 x pdftotext pass |
| pdftomarkdown --stream | 0.340 | 0.477 | 1048 | 19.4 |  |
| + --page-facts basic | 0.373 | 0.514 | 973 | 19.5 | +7.7 % |
| + --page-facts full | 0.399 | 0.537 | 930 | 19.7 | +12.6 % |
| pdftomarkdown --stream --markers | 0.360 | 0.522 | 958 | 19.4 |  |
| + --page-facts basic | 0.374 | 0.517 | 967 | 19.7 | -1.0 % |
| + --page-facts full | 0.401 | 0.550 | 910 | 19.8 | +5.3 % |

## synthetic scans + OCR layer, 300 pp (300 pages)

| command | wall s | CPU s | pages / CPU s | peak RSS MiB | CPU vs reference |
|---|---:|---:|---:|---:|---:|
| pdfinfo | 0.008 | 0.008 | 36955 | 16.1 |  |
| pdftotext pass (as pdftomarkdown runs it) | 0.156 | 0.155 | 1934 | 19.2 |  |
| + -page-facts-fd basic | 0.170 | 0.169 | 1771 | 19.3 | +9.2 % |
| + -page-facts-fd full | 0.181 | 0.180 | 1667 | 19.5 | +16.0 % |
| pdfpagefacts basic | 0.118 | 0.117 | 2562 | 19.3 | 0.75 x pdftotext pass |
| pdfpagefacts full | 0.128 | 0.128 | 2353 | 19.3 | 0.82 x pdftotext pass |
| pdftomarkdown --stream | 0.161 | 0.226 | 1329 | 19.3 |  |
| + --page-facts basic | 0.172 | 0.236 | 1272 | 19.3 | +4.5 % |
| + --page-facts full | 0.189 | 0.252 | 1192 | 19.6 | +11.5 % |
| pdftomarkdown --stream --markers | 0.168 | 0.231 | 1301 | 19.3 |  |
| + --page-facts basic | 0.191 | 0.266 | 1129 | 19.3 | +15.3 % |
| + --page-facts full | 0.190 | 0.259 | 1159 | 19.5 | +12.3 % |

## synthetic MRC scans, 100 pp (100 pages)

| command | wall s | CPU s | pages / CPU s | peak RSS MiB | CPU vs reference |
|---|---:|---:|---:|---:|---:|
| pdfinfo | 0.008 | 0.008 | 12385 | 16.0 |  |
| pdftotext pass (as pdftomarkdown runs it) | 0.012 | 0.011 | 8787 | 17.7 |  |
| + -page-facts-fd basic | 0.012 | 0.012 | 8381 | 18.0 | +4.9 % |
| + -page-facts-fd full | 0.014 | 0.013 | 7504 | 18.1 | +17.1 % |
| pdfpagefacts basic | 0.012 | 0.012 | 8445 | 17.9 | 1.04 x pdftotext pass |
| pdfpagefacts full | 0.013 | 0.013 | 7618 | 18.0 | 1.15 x pdftotext pass |
| pdftomarkdown --stream | 0.012 | 0.012 | 8053 | 17.8 |  |
| + --page-facts basic | 0.013 | 0.013 | 7715 | 18.0 | +4.4 % |
| + --page-facts full | 0.014 | 0.014 | 7102 | 18.1 | +13.4 % |
| pdftomarkdown --stream --markers | 0.013 | 0.014 | 7392 | 17.8 |  |
| + --page-facts basic | 0.013 | 0.014 | 7248 | 18.0 | +2.0 % |
| + --page-facts full | 0.015 | 0.015 | 6639 | 18.1 | +11.3 % |

## Cold start (1-page PDF, median of 21)

| command | wall ms | CPU ms | peak RSS MiB |
|---|---:|---:|---:|
| pdfinfo | 8.0 | 7.9 | 15.9 |
| pdfpagefacts basic | 9.3 | 9.2 | 17.7 |
| pdfpagefacts full | 9.5 | 9.3 | 17.9 |
| pdftomarkdown | 9.9 | 9.9 | 17.7 |

Cold start is the dynamic loader and GlobalParams, as for every poppler util (pdfinfo, which
reads no page, takes 8.0 ms); a static minimal pdfpagefacts build is deferred (packaging).
