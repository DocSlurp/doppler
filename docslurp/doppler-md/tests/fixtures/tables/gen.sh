#!/bin/sh
# Rebuilds the table fixtures of tests/table_pdfs.rs from their sources (test fixtures only;
# none of these generators is in the product path). All content is made up.
#   LaTeX: pdfLaTeX with booktabs (texlive/texlive:latest-medium has it)
#   HTML: Chromium print to PDF; writer.html: LibreOffice Writer PDF export
set -eu
cd "$(dirname "$0")"
T=${TMPDIR:-/tmp}
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:/w" -w /w texlive/texlive:latest-medium \
  sh -c 'for f in booktabs grid norules matrix matrix3; do pdflatex -interaction=nonstopmode $f.tex >/dev/null; done'
rm -f ./*.aux ./*.log
for f in bordered borderless signs signs-ruled; do
  chromium --headless=new --no-sandbox --disable-gpu --no-pdf-header-footer --user-data-dir="$T/chr-$$" \
    --print-to-pdf="$PWD/$f.pdf" "file://$PWD/$f.html"
done
rm -rf "$T/chr-$$"
soffice -env:UserInstallation="file://$T/lo-$$" --headless --convert-to pdf:writer_pdf_Export --outdir "$PWD" writer.html
rm -rf "$T/lo-$$"
# data/: data tables whose cells hold enumerated labels, footnote marks, half-open bins,
# bars, math italic letters and check marks (Chromium); pdfLaTeX matrices, tables with math in
# their cells (fractions, determinants, pmatrix, brace-grouped rows) and tables beside matrices
cd data
for f in enum ruledenum footnote bins2 flags pipe mathital mathitalhdr check numlabels; do
  chromium --headless=new --no-sandbox --disable-gpu --no-pdf-header-footer --user-data-dir="$T/chr-$$" \
    --print-to-pdf="$PWD/$f.pdf" "file://$PWD/$f.html"
done
rm -rf "$T/chr-$$"
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:/w" -w /w texlive/texlive:latest-medium \
  sh -c 'for f in *.tex; do pdflatex -interaction=nonstopmode $f >/dev/null; done'
rm -f ./*.aux ./*.log
