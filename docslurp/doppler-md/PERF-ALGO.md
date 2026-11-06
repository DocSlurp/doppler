# doppler-md: algorithmic tuning (layout, reading order, tables, paragraphs)

Scope: hot spots after parsing (furniture, regions/bands/gutters, gutter and ruled tables,
visual rows, paragraphs). Parsing, I/O and startup were tuned separately and are not touched
here. Output is unchanged: every step kept byte-identical output.

## How it was measured

- **Launches**: one process per file over all 258 golden raws (1917 pages), children's
  user+sys from `getrusage(RUSAGE_CHILDREN)`. Median of 5 interleaved A/B runs on a shared
  8-core host.
- **In-process**: `examples/bench_algo.rs` on the 10 largest raws (673 pages). It reports the
  whole conversion and `model::parse` alone, so `algo` = total - parse is the share this
  work targets. Thread CPU time, batches of at least 100 ms, median of 3 per run, 5
  interleaved runs.
- **Adversarial pages**: `scripts/make_adversarial.py OUT 1` writes hostile one-page inputs
  (single run, user+sys).
- **Correctness after every change**: `scripts/parity.sh` (258/258), the adversarial
  outputs compared with the baseline binary's, and a differential fuzz run:
  `make_adversarial.py --fuzz DIR SEED N` makes random documents (shared baselines, columns,
  rules, boxes, RTL/CJK, list markers, 1e308 and -0.0 values). Seeds 1 and 2 gave 5000
  documents, 70% of them with tables, all identical to the baseline. The ruled-table row
  window was also run with its walk cap forced to 3, so the fallback path was exercised too.

## Results

| measurement | baseline | final |
| --- | --- | --- |
| launches, 258 raws | 1.691 s CPU, 68,006 pages/min | 1.609 s, 71,470 pages/min |
| in-process, 10 largest, total | 0.406 s, 99,500 pages/min | 0.377 s, 107,150 pages/min |
| in-process, 10 largest, algo share | 0.116 s, 350,000 pages/min | 0.071 s, 566,000 pages/min |

Parsing is now about 80% of the in-process time, so whole-file gains are small.

Adversarial pages (scale 1, user+sys seconds):

| page | lines | baseline | final |
| --- | --- | --- | --- |
| adv-twocol: two-column key/value list | 20,000 | 273.0 | 3.67 |
| adv-grid: 300 x 30 one-cell lines | 9,000 | 31.3 | 0.63 |
| adv-ruled-many: 3000 small ruled tables | 15,000 | 13.2 | 0.08 |
| adv-sameline: one-word lines on one baseline | 20,000 | 6.05 | 1.27 |
| adv-furniture: page numbers in the header band | 20,000 | 1.49 | 0.04 |
| adv-caption-rules: 20k rules, one line per gap | 20,000 | 1.35 | 0.07 |
| adv-rules: 3000-row ruled table + 3000 spread rules | 6,000 | 0.04 | 0.03 |

## Loops

1. **Neighbour lookups, per-band gutters once** (fa6449f1b). The profile showed adv-twocol
   was cubic: `table_like` compared every right-side baseline with every left one, and the
   function was called once per band. The same quadratic scans were in furniture's "no other
   line on this baseline" check and in `gutter_columns`' narrow-gap alignment. The fix:
   values sorted with `fcmp` and a nearest-neighbour test (`layout::near_any` and
   `near_other`). Floating subtraction is monotonic, so the closest value is always one of
   the two neighbours. Non-finite queries and NaN fall back to the scan, and
   `tests/algo.rs` compares both against brute force. `regions` also computes each band's
   gutters once instead of again for every band a gutter is followed through. Effect:
   twocol 273 -> 6.9 s, grid 31.3 -> 1.1 s, furniture 1.49 -> 0.045 s. The real workload
   did not change measurably.
2. **Moves, no member copies, merge sweep** (f3bdac7ef). In adv-sameline, `paragraphs`
   cloned the growing merged line for every fragment on the same baseline. It now releases
   `prev`, so the merged `Rc` is unwrapped and its words are moved. `regions` copied every
   member of a followed gutter for each band start. It now builds left/right from the band
   slice, and only after the multi-band test passes. `table_like` counts aligned lines in
   one merge pass. Effect: sameline 6.05 -> 1.36 s, twocol 6.9 -> 5.1 s.
3. **Flag vectors and cheaper Counter** (9ac8c6f1a). The real-workload profile showed SipHash
   and HashMap allocation from the drop/in_ruled/consumed/used `HashSet<usize>` sets and from
   the per-line colour `Counter`. The sets became page-sized flag vectors, cleared after each
   region or group so the per-region behaviour stays the same. `Counter` now checks the last
   key first, then scans up to 8 keys, and only then builds a std hash index. Effect: about
   6% less in-process time (0.407 -> 0.381 s).
4. **Shared cells, ASCII `rtl`** (db82a95e9). `visual_rows` cloned every line into a cell, and
   `ruled_tables` calls it up to three times per group. `Row.cells` is now `Vec<Rc<Line>>`,
   and a line with no cell gap is its own cell. `Line::rtl` returns early for all-ASCII
   lines. Effect: algo share about 0.081 -> 0.065 s (noisy).
5. **Ruled tables without page scans** (e9ce6a99b). In adv-ruled-many, every rule group
   rebuilt the visual rows of all other lines, scanned all lines for members and for each
   caption band, and scanned every group for each rule. Now:
   - Lines are indexed by vertical middle, so members and caption bands are range lookups.
   - The row above and the rows below a table are grouped from the nearest row start. A line
     further than its tolerance from the previous kept baseline always starts a row. Walks
     longer than 256 positions fall back to building all rows.
   - Groups that are past the rule gap are dropped from the match scan.
   - Vertical rules are looked up by x range.

   Effect: ruled-many 13.2 -> 0.07 s, caption-rules 1.35 -> 0.06 s.

## Rejected or deferred

- **FxHash for `Counter`**: rejected without measuring. Keys such as normalised furniture
  text come from hostile input, and FxHash collisions are easy to craft. The std hasher
  stays, and the linear scan avoids most hashing anyway.
- **Selection-based `pyc::median`**: rejected. `select_nth_unstable` can return -0.0
  where the stable sort returns 0.0 (or the reverse), which can change a later division.
- **Walk memo plus incremental `table_like` over suffixes in `regions`**: exact only when
  walks end in the same final gutter. Jittered column edges give every start its own gutter,
  so an attacker gets O(n^2) back. Not worth the code.
- **Incremental bbox and median for same-baseline merges**: deferred. It needs either a
  new `Line` constructor in `model.rs` (the parser's file) or a lazy merge state in
  `paragraphs`.

## Remaining super-linear paths

These are still quadratic in the number of lines, with output-exact behaviour:

| page | 20k lines | 50k lines |
| --- | --- | --- |
| adv-twocol: `regions` re-follows a rejected (table-like) gutter from every band, O(n) each | 3.7 s | 25.7 s |
| adv-sameline: each same-baseline merge rebuilds the line's bbox and median | 1.3 s | 8.0 s |
| adv-grid: as twocol, once per gutter | 0.63 s (9k lines) | 5.2 s (22.5k lines) |

Two smaller ones:

- `table_columns` and the caption-band "crossed" test scan vertical rules in the group's x
  range for every group and gap, which is O(rules^2) when thousands of vertical rules share
  one x.
- The rule-group match is O(groups^2) when thousands of disjoint rules share one y.

## Work limits (doppler_md.py v24)

A council review timed one page of one-cell lines at 4.3 s (100 x 200), 25.7 s (200 x 200) and
123 s (280 x 280), so the converter and `doppler_md.py` now bound the work (`max_page_lines`,
`max_page_rules`, `max_page_work`, `max_doc_work`; see README.md). Layout work units are elements
visited, charged in the same places in both implementations (`layout::Budget`): each `regions`
call its element count, each band a gutter is followed through its elements, each gutter
reaching the column test its member count, and each same-baseline merge its merged word count.
A page that runs out is read as plain lines in poppler order and listed in the report.

Defaults are far above the golden set (largest page 303 lines and 20,617 units, largest document
50,508 units): 20,000 lines or rules per page, 10 million units per page, 50 million per
document. Timings (CPU s, single run each, `doppler-md` before and after on the same host):

| page | lines | before | after | fallback |
| --- | --- | --- | --- | --- |
| grid 100 x 200 | 20,000 | 6.3 | 0.16 | page_work |
| grid 200 x 200 | 40,000 | 24.5 | 0.04 | page_lines |
| grid 280 x 280 | 78,400 | 105.9 | 0.08 | page_lines |
| six columns, 10,000 rows | 60,000 | 61.3 | 0.06 | page_lines |
| one baseline, 50,000 one-word lines | 50,000 | 8.3 | 0.05 | page_lines |
| two columns, 25,000 rows | 50,000 | 26.1 | 0.05 | page_lines |
| at the line cap: two columns, 10,000 rows | 20,000 | 3.7 | 0.31 | page_work |
| at the line cap: six columns, 3,333 rows | 19,998 | | 0.18 | page_work |
| at the line cap: one baseline, 20,000 lines | 20,000 | 1.3 | 0.09 | page_work |
| 100 pages of 100 x 150 grids | 1,500,000 | | 2.0 (mostly parsing 330 MB) | page_work, then doc_work |

The worst page under the budget is the two-column list (about 35 ns per unit, from `table_like`'s
sorts), so one page costs at most about 0.4 s of layout and a document about 2 s more than its
linear cost. The golden set is unchanged by the limits, and its speed is too (median of 7
interleaved launches over the 258 inputs: 0.643 s CPU before, 0.636 s after).

A sub-quadratic `regions` that keeps output exact is still open (see the walk memo above); the
budget makes it unnecessary for safety.

## Gutter-table grid work (2026-11-05)

`build_grid` placed every word with a linear scan of the columns, O(words x columns): three
rows of 160,000 one-word cells (each its own column) took 62 s on the review host (0.7 s before
the wide-spaced pass, which reached that path). Columns from `gutter_columns` and ruled tables
are finite and ordered, so the column is now two binary searches with the same answer (the
first interval holding the midpoint with 1 point of slack, else the first nearest centre; a unit
test checks it against the scan on random ordered columns, including touching, zero-width and
duplicate intervals); any other column list keeps the scans. The reproducer takes 1.3 s.

Table detection now runs within the page `Budget`: each run of rows that reaches column
detection, in the dense or the wide-spaced pass, charges twice its words (columns, then grid).
Grouping rows is linear and is not charged. On the golden set the largest page goes from 20,617
to 21,016 units and the largest document to 59,725; nothing degrades. `doppler_md.py` does not
charge these units, so the work counts in `tests/page_layout.rs` no longer equal Python's.

The wide-spaced pass is one forward walk over the rows (a run grows row by row and the next
starts where it stopped) plus the column and grid work charged above. The rotated-text pass
copies each rotated line once and scans the rules three times per rotation; the reframed layout
shares the page budget, and a 19,800-line rotated page stays under the limits
(`tests/rotated_borderless.rs`).
