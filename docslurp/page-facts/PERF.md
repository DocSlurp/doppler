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

## Issue #45: the /Prev chain read at open, read-only lookups for facts

Measured 2026-11-13 (whole documents at the final commit, the rest at the round-4 commit), interleaving master (af38de11e) and this branch.
CPU is user+sys and peak RSS comes from `wait4`.
- **Runs:** whole documents are the median of 11 runs; `-l 1` (mostly open time) is the median
  of 7 runs.
- **Host:** the shared 8-thread host at load average 3-9.
- **The pass** is `pdftotext -bbox-layout -wordmeta -rules -compact-layout`.
- **`gs-incr-N`** is the Ghostscript manual with N classic incremental updates appended, each
  redefining `/Info`.
- **The `h-*` files** are made by `test/docslurp/page-facts/xref_hostile/chains.py`.

| input | run | master CPU | branch CPU | change | RSS master / branch (MiB) |
|---|---|---:|---:|---:|---:|
| Ghostscript.pdf (477 pp) | the pass | 0.532 s | 0.535 s | +0.6 % | 25.4 / 25.2 |
| Ghostscript.pdf | `pdfpagefacts -level full` | 0.404 s | 0.407 s | +0.8 % | 26.1 / 26.2 |
| gs-incr-999 (999 updates) | the pass | 0.538 s | 0.532 s | -1.0 % | 25.5 / 25.3 |
| gs-incr-999 | `pdfpagefacts -level full` | 0.409 s | 0.404 s | -1.1 % | 26.1 / 26.3 |
| gs-incr-999 | `pdftotext -l 1` | 0.016 s | 0.016 s | -1.0 % | 19.0 / 18.9 |
| gs-incr-1200 (past the 999-section cap) | `pdftotext -l 1` | 0.018 s | 0.046 s | +28 ms | 18.9 / 19.6 |
| h-deep (100,000 empty sections) | `pdftotext -l 1` | 2.817 s | 0.262 s | -91 % | 17.8 / 18.0 |
| h-bomb (older xref stream, `/Size` 50,000,000, 340 KB of Flate) | `pdftotext -l 1` | 0.010 s | 0.009 s | | 18.0 / 18.0 |
| h-overlap (1,000 trailers, each a string over 4 MB) | `pdftotext -l 1` | 20.05 s | 0.105 s | -99.5 % | 26.4 / 26.5 |

- **Whole documents** are within noise.
- **Reading the chain at open** costs nothing measurable over 999 updates. Upstream already read
  most of the chain on the first fetch of an unchanged object.
- **Past the section cap** the walk stops, and the first fetch of an object only the unread
  sections list rebuilds the table (+28 ms here, flagged).
- **The doc line** reads under a guard instead of through a copy of the table, so it needs no
  extra table memory.
- **Hostile chains** stay within the bounds:
  - the section cap ends the 100,000-section chain, which master walked with a quadratic loop
    check;
  - the growth budget refuses the 50,000,000-entry stream before allocating;
  - the rebuild's trailer budget ends h-overlap's quadratic trailer parse (master re-parsed 4 MB
    once per trailer).

**Council repros**, branch vs master, `pdftotext -q -l 1` (generators in `xref_hostile/`):

| repro | master | branch |
|---|---:|---:|
| 4,000-deep nested `/XRefStm` in an older section, 4.2 MB | 0.01 s (never read) | 0.05 s |
| 400-deep nested `/XRefStm` in the main section | 2.03 s | 0.04 s |
| 50 older xref streams with 268 MB predictor rows | 21.9 s, 1,295 MiB | 1.22 s, 271 MiB (the decode budget ends the chain; the later rebuild still allocates the rows) |
| 2,000 unterminated trailers in a rebuild | 8.99 s | 0.02 s |
| 999 sections sharing one 65,536-entry Paeth `/XRefStm` | 0.01 s (never read) | 0.16 s (decode budget) |
| 999 older 30,000-entry Paeth xref streams | 0.01 s (never read) | 0.18 s (decode budget) |
| 1,000 sections, each an unterminated string to end of file | 0.01 s (never read) | 0.13 s (parse budget) |

## Issue #43: thin rules per grid, words centred inside images

Measured 2026-11-13 on the same inputs (the Ghostscript manual, the three poppler-test files and
bench.py's synthetic sets), interleaving master (dd02511ee) and this branch (e50929235), median of
21 runs with bench.py's C harness, CPU user+sys, on the shared 8-thread host at load average 6-10.
Same A/B method as #40: `pdftotext -bbox-layout -wordmeta -rules -compact-layout` (the pass),
the pass with `-page-facts-fd` at `full`, and `pdfpagefacts -level full`.

| input | full vs pass, master | full vs pass, #43 | pass + full CPU, #43 vs master | pdfpagefacts full, #43 vs master |
|---|---:|---:|---:|---:|
| Ghostscript.pdf (477 pp) | +8.5 % | +10.6 % | +1.3 % | -0.0 % |
| ClarityOCGs.pdf | +3.5 % | +3.2 % | +0.9 % | +0.3 % |
| checkbox_issue_159.pdf | +2.9 % | -0.2 % | -4.0 % | +1.2 % |
| type3.pdf | +3.7 % | -3.8 % | +5.1 % | -1.0 % |
| synthetic text, 500 pp | +16.5 % | +15.6 % | +1.0 % | -2.7 % |
| synthetic scans + OCR layer, 300 pp | +18.5 % | +20.7 % | -2.2 % | +1.6 % |
| synthetic MRC scans, 100 pp | +11.3 % | +9.4 % | +4.0 % | +1.9 % |

No CPU this benchmark can tell from noise. The facts-only measure (`pdfpagefacts full`) is -2.7 %
to +1.9 % (Ghostscript -0.0 %, 0.728 s both). The pass with facts on Ghostscript is +1.3 %
(1.072 s against 1.058 s), while the bare pass, which this change does not touch, moved -0.7 %
between the same two builds; the 14-19 ms files move ±5 % run to run (an earlier round on a busier
host gave -4.6 % to +2.8 %). What the fields cost: at `vectors`, each page's non-box rules are
rounded to 0.01 pt (`to_chars`/`from_chars`, four a rule) and the thin ones sorted; pages with
both directions get a sorted sweep for touching pairs and a union-find over at most 8,192 rules,
and each grid a run of its candidate rules (at most 2^20 visits a page in all); at
`images-detail`, a second test of each word's centre against at most 32 images. Hostile cases
are in `test_cost_bounds.py` (a 2,400 x 2,400 crossing mesh: the visit cap; 11 pages of 19,000
rules then a table: the run's rule budget), within the bounds there.

## Issue #40: grids, words inside images, bytes per pixel

Measured 2026-11-13 on the same inputs (the Ghostscript manual, the three poppler-test files and
bench.py's synthetic sets), interleaving master (f4355280e) and this branch, median of 21 runs
with bench.py's C harness, CPU user+sys. The host was much busier than for the tables below
(load average 30-55 on 8 threads), so absolute CPU is about twice theirs; the comparison is
between builds run alternately on the same load. Script: an A/B driver over `bench.run`
(`pdftotext` pass, `+ -page-facts-fd basic`, `+ -page-facts-fd full`, `pdfpagefacts -level full`).

| input | full vs pass, master | full vs pass, #40 | pass + full CPU, #40 vs master | pdfpagefacts full, #40 vs master |
|---|---:|---:|---:|---:|
| Ghostscript.pdf (477 pp) | +9.1 % | +9.4 % | -0.5 % | +0.8 % |
| ClarityOCGs.pdf | +3.6 % | +2.1 % | +0.1 % | +0.4 % |
| checkbox_issue_159.pdf | +3.4 % | +2.9 % | -1.4 % | +2.4 % |
| type3.pdf | +1.4 % | +2.7 % | +0.2 % | +1.2 % |
| synthetic text, 500 pp | +13.1 % | +13.3 % | +0.3 % | +1.5 % |
| synthetic scans + OCR layer, 300 pp | +13.1 % | +13.5 % | -0.4 % | -2.4 % |
| synthetic MRC scans, 100 pp | +16.5 % | +13.5 % | -1.4 % | -0.9 % |

The new fields add no CPU this benchmark can tell from noise (-1.4 % to +0.3 % of the pass
with facts at `full`; an earlier 15-run round gave -3.5 % to +3.1 %). The full-level overhead
over the bare pass stays where master has it on the same load (Ghostscript +9.4 % against
master's +9.1 %; the +6.0 % in the tables below was measured on a quieter host). What the
fields cost: one box per word kept while `images-detail` is on (40 bytes), tested against at
most 32 listed images at the end of the page; a half-point size histogram per word at
`vectors`; the grid search over the page's rules (sorts plus the bounded grouping); and, once
per image XObject, a 41-byte read after its declared `/Length`. Hostile cases are in
`test_cost_bounds.py` (rules that never share a group: the 2^20-visit cap; 32 images under
60,000 words a page), within the bounds there.

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
