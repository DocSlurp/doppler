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
