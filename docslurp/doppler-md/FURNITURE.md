# Furniture window and body sample (2026-11-12)

`--window N` used to set two things: the furniture window (running headers and footers,
repeated watermark words and pictures, judged over N pages either side) and the body size and
colour (fixed from the first N + 1 pages). They are now separate (`--window`, `--body-pages`,
`--body-size`, `--body-color`, see the README's Streaming section), and the defaults were set
from this sweep.

**Inputs.** 15 long PDFs from a local corpus of public-domain and government documents (not
committed): a few born-digital textbooks and manuals, most scanned books, manuals, reports and
a dictionary with an OCR text layer; 134 to 812 pages, 6,620 in all. pdftotext ran once per
PDF (`-bbox-layout -wordmeta -rules -compact-layout`) and every arm converted the same XHTML
with `doppler-md`. Quality: the v3.6 Markdown judge (as in PERF.md) on the 254 judged wild-v2
cases of the golden set, and byte changes on all 258 golden inputs.

**Measures.** (a) Lines of each page that differ from the whole-document run (multiset per
page). (b) Running heads and footers left in the text, found independently of the converter's
rule: lines in the top or bottom 12% of the page (the converter uses 9%) whose text, lower-cased
with digits and punctuation folded, appears in that band on at least 3 of the 11 (or 33)
pages around; page numbers alone are not counted; a leak is such a line that is still a line of
the page's Markdown. Its bias: it is itself a local rule, so it favours small windows somewhat.
(c) Words of the whole-document run missing from the arm, not counting the detected furniture
(an over-removal check; inspected, most of it is running heads the detector missed, OCR noise
lines of dashes, and on window 2 "(Continued)" table titles). (d) CPU and peak RSS of the
conversion (`doppler-md` alone; pdftotext is the same for all arms and dominates).

| window | body sample | lines differing from whole, % | worst doc, % | docs over 5% | pages differing | running heads/footers left (±5 / ±16 detector) | whole-run words missing, % | body size not the whole document's (of 15) | conversion CPU s | converter peak RSS MiB |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| – | whole document | 0.00 | 0.0 | 0 | 0 | 2436 / 2653 | 0.000 | 0 | 4.66 | 82 |
| 2 | first 3 pages (old rule) | 11.85 | 46.4 | 11 | 4406 | 1509 / 1706 | 0.428 | 11 | 4.33 | 16 |
| 2 | 16 text pages | 2.98 | 7.8 | 6 | 3276 | 1519 / 1719 | 0.186 | 2 | 4.57 | 16 |
| 2 | 32 text pages | 2.97 | 7.8 | 6 | 3262 | 1519 / 1719 | 0.186 | 1 | 4.51 | 16 |
| 2 | 64 text pages | 2.97 | 7.8 | 6 | 3262 | 1519 / 1719 | 0.186 | 1 | 4.72 | 16 |
| 4 | first 5 pages (old rule) | 12.15 | 81.7 | 10 | 4138 | 1659 / 1875 | 0.354 | 10 | 4.78 | 16 |
| 4 | 16 text pages | 2.02 | 7.0 | 3 | 2483 | 1666 / 1884 | 0.117 | 2 | 4.72 | 16 |
| 4 | 32 text pages | 2.00 | 7.0 | 3 | 2469 | 1666 / 1884 | 0.116 | 1 | 4.71 | 16 |
| 4 | 64 text pages | 2.00 | 7.0 | 3 | 2469 | 1666 / 1884 | 0.116 | 1 | 4.47 | 16 |
| 6 | first 7 pages (old rule) | 7.05 | 18.3 | 7 | 3645 | 1767 / 1983 | 0.347 | 8 | 4.53 | 16 |
| 6 | 16 text pages | 1.76 | 6.6 | 2 | 2114 | 1770 / 1987 | 0.107 | 2 | 4.75 | 16 |
| 6 | 32 text pages | 1.74 | 6.6 | 2 | 2100 | 1770 / 1987 | 0.106 | 1 | 4.66 | 16 |
| 6 | 64 text pages | 1.74 | 6.6 | 2 | 2100 | 1770 / 1987 | 0.106 | 1 | 4.99 | 16 |
| 8 | first 9 pages (old rule) | 4.02 | 19.1 | 6 | 3249 | 1781 / 1996 | 0.280 | 7 | 4.74 | 16 |
| 8 | 16 text pages | 1.84 | 6.6 | 3 | 2223 | 1784 / 2000 | 0.108 | 2 | 4.82 | 16 |
| 8 | 32 text pages | 1.82 | 6.6 | 3 | 2209 | 1784 / 2000 | 0.108 | 1 | 4.74 | 16 |
| 8 | 64 text pages | 1.82 | 6.6 | 3 | 2209 | 1784 / 2000 | 0.108 | 1 | 4.57 | 16 |
| 16 | first 17 pages (old rule) | 1.46 | 6.3 | 1 | 1740 | 1957 / 2171 | 0.096 | 2 | 4.74 | 16 |
| 16 | 16 text pages | 1.46 | 6.3 | 1 | 1740 | 1957 / 2171 | 0.096 | 2 | 4.86 | 16 |
| 16 | 32 text pages | 1.45 | 6.3 | 1 | 1726 | 1957 / 2171 | 0.096 | 1 | 4.65 | 16 |
| 16 | 64 text pages | 1.45 | 6.3 | 1 | 1726 | 1957 / 2171 | 0.096 | 1 | 4.45 | 16 |

Judge (254 cases, quality / headings / tables / content): whole document 0.7501 / 0.7453 /
0.811 / 0.9087. Every arm with a window of 4 or more and a 16- or 32-page body sample is the
same on every case (no case changes). Window 2 changes 10 cases, quality 0.7498, tables 0.8102.
The old body rule changes 3 cases at windows 4 and 6 (headings 0.7458 and 0.7444). Golden set
(258 inputs) changed against whole-document conversion: window 4 / 32 pages 3 inputs (the
Ghostscript manual, a 10-K, a US Code title), window 6: 2, window 8 and 16: 1 (the manual).

**Reading.**

- The body sample decided most of the difference. The old rule (first N + 1 pages) gave a
  different body size than the whole document on 11, 10, 8, 7 and 2 of the 15 documents at
  windows 2, 4, 6, 8, 16 (front matter, covers, tables of contents). The first 32 pages with
  text give the whole document's on 14 of 15, and 64 pages do no better; 16 pages miss one more.
- With that sample, a smaller furniture window leaves fewer running heads and footers:
  1,666 at window 4 against 1,957 at window 16 (-15%) and 2,436 for the whole document
  (by the ±16 detector: 1,884, 2,171, 2,653). The heads a large window keeps are chapter
  titles that repeat within their chapter but fall under 40% of a 33-page window.
- Window 2 removes a little more again (1,519) but needs only 2 of 5 pages, so it starts
  removing content that repeats nearby (continued-table titles, repeated short lines), and it is
  the only window that changes judged golden cases. Windows 6 and 8 sit between 4 and 16.
- CPU and memory do not depend on the arms: conversion CPU over the 15 documents is 4.3 to
  5.0 s for every arm (host noise), the converter's peak RSS is 16 MiB for every window and
  sample (82 MiB for whole-document conversion), and end-to-end `pdftomarkdown --stream`
  (median of 5, interleaved) took the same CPU and peak RSS as before on three documents; the
  first page arrives a few hundredths of a second later (the larger body sample).

**Chosen.** `--stream` is `--window 4` with the body from the pages up to the 16th with text
(at most 32 pages read, and fewer once they hold 262,144 words, rules and glyph records), and
`--window N` without `--body-pages` uses the same sample. Whole-document conversion (no window)
is unchanged.

Window 4 rather than 16 (owner decision after council review). The council noted that
against the whole-document run every measure but one favours 16 (lines differing 1.45%
against 2.00%, documents over 5% 1 against 3, whole-run words missing 0.096% against 0.116%,
golden inputs changed 1 against 3; the judge shows no difference), and that the measure that
favours 4, running heads left (-15%), comes from a detector that is itself a local rule; a
re-run on three documents outside the sweep found only -2% (523 against 533 heads). The
whole-document run is not a quality reference for furniture (it keeps the most running heads),
the three golden inputs that change lose running heads, and the judge shows no loss, so
`--stream` takes the window that removes more furniture. `--window 16` stays available. Open:
a measure not biased towards local windows (judged heading and furniture labels on long
documents), and a look at the extra 0.02% of whole-run words missing at window 4.

**Out of sample (council re-run, 3 documents, 1,632 pages: a tax guide, a security-controls
catalogue and a physics textbook).** On the catalogue the 32-text-page sample gives body size
9 where the whole document, 16 pages and 64 pages give 10; window 4 with 32 pages then differs
from the whole run on 5.41% of lines (worst document 8.9%, 2 documents over 5%), against
3.52% with 16 or 64 pages. On the 15 sweep documents 64 pages did no better than 32, so with
18 documents 64 is at least as good on every one and 32 is worse on one. Across all 18
documents 16 and 32 each miss the whole document's body size on 2 (different ones), and the
judged golden cases do not change between them, so the default is 16 (owner decision): it
halves what is held before the first page streams, and an occasional mis-sized body on an odd
document is accepted. A larger sample (64), skipping front matter, or a stratified sample (still
at most 2N pages read) remains the option if a wider held-out set shows a gain.

**Page ranges.** With `--context N` (N at least the window) and the body from
`--estimate-body`, 64-page ranges of eight long PDFs (3,306 pages; the Ghostscript manual and
seven of the documents above) joined to the whole `--window 4` run byte for byte, every page.
CPU of converting a whole PDF as ranges (user+sys of pdftomarkdown and pdftotext, median of 5,
interleaved), against the same ranges without context:

| PDF | pages | range | `--context 4` | `--context 16` |
| --- | ---: | ---: | ---: | ---: |
| Ghostscript manual | 477 | 16 | +29% | +92% |
| | | 64 | +12% | +31% |
| | | 256 | +23% (noise: 2 ranges) | +35% |
| born-digital ebook | 236 | 16 | +38% | +120% |
| | | 64 | +6% | +35% |
| | | 256 | 0% | +2% |
| scanned pamphlet (OCR) | 566 | 16 | +30% | +121% |
| | | 64 | +11% | +39% |
| | | 256 | 0% | +7% |

The extra cost is pdftotext reading the context pages (they are not laid out without
`--markers`; with it the pages before the range are, and their work is charged to the document
budget): about 2N / L of the range's pdftotext time, plus a pdftotext start per range.
`--context` now defaults to the window (4 with `--stream`, so the `--context 4` column
applies to default ranges); more than the window buys nothing.

Scripts: `docslurp-review/window-body-20261112/`.

# Distinct margin text preservation

The default `furniture_preserve_headings=true` requires literal repetition (case-folded and trimmed, counted once per page and separately per margin) before deleting a running header or footer. Text differing in numbers is retained regardless of font size, body distance or split-line layout. Recognized isolated page numbers still drop. Setting the option to false restores aggressive digit-normalized repetition matching.

This favors retaining content over removing variable running headers/footers. Identical repeated real titles or fragments can still be removed. It is not a lossless-title guarantee.

## Current validation

- 79 Rust tests pass; release build passes. New regression was observed failing before the fix.
- Original pilot retains all four titles; previously failing confirmation now retains all three. Exact publication headers and Page n footers still drop.
- Fresh 254-case comparison against the retained pre-heading baseline changes 166 outputs. One case changes table layout: a recovered section prefix causes an existing payment block to be represented as a table. No unchanged-corpus claim applies to this change. Added variable footer text is expected.
- Independent integrity gate passes, with no findings. Raw replay and corpus outputs are retained in the app performance/header-deletion-fix-2026-10-30 directory.
- Independent adversarial and fidelity reviews found no high security issue. The fidelity reviewer requested a new corpus/table comparison; the figures above resolve that validation gap, but do not establish end-to-end app fidelity.
- The optional frozen full-text scoring pass was interrupted after conversion; no final aggregate quality scores are claimed.
- No deployment, live VM test, provider calls or app-pin update.

## Historical evidence (superseded candidate)

# Conservative numbered-heading preservation

Margin furniture is grouped by top/bottom band and counted once per page. Exact literal repetitions keep their own counts. A line that repeats only after digit normalization may now be retained if it is a short, prominent top-band heading, has nearby smaller body text outside the margin, and is not a fragment sharing a vertical start with another line. The checks reuse `h2_ratio`, `heading_max_chars`, `minor_heading_ratio` and `title_gap`.

`furniture_preserve_headings` defaults to true. Setting it to false disables the new heading exemption; it does not restore cross-margin grouping. Exact repeated headers, ordinary-size numbered running headers and isolated page numbers still drop. Typography evidence uses linear word scans, not additional median allocations/sorts. Cached body and line-start evidence is computed at most once per page and only when normalized repetition could cause removal.

This is intentionally conservative. Split headings, multicolumn body association, unusual typography and larger title-to-body gaps may prevent recovery. It is not a semantic classifier and cannot guarantee every title is retained.

## Evidence and limits

- Full Rust suite: 78 tests pass; release build passes.
- Four-page authored integration pilot: four titles recovered (previously none), table bytes unchanged.
- Final 254-case synthetic public corpus comparison: all outputs byte-identical to baseline, all tables unchanged, zero degradation. This establishes regression preservation, not corpus-wide title gains. Frozen v3.6 scorer unchanged; the rejected first candidate and its scores remain in external evidence.
- Independent three-page authored PDF confirmation: two titles recovered (previously none), body retained and genuine running header/page numbers removed. The full three-of-three title-retention gate **failed**. The smaller heading exceeded the conservative proximity bound. No tuning was performed against that confirmation. This remains an experimental draft, not validated broad title recovery.
- Independent authored-provenance integrity gate passed on the working production diff (8,435 files scanned, 8,164 production files); no fixture names/answers or score-weight changes in product code. The initial sandbox-blocked scan is retained separately.

Six interleaved process runs per arm, user+system CPU median, same host and `pdftotext`, `pdftomarkdown --html`:

| Workload | Pages | Baseline seconds | Candidate seconds |
| --- | ---: | ---: | ---: |
| Authored pilot | 4 | 0.011779 | 0.011503 |
| Public synthetic annual report | 9 | 0.039055 | 0.035507 |
| Public Ghostscript manual | 477 | 1.790243 | 1.778953 |

No speedup is claimed. The public-manual baseline range was 1.680–2.025 seconds, candidate 1.762–1.921 seconds; host noise exceeds the median difference. The two non-pilot workloads emitted equal output hashes between arms. Timing is local CLI CPU, not deployed ingestion latency.

## Reviews

Independent initial adversarial/fidelity reviewers identified exact-header contamination by other numbered lines and added median work before budgets. Revisions count exact literals per margin and use linear min/all typography scans. Fragment rejection also prevents the first candidate's changed form layout. Independent final reviewers reproduced prior cases and established no remaining medium/high issue in their scope. Their low findings (unnecessary preprocessing and rollback wording) were addressed locally. Reviews do not establish exhaustive resource bounds, every layout, VM integration or approval to merge.

## PDFust integration baseline

DocSlurp uses PDFust for rendering/probing as well as optional extraction surfaces; this pass measured the actual binding adapter's page-render helper. Dependency-complete core imports repaired the initial harness failure before processing. The loaded binding reports version 0.13.0 / source f702a7a0e59e7a4285a4eddb798d9608d936d9c1, which differs from the current PDFust checkout; no source-version equivalence is claimed.

Six interleaved four-page/100-DPI runs per lane count produced identical image hashes. One lane: median wall 36.16 ms, CPU 36.55 ms. Four lanes: wall 12.77 ms, CPU 41.72 ms. Four lanes are already the default; this is configuration evidence, not a new engine optimization. No PDFust code or global concurrency default changed. Broader rendering/probing throughput and live VM behavior are unmeasured.

Raw outputs, hashes, source provenance, rejected-candidate evidence, scripts and measurements are retained in the app's `performance/engine-pass-2026-10-30/`. Only aggregate evidence is committed here. No customer inputs, paid calls, merges, deployments or releases were used.
