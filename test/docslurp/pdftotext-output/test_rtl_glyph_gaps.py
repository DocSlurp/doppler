"""Right-to-left glyphs drawn one by one with small shaping gaps stay in one word.

Shaped Arabic is often drawn glyph by glyph in logical (right-to-left) order. The gaps between
the glyph boxes of one word (here 0.08 em) are below the word-break space addChar uses
(0.1 em), but line assembly treated any gap above 0.03 em as a space and split the word.
A real word space (0.38 em) still separates words; left-to-right text is unchanged.
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
3 beginbfchar <41> <0628> <42> <0646> <43> <0643> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end"""
# Helvetica widths at 10 pt: A = B = 6.67, C = 7.22. Visual left to right:
# word 2: B@282 A@289.5 (ends 296.17) | 3.83 pt gap | word 1: C@300 then "BA"@308 as one run (0.78 pt gap)
# drawn in logical order (rightmost run first), one text object per run. The two-glyph run makes
# line assembly use its fixed 0.03 em space threshold, as real shaped lines do.
GLYPHS = [(308.0, b"BA"), (300.0, b"C"), (289.5, b"A"), (282.0, b"B")]
CONTENT = b" ".join(b"BT /F2 10 Tf %g 700 Td (%s) Tj ET" % (x, c) for x, c in GLYPHS)
CONTENT += b" BT /F1 10 Tf 72 600 Td (Hello) Tj ET BT /F1 10 Tf 98.6 600 Td (world) Tj ET"
EXTRA = [
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode 7 0 R >>",
    t.stream("", TOUNICODE),
]
PDF = t.simple_page_pdf(CONTENT, resources="<< /Font << /F1 4 0 R /F2 6 0 R >> >>", extra_objects=EXTRA)
WORD1 = "بنك"  # beh noon kaf
WORD2 = "بن"  # beh noon
CONTROLS = dict.fromkeys(map(ord, "‪‫‬"))

with t.TempDir() as d:
    path = d.write("gaps.pdf", PDF)
    plain = utils.run("pdftotext", path, "-").translate(CONTROLS)
    t.check(WORD1 + " " + WORD2 in plain, "plain: shaping gaps do not split the word; the word space remains", repr(plain))
    t.check("Hello world" in plain, "plain: left-to-right word space unchanged", repr(plain))
    root = ET.fromstring(utils.run("pdftotext", "-bbox-layout", "-wordmeta", path, "-").encode("utf-8"))
    words = [(e.text, e.attrib["spaceAfter"]) for e in root.iter() if e.tag.endswith("}word")]
    rtl = [w for w in words if w[0] and "؀" <= w[0][0] <= "ۿ"]
    # glyph words of one Arabic word carry spaceAfter=0 except at the real word space
    t.check(sum(s == "1" for _, s in rtl[:-1]) == 1, "-bbox-layout: one space between the two Arabic words", repr(words))

t.finish()
