#!/bin/bash
# Make a tagged multi-page PDF with LibreOffice: make_lo_fixture.sh OUT.pdf [PARAGRAPHS]
# Needs soffice. Uses a private profile and scratch dir under $TMPDIR.
set -e
out=$1
n=${2:-400}
work=$(mktemp -d "${TMPDIR:?}/lo-fixture.XXXXXX")
trap 'rm -rf "$work"' EXIT
{
  echo '<html><body>'
  for i in $(seq 1 "$n"); do
    echo "<h2>Section $i</h2><p>Paragraph $i. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco.</p><ul><li>First point $i</li><li>Second point $i</li></ul>"
  done
  echo '</body></html>'
} > "$work/in.html"
soffice --headless -env:UserInstallation=file://$work/profile --convert-to 'pdf:writer_pdf_Export:{"UseTaggedPDF":{"type":"boolean","value":"true"}}' --outdir "$work/out" "$work/in.html" >/dev/null
cp "$work"/out/in.pdf "$out"
