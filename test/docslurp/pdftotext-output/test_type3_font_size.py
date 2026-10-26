"""Type 3 fonts with the conventional 1000-unit FontMatrix report their real font size.

TextPage::updateFont guesses a Type 3 font's size from the width of an 'm' (assumed 0.6 em)
or any glyph (assumed 0.5 em). For a CJK subset the only glyphs are full-width (1 em), so an
8 pt font was reported as 16 pt and mixed CJK/Latin text on one baseline split into separate
lines. With FontMatrix [0.001 0 0 +-0.001 0 0] the glyph space is the standard em and needs no guess.
"""

import os
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t  # noqa: E402

utils = t.Utils.from_args()
TOUNICODE = b"""/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def /CMapType 2 def
1 begincodespacerange <00> <FF> endcodespacerange
2 beginbfchar <41> <6587> <42> <7AE0> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end"""


def type3(matrix, bbox):
    return ("<< /Type /Font /Subtype /Type3 /FontBBox %s /FontMatrix %s /CharProcs << /g1 8 0 R /g2 8 0 R >> "
            "/Encoding << /Type /Encoding /Differences [65 /g1 /g2] >> /FirstChar 65 /LastChar 66 /Widths [1000 1000] "
            "/ToUnicode 7 0 R >>" % (bbox, matrix))


# F2: Ghostscript-style CJK Type 3 font (y-flipped 1000-unit matrix). Text: two ideographs at 8 pt,
# then Latin at 8 pt on the same baseline right after them.
CONTENT = b"BT /F2 8 Tf 72 700 Td (AB) Tj ET BT /F1 8 Tf 90 700 Td (2026 DOI) Tj ET"
EXTRA = [type3("[0.001 0 0 -0.001 0 0]", "[0 120 1000 -880]"), t.stream("", TOUNICODE), t.stream("", b"1000 0 d0")]
PDF = t.simple_page_pdf(CONTENT, resources="<< /Font << /F1 4 0 R /F2 6 0 R >> >>", extra_objects=EXTRA)

with t.TempDir() as d:
    path = d.write("type3.pdf", PDF)
    root = ET.fromstring(utils.run("pdftotext", "-bbox-layout", "-wordmeta", path, "-").encode("utf-8"))
    sizes = {e.text: float(e.attrib["fontSize"]) for e in root.iter() if e.tag.endswith("}word")}
    cjk = sizes.get("文章")
    t.check(cjk is not None and abs(cjk - 8) < 0.01, "Type 3 CJK word reports 8 pt", repr(sizes))
    lines = [" ".join(w.text for w in l if w.tag.endswith("}word")) for l in root.iter() if l.tag.endswith("}line")]
    t.check(any("文章" in l and "2026" in l for l in lines), "CJK and Latin on one baseline form one line", repr(lines))
    plain = utils.run("pdftotext", path, "-")
    t.check("文章 2026 DOI" in plain or "文章2026 DOI" in plain, "plain text keeps the line together", repr(plain))

t.finish()
