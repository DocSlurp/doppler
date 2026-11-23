"""Recover compact symbolic TrueType subsets with a byte-to-glyph symbol cmap."""
import io
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import um_fixtures as fixtures

try:
    from fontTools import subset
    from fontTools.ttLib import TTFont
    from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
except ImportError:
    sys.exit(77)

t = fixtures.t
utils = t.Utils.from_args()
if not shutil.which("fc-match"):
    sys.exit(77)
name, path = subprocess.check_output(
    ["fc-match", "-f", "%{postscriptname}\n%{file}", "DejaVu Serif"], text=True
).split("\n", 1)
if name != "DejaVuSerif":
    sys.exit(77)
font = TTFont(path)
letters = "2468"
glyph_names = [font.getBestCmap()[ord(c)] for c in letters]
original_ids = [font.getGlyphID(g) for g in glyph_names]
subsetter = subset.Subsetter()
subsetter.populate(text=letters)
subsetter.subset(font)
assert original_ids != [font.getGlyphID(g) for g in glyph_names]
symbol = CmapSubtable.newSubtable(4)
symbol.platformID, symbol.platEncID, symbol.language = 3, 0, 0
symbol.cmap = dict(zip(range(0xf001, 0xf005), glyph_names))
font["cmap"].tables = [symbol]
buf = io.BytesIO()
font.save(buf)
data = buf.getvalue()


def sample(extra=""):
    return t.simple_page_pdf(
        b"BT /F2 14 Tf 50 500 Td <01020304> Tj ET",
        resources="<< /Font << /F2 6 0 R >> >>",
        extra_objects=[
            f"<< /Type /Font /Subtype /TrueType /BaseFont /AAAAAA+DejaVuSerif /FirstChar 1 /LastChar 4 /Widths [680 680 680 680] /FontDescriptor 7 0 R {extra} >>",
            "<< /Type /FontDescriptor /FontName /AAAAAA+DejaVuSerif /Flags 4 /FontBBox [0 -200 1100 1000] /Ascent 900 /Descent -200 /CapHeight 800 /StemV 80 /ItalicAngle 0 /FontFile2 8 0 R >>",
            t.stream(f"/Length1 {len(data)}", data, compress=True),
            t.stream("", b"begincmap 1 begincodespacerange <00> <ff> endcodespacerange 1 beginbfrange <01> <04> <0057> endbfrange endcmap"),
        ],
    )


with t.TempDir() as tmp:
    pdf = tmp.write("renumbered.pdf", sample())
    assert "2468" not in utils.run("pdftotext", pdf, "-")
    result = utils.run("pdftotext", "-recover-fonts", pdf, "-")
    assert "2468" in result, repr(result)
    pdf = tmp.write("author-mapped.pdf", sample("/ToUnicode 9 0 R"))
    assert "WXYZ" in utils.run("pdftotext", "-recover-fonts", pdf, "-")
