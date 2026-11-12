# DocSlurp development notes

This repository is DocSlurp's fork of poppler (`private` = github.com/DocSlurp/doppler,
`old` = upstream gitlab.freedesktop.org/poppler/poppler). Work lands on `master` through PRs.

## Toolchain

- System packages (Arch): `sudo pacman -S --needed glib2-devel gobject-introspection boost`
  plus the usual poppler deps (freetype2, fontconfig, libjpeg, openjpeg2, libpng, libtiff,
  cairo, glib2, nss, gpgme, lcms2, curl, qt6-base).
- cmake, ninja and ccache come from mise (`mise use -g cmake ninja ccache`); in a non-login
  shell run `eval "$(mise env -s bash)"` first.
- ccache is configured with `base_dir=/home/dan`, so worktrees under `~/worktrees/poppler/*`
  share cache hits with the main checkout.
- Test data: `git clone --depth 1 https://gitlab.freedesktop.org/poppler/test.git ../poppler-test`.

## Build and test

```bash
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  -DCMAKE_C_COMPILER_LAUNCHER=ccache -DCMAKE_CXX_COMPILER_LAUNCHER=ccache \
  -DENABLE_QT5=OFF -DTESTDATADIR=$PWD/../poppler-test
cmake --build build -j 8
ctest --test-dir build -j 8 --output-on-failure
```

Binaries land in `build/utils/` (`pdftotext`, `pdftoppm`, `pdfimages`, `pdfinfo`, ...) and run
from the build tree without installing.

## PDF to Markdown

**Use `pdftomarkdown` for text extraction, not `pdftotext`.** Calling `pdftotext` directly
(plain, `-layout`, `-bbox`, `-tsv` or `-bbox-layout` XHTML piped into `doppler-md`) is
deprecated for DocSlurp ingestion. `pdftomarkdown` chooses the pdftotext flags, applies the
layout, table, heading and furniture handling, and its output is what the golden set and the
quality measurements cover. `pdftotext` stays in the build because `pdftomarkdown` runs it;
treat it as an internal backend and do not depend on its flags or output format.

| Instead of | Use |
|---|---|
| `pdftotext FILE.pdf -` | `pdftomarkdown FILE.pdf` |
| `pdftotext -f N -l M FILE.pdf -` | `pdftomarkdown -f N -l M FILE.pdf` |
| `pdftotext -upw PW FILE.pdf -` | `pdftomarkdown --upw PW FILE.pdf` |
| `pdftotext -bbox-layout -wordmeta -rules FILE.pdf - \| doppler-md` | `pdftomarkdown --strict FILE.pdf` |
| (restitching pictures, tables and paragraphs across pages) | `pdftomarkdown --markers FILE.pdf`, see [doppler-md](docslurp/doppler-md/README.md#restitching-markers---markers) |
| `pdfimages` / cropping figures yourself | `pdftomarkdown --images-dir DIR FILE.pdf` (WebP/SVG per marked picture and figure, the original JPEG (metadata stripped) or a 1-bit PNG per scanned page, at the path its Markdown link names, `manifest.json`), see [doppler-md](docslurp/doppler-md/README.md#extracting-images-and-figures---images-dir) |
| `... \| doppler-md --html` | `pdftomarkdown --html FILE.pdf` |


Streaming: use `pdftomarkdown --stream` (a 16-page window) or no window. Do not use
`--window 0` (or a tiny window): running headers and footers and repeated watermarks are found
by their repetition across the pages in the window, so with 0 none of them is removed.

`docslurp/doppler-md` builds `pdftomarkdown` (`cargo build --release` there), which runs this
fork's `pdftotext -bbox-layout -wordmeta -rules -compact-layout` and converts its XHTML to
Markdown in one step: `PDFTOTEXT=build/utils/pdftotext
docslurp/doppler-md/target/release/pdftomarkdown [--html] [--strict] FILE.pdf`. `--html` writes
tables as HTML, `--strict` keeps full-precision coordinates (the old pipe's exact output). See
its README and PERF.md.

## Page counts

`pdfpagecount FILE.pdf` prints the page count that DocSlurp bills, authorizes and limits by.
It walks the page tree from the catalog's `/Pages` root and counts leaf pages; it never reads
`/Count`, the linearization dictionary, Info, XMP or text, so a document cannot claim its own
count. Do not parse `pdfinfo`'s `Pages:` line or use `getNumPages()` for these decisions.

- **Output:** one decimal integer (at least 1) and a newline on stdout, nothing else. `-claimed`
  adds the root `/Count` (or `none` if it is not an integer) on a second line, for diagnostics
  only. Every failure leaves stdout empty.
- **Diagnostics:** fixed strings on stderr, never document data. Poppler's messages are dropped
  and, on POSIX, fd 2 points at `/dev/null` while the file is processed, so Poppler code that
  writes to stderr directly is silenced too.
- **Limits:** `-max-pages N` (default 100000, at most 10^9) and `-max-depth N` (default 256).
  Kid entries examined, including ones that count nothing, are capped at `2 * max-pages + 64`.
  `-max-memory MiB` (default 4096, 0 for none) caps the process's address space on POSIX.
  Exceeding any limit, or running out of memory, exits 3. Long options also accept `--`.
- **Exit codes:** 0 ok; 1 cannot open (missing, not a PDF, unrepairable) or stdout cannot be
  written; 2 encrypted and needs a password (an empty user password is tried; an unknown
  security handler also lands here); 3 limit exceeded; 4 malformed page tree; 99 usage. Treat
  any other status (a signal, say) as failure.
- **Malformed (4):** no valid `/Pages` reference or dictionary; zero pages; a kid that is not an
  indirect reference; an intermediate node whose `/Kids` is not an array; a page Poppler's `Page`
  rejects (bad `/Contents`, 10000+ annotations); a cycle; a root without `/Kids` that is not a
  lone page.

Kids are classified as `Catalog::cacheSubTree` builds the page list every util renders from:
`/Type /Page`, or any dictionary without `/Kids`, is a page; any other dictionary is an
intermediate node whatever its `/Type`; a kid resolving to a non-dictionary is skipped. Subtrees
are memoized, so shared kids count every time they are referenced (Poppler renders them every
time) without exponential work. Before walking, every in-use object is fetched once, so a
damaged entry that would make a util rebuild the xref later (when it reads page content, say)
rebuilds it now and the walk sees the table the utils end up using; if a rebuild still happens
mid-walk, the walk restarts once. Stdin is not supported; pass a path.

**Calling it.** Run it, and the utils, under a memory limit (cgroup, or at least the built-in
`-max-memory`) and a wall-clock timeout: a few hundred KB of compressed `/Kids` or object streams
can make Poppler allocate gigabytes. Treat exit 3, a signal or any undocumented status as
failure (`gmalloc` failures abort with 134). Then pass
`-f 1 -l N` with the count to every util. That caps their work at the billed pages, but it does
not make them process all N: `pdftotext` and `pdftoppm` clamp `-l` to `getNumPages()`, so
compare the pages produced with N.

How the count differs from what Poppler processes:

- `getNumPages()` returns the root `/Count` (or linearization `/N`), not the walk. A `/Count`
  above the xref size, or a root with no numeric `/Count`, makes it 0 and the utils process
  nothing; this tool still reports the walked pages.
- Poppler stops its page list at `/Count`: with `/Count 1` and 50 kids it renders 1 page; this
  tool reports 50. A linearized `/N` below the walk has the same effect.
- With `/Count` (or `/N`) above the walk and no `-l`, `pdftotext` emits only real pages but
  `pdftoppm` writes blank images up to `/Count`. `-l N` stops that.
- Poppler skips a kid that points back at an ancestor; this tool reports the tree as malformed.
- A `/Type /Page` root without a numeric `/Count` is 1 page, ignoring any `/Kids`, as in Poppler.
  When `/Pages` points at a leaf inside a tree, Poppler renders only that leaf (or nothing if it
  is untyped); this tool reports 1, or malformed.
- `-max-depth` is measured on the first path that reaches a shared subtree.

Tests: `test/docslurp/pagecount/`.

## Page facts

Per-page facts for routing pages (text layer, scans, images and their encodings, fonts, odd
characters, vector drawing and rules, forms, annotations, optional content) and one document
line, as JSON lines (schema `doppler-page-facts/1`). Every field and value:
[docslurp/page-facts/README.md](docslurp/page-facts/README.md); JSON Schema beside it;
benchmarks in [docslurp/page-facts/PERF.md](docslurp/page-facts/PERF.md).

- **With a conversion:** `pdftomarkdown --stream --page-facts FILE FILE.pdf` (or
  `--page-facts-fd N`; `--page-facts-level full`, `--page-facts-include LIST`). pdftotext
  computes them on the same pass (`-page-facts-fd N`); the Markdown does not change.
- **Without one:** `pdfpagefacts [-level basic|full] [-f N] [-l N] FILE.pdf|-` writes them to
  stdout: no text output, no images, no rendering; fixed-string diagnostics, `-max-memory`,
  `-max-pages`, `-timeout`, and a Landlock + seccomp sandbox entered after the file is opened
  and before it is parsed (`utils/Confine`, shared with pdfregions; `-confine require` by
  default). Exit 0 ok, 1 cannot open, 2 needs a password, 3 limit, 5 cannot confine, 99 usage.
- The three write byte-identical lines for the same input, range and level. Facts are
  deterministic (timing only with `-timing`) and page-local (`-f p -l p` gives a page the
  same line as a whole run). They never contain document text, paths, URIs or field values,
  except the capped, sanitised and untrusted `producer`/`creator`.
- `pdftotext -images` also writes the scan verdict (`scan=`, `scanImage=` on `<page>`) and
  each image's dictionary facts (`filter=`, `bpc=`, `cs=`, `smask=`, `decode=`, `inline=`,
  `copy=` on `<image>`, then `orient=` and `overlaid=`: how its pixels land on the page and
  whether anything else shows over them), appended last; the facts and the XHTML use the same
  classifier. `pdftomarkdown` places such pages as `page-scan` (`![page N scan](...)` first)
  and, with `--images-dir`, copies the pixels of an upright scan with nothing over it through
  `pdfregions` copy jobs (a JPEG without its metadata, bilevel as 1-bit PNG) instead of
  rendering; a redaction box, white-out, clip or annotation over a scan makes it rendered;
  see [Page scans](docslurp/doppler-md/README.md#page-scans).
- The scan test's thresholds are options of all three tools: `-scan-min-coverage F`
  (0.85), `-scan-max-words N` (20) and `-scan-layer-overlap F` (0.5) in pdfpagefacts and
  pdftotext, `--scan-min-coverage`, `--scan-max-words`, `--scan-layer-overlap` in
  pdftomarkdown (passed to pdftotext, and part of the images-plan digest). A changed one is
  recorded on the doc line (`scanThresholds`); with the defaults the lines are
  byte-identical to before.
- Page counts for billing stay with `pdfpagecount`: `pageCount` in the doc line is what
  poppler processes.

Tests: `test/docslurp/page-facts/` (one fixture per family, schema, determinism, locality,
pdftotext == pdfpagefacts, a 320-case mutation fuzz) and `docslurp/doppler-md/tests/page_facts.rs`.

## DocSlurp regression tests

`test/docslurp/` holds Python tests that build small PDFs in memory
(`docslurp_testlib.py`) and run the freshly built utils against them. Each subdirectory
with a `CMakeLists.txt` is picked up automatically; register a case with
`docslurp_add_test(<name> <script.py>)`. Run only these with
`ctest --test-dir build -R docslurp --output-on-failure`.

## Benchmarking

The machine is often loaded, so compare user CPU time (not wall time), interleave baseline
and candidate runs, and report the median of at least five runs. Useful inputs:
`/usr/share/doc/ghostscript/Ghostscript.pdf` (477 pages) and the DocSlurp samples in
`~/.cache/docslurp/pricing-samples-2026-10-21/`.
