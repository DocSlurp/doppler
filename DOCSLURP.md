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

- **Output:** one decimal integer and a newline on stdout, nothing else. `-claimed` adds the
  root `/Count` (or `none`) on a second line for diagnostics only. Diagnostics go to stderr as
  fixed strings; Poppler's own messages are suppressed because they can quote document data.
- **Limits:** `-max-pages N` (default 100000) and `-max-depth N` (default 256). Exceeding either
  exits 3 with nothing on stdout. Long options also accept `--`.
- **Exit codes:** 0 ok, 1 cannot open (not a PDF, unrepairable), 2 encrypted and needs a password
  (an empty user password is tried), 3 limit exceeded, 4 malformed page tree, 99 usage.
- **Malformed (4):** no valid `/Pages` reference or dictionary, a kid that is not an indirect
  reference, an intermediate node whose `/Kids` is not an array, a page that Poppler's `Page`
  rejects (bad `/Contents`, 10000+ annotations), or a cycle.

Kids are classified exactly as `Catalog::cacheSubTree` builds the page list every util renders
from: `/Type /Page`, or any dictionary without `/Kids`, is a page; any other dictionary is an
intermediate node whatever its `/Type`; a kid resolving to a non-dictionary is skipped. Pages are
validated by constructing Poppler's own `Page`. Subtrees are memoized, so shared kids count every
time they are referenced (Poppler renders them every time) without exponential work.

How it differs from Poppler, so callers pass `-f 1 -l N` with this count to every util:

- `getNumPages()` returns the root `/Count` (or linearization `/N`), not the walk. A `/Count`
  above the xref size makes it 0, so the utils process nothing.
- Poppler stops its page list at `/Count`: with `/Count 1` and 50 kids it renders 1 page; this
  tool reports 50.
- With `/Count` above the real pages but within the xref size, `pdftotext` emits only the real
  pages but `pdftoppm` writes blank images up to `/Count`.
- Poppler skips a kid that points back at an ancestor; this tool reports the tree as malformed.
- A lone `/Type /Page` root counts as 1 only without a numeric `/Count`, as in Poppler.

Tests: `test/docslurp/pagecount/`.

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
