# doppler-md performance log: parsing, I/O, allocation and startup

Workload: the 258 golden inputs (`md-golden/raw/*.raw`, 1,917 pages, 134 MB), one process launch
per file, stdout to `/dev/null`, single core. CPU is user+sys from `getrusage(RUSAGE_CHILDREN)`
around the whole set; each figure is the median of at least 5 runs, with the binaries interleaved
(alternating order) because the host is shared. Pages/min = 1,917 / (CPU s / 60). Where CPU
differences were within noise, total user-space instructions and cycles (`perf stat -e
instructions:u,cycles:u` over the same loop) settle the call. Every kept change passed
`scripts/parity.sh` (258/258 byte-identical) and `cargo test`.

| step | CPU s (user / sys) | pages/min | instructions |
| --- | --- | --- | --- |
| baseline `d800a2bfa` | 1.649 (1.271 / 0.373) | 69,800 | 11.39 G |
| loop 1 `81a351c87` | 1.374 (1.019 / 0.366) | 83,700 | 8.70 G |
| loop 2 `c7b5fcb9b` | 1.004 (0.752 / 0.252) | 114,600 | 6.46 G |
| loop 3 `6dd3e3ebe` | 0.839 (0.611 / 0.222) | 137,100 | 5.73 G |
| loop 4 `3c0a35254` | 0.833 (0.623 / 0.228) | 138,100 | 5.04 G |
| loop 5 `e301d7fe6` | 0.721 (0.553 / 0.168) | 159,400 | 4.97 G |
| loop 6 `facb943ea` | (robustness; neutral) | | 4.97 G |
| loop 7 `e33854948` | 0.702 (0.539 / 0.165) | 163,800 | 4.84 G |
| **final, re-measured against baseline** | **0.698 (0.538 / 0.159)** vs 1.649 | **164,800** vs 69,800 | 4.84 G vs 11.39 G |


Startup alone (258 launches on an empty file): 0.081 s user + 0.102 s sys at baseline, 0.063 s
user + 0.025 s sys at the end; `/usr/bin/true` takes 0.077 + 0.033.

## Loop 1: input decoding, attribute scan, number parsing

Profile (`perf record -g`): `Attrs::parse` 13.6%, `Utf8Chunks::next` (the lossy UTF-8 decoder)
5.2%, `dec2flt` + `py_float` about 7.6%, regex compilation about 10%, malloc/free about 7%.

- `String::from_utf8_lossy` walks the input with `Utf8Chunks`, which is much slower than the
  strict validator. Now `str::from_utf8` runs first and the lossy decoder runs only on invalid
  input (same replacement semantics).
- Attribute names are scanned byte-wise for ASCII (decoding a `char` only at non-ASCII bytes), and
  one attribute buffer is reused across tags.
- `py_float` takes Clinger's exact fast path for `-?\d+(\.\d*)?` with at most 15 significant
  digits and at most 22 fractional digits: the mantissa and the power of ten are both exact, so a
  single IEEE division is correctly rounded and bit-identical to `str::parse` (a new test compares
  bits on 20,000 generated decimals plus edge cases). Everything else goes through the old path.

Result: 1.649 -> 1.374 s (-17%). Kept.

## Loop 2: regex compilation at startup

Profile: regex compilation (`regex_syntax` parser, Thompson compiler, `Utf8Sequences`) about 15%
of all samples after loop 1. The three patterns are compiled in every process, with Python's
Unicode `\d`/`\s` interpolated as hundreds of explicit ranges for non-ASCII haystacks.

Hypothesis: hand-written matchers remove the compile cost entirely. `src/patterns.rs` implements
`NUMBERED_RE`, `NUMERIC_RE` and `LEADING_PUNCT_RE` directly; `NUMERIC_RE`'s ambiguous first
alternative is a set-of-positions simulation, so every matcher is linear. `tests/patterns.rs`
builds the original patterns with the same explicit Unicode classes and compares all three on
400,000 generated strings (two alphabets, one biased toward number shapes) plus fixed cases;
two deliberate mutations of the matchers were both caught. `regex` moved to `[dev-dependencies]`.

Result: 1.452 -> 1.004 s (-31%; this run measured loop 1 at 1.452 s). Binary 2.8 MB -> 1.1 MB.
Kept. `tables.rs` changes only in the two pattern definitions, which now call `patterns`.

## Loop 3: per-word allocation

Profile: `Attrs::parse_into` 16%, `py_float` 7%, `model::parse` 7.5%, malloc/free 6.5%,
`str::contains` (font style) 3%, `to_lowercase` 0.8%.

- `Attrs` keeps a fixed slot for each of the 11 attribute names the converter reads (later
  duplicates still win), so there is no Vec and no lookup scan.
- The last font name's (bold, italic) and the last colour's `Rc<str>` are reused (fonts and
  colours repeat word after word).
- Word text goes straight into an `Rc<str>` when it has no control character, instead of through a
  `String` (one allocation and copy fewer).
- A line's words collect into one reused buffer and are moved out with an exact-size allocation.

Result: 1.011 -> 0.839 s (-17%, median of 7). Kept.

## Loop 4: attribute name scan

Byte-table lookup for ASCII `\w` and a `match` (length switch) for the key index. CPU 0.836 ->
0.833 s, within noise; instructions 5.73 G -> 5.04 G (-12%), cycles -3%. Kept on the instruction
count. A unit test checks the key table and duplicate/odd attribute strings.

## Loop 5: build and link settings (process startup)

perf showed `ld-linux` at 3% of samples and 258 empty-file launches cost 0.083 s of system time
against 0.033 s for `/usr/bin/true`.

- `.cargo/config.toml` links glibc statically (`+crt-static`, static-pie) for x86_64 and aarch64
  Linux. The binary uses no NSS, iconv or dlopen. Empty-file startup sys 0.083 -> 0.026 s; golden
  set 0.812 -> 0.721 s (-11%).
- `lto = "fat"` instead of thin: 0.812 -> 0.795 s, cycles -3%, within noise; kept (same build time).

Rejected in this loop:
- `panic = "abort"`: 0.736 vs 0.738 s, no gain, and it changes the exit status of a panic.
- `strip = true`: 0.757 vs 0.736 s, no gain (symbols are never paged in).

## Loop 6: unterminated attribute values (robustness)

With no closing quote left in a tag, no later `name="value"` pair can match, so the scan stops
instead of re-running memchr from every later name (quadratic for a tag of repeated `a="`).
Neutral on the golden set. Tried in the same loop and rejected: a plain byte loop instead of
`memchr` for the closing quote (+1% instructions, cycles within noise).

## Loop 7: tag scanner searches

Profile: `memchr` 6%, and `memmem::find` built a new searcher for every `<word>`. Every search for
`>`, `</word>` and the newline test also ran to the end of the input when nothing matched, which
made the scan quadratic on unclosed tags.

Each search now remembers its last answer; queries only move forward, so a search resumes where
the previous one ended. One `memmem::Finder` is built per parse. The newline test only needs the
first newline after the text start.

Result: 0.764 -> 0.702 s (-8%, median of 9), instructions -2.6%. On hostile inputs: one line with
300,000 unclosed `<word>` tags goes from more than 60 s (timeout) to 14 ms; 300,000 unclosed
`<page` tags from 4.1 s to 7 ms. Output on 63 truncated and garbled inputs (random truncation,
byte noise, dropped quotes, `</word>` and `>`, spliced chunks) is byte-identical to the baseline
binary, and no input panics.

## Rejected without a commit

- Skipping destructors at exit (`mem::forget` of the parsed pages, input and output): -2.7%
  instructions, CPU 0.761 -> 0.739 s, within noise. Forgetting the pages would need a library
  API change (`to_markdown` drops them inside) or a leak in library use, so it isn't worth it.
- Writing output pieces without `join`: stdout is a `LineWriter`, and one `write_all` of a large
  buffer goes straight through, so the current single write is already the cheap path.

## Where the time is now (final profile)

`to_markdown` (with `parse` inlined) 12%, `Attrs::parse_into` 12%, `py_float` 9% (mostly the
exact division), malloc/free about 12% combined, memchr 5%, memcmp 4%. SipHash appears at about 3%
from `HashSet<usize>`/`HashSet<(usize, usize)>` in `to_markdown`/`furniture` and from the colour
`Counter` in `body_size`. Those are in the algorithmic code and were left alone here; a faster
hasher would need care because the keys come from hostile input. Kernel time left over is
mostly page faults for reading the input (about 33k pages) and for the parsed structures.

## doppler_md.py v23 rules (right-to-left tables, table header lines)

The two new rules add a pass over each region's items and render tables at output time.
`DATA_CELL_RE` is a hand-written matcher in `src/patterns.rs` with the same randomized check as
the others. Measured against `b23e93187` on a busy host (load about 7), alternating the two
binaries over 5 rounds: per-file-launch CPU median 1.070 s vs 1.112 s pinned to one core and
0.934 s vs 0.905 s unpinned, both within noise. User-space instructions over the set were
4.090 G vs 4.053 G (+0.9%) and cycles 2.65 G vs 2.71 G. Output is 258/258 against
`golden-v23`, and the adversarial pages take the same time.

## pdftomarkdown: one command, compact coordinates, HTML tables (2026-10-29)

`pdftomarkdown FILE.pdf` replaces `pdftotext -bbox-layout -wordmeta -rules FILE - | doppler-md`.
CPU is user+sys of the whole command (pdftotext plus conversion), interleaved, median of n runs.

- **One process instead of a pipe:** Ghostscript.pdf (477 pages) 2.344 s vs 2.331 s for the
  pipe, n=9: the same within noise. Of that, pdftotext is about 91%; the conversion of its 44 MB
  of XHTML takes about 0.2 s, so no converter change can save more than about 9% end to end.
- **`--html`:** conversion alone on the saved XHTML, 193 ms vs 200 ms for GFM tables, n=15
  with overlapping ranges: no measurable cost.
- **`-compact-layout` (the new default; `--strict` turns it off):** confirmation on three
  PDFs not used for screening, n=7, with heavy host load (about 20), so the absolute times run
  high:

  | PDF | `--strict` | default (compact) | change |
  | --- | ---: | ---: | ---: |
  | Ghostscript.pdf, 477 pages | 3.834 s | 3.160 s | -17.6% |
  | AAPL 10-K FY2025, 80 pages | 0.680 s | 0.505 s | -25.8% |
  | US Code title 1 | 0.266 s | 0.176 s | -33.8% |

  The ranges do not overlap. Screening over 29 real PDFs: pdftotext CPU 8.47 s -> 6.55 s
  (-22.7%), Markdown byte-identical for 25.

Quality gate: the DocSlurp Markdown judge v3.6 (the frozen v3.5 judge plus HTML tables scored as
pipe tables) on the 254 wild-v2 cases with authored references, with every arm's XHTML made from
the PDFs by the same pdftotext. Accepting a change required quality (overall and per level), the
strict and tolerant table scores and headings to stay within 0.005, and the hallucinated-token
and substitution rates (diagnostics the judge reports separately, not part of its score) to rise
by at most 0.002.

| arm | quality | content | tables | tolerant tables | headings | hallucinated | substituted | cases changed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `--strict` (the pipe) | 0.7666 | 0.9217 | 0.8097 | 0.9020 | 0.743 | 0.0728 | 0.0458 | |
| compact (default) | 0.7669 | 0.9217 | 0.8097 | 0.9020 | 0.742 | 0.0729 | 0.0458 | 12 |
| `--strict --html` | 0.7666 | 0.9217 | 0.8097 | 0.8891 | 0.743 | 0.0728 | 0.0458 | 223 |

Compact passes: every level is within 0.001 (L8 and L9 slightly up). On the four real PDFs that
changed, the differences are reading-order swaps of a few lines where words sit on a layout
threshold (a footnote quote mark, a two-line table header cell) and one scanned map table that
merges two OCR fragments into one cell. Their word multisets are identical apart from markup:
two en dashes that strict read as `-` list bullets stay as text, and the map table has one
column fewer.

`--html` keeps every table's content (tolerant content F1 is unchanged in every case); it loses
exactly 0.01 on every table case, the judge's flat `htmlEncoding` penalty for a table "encoded as
HTML instead of a pipe table". That is a format preference, not lost data, so `--html` stays an
opt-in output for renderers that want it. Harness and aggregate results:
`docslurp-review/pdftomarkdown-20261029/`.
