"""An explicit CIDToGIDMap must recover through the embedded shapes, not the PDF font name."""
import os
import shutil
import struct
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import um_fixtures as fixtures

t = fixtures.t
utils = t.Utils.from_args()
if not shutil.which("fc-match"):
    sys.exit(77)


def system_font(name):
    output = subprocess.check_output(["fc-match", "-f", "%{postscriptname}\n%{file}", name.replace("DejaVu", "DejaVu ")], text=True)
    found, path = output.split("\n", 1)
    if found != name or not path.endswith(".ttf"):
        sys.exit(77)
    return Path(path).read_bytes()


regular = system_font("DejaVuSans")
other = system_font("DejaVuSerif")
# In the DejaVuSans Unicode cmap, A-D are GIDs 36..39. Assign unrelated CIDs 1-4.
cid_to_gid = struct.pack(">5H", 0, 36, 37, 38, 39)


def sample(font, mapping=""):
    return t.simple_page_pdf(
        b"BT /F2 12 Tf 40 500 Td <0001000200030004> Tj ET",
        resources="<< /Font << /F2 6 0 R >> >>",
        extra_objects=[
            f"<< /Type /Font /Subtype /Type0 /BaseFont /AAAAAA+DejaVuSans /Encoding /Identity-H /DescendantFonts [7 0 R] {mapping} >>",
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /AAAAAA+DejaVuSans /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 8 0 R /CIDToGIDMap 10 0 R /DW 700 >>",
            "<< /Type /FontDescriptor /FontName /AAAAAA+DejaVuSans /Flags 4 /FontBBox [0 -200 1100 1000] /Ascent 900 /Descent -200 /CapHeight 800 /StemV 80 /ItalicAngle 0 /FontFile2 9 0 R >>",
            t.stream(f"/Length1 {len(font)}", font, compress=True),
            t.stream("", cid_to_gid),
            t.stream("", b"begincmap 1 begincodespacerange <0000> <ffff> endcodespacerange 1 beginbfrange <0001> <0004> <0057> endbfrange endcmap"),
        ],
    )


with t.TempDir() as tmp:
    path = tmp.write("numeric-cids.pdf", sample(regular))
    assert "ABCD" not in utils.run("pdftotext", path, "-")
    assert "ABCD" in utils.run("pdftotext", "-recover-fonts", path, "-")
    path = tmp.write("different-shapes.pdf", sample(other))
    assert utils.run("pdftotext", "-recover-fonts", path, "-") == utils.run("pdftotext", path, "-")
    path = tmp.write("mapped.pdf", sample(regular, "/ToUnicode 11 0 R"))
    assert "WXYZ" in utils.run("pdftotext", "-recover-fonts", path, "-")
