"""CJK radicals from ToUnicode are extracted as their equivalent unified ideographs.

Fonts and converters often map ideograph glyphs to Kangxi Radicals (U+2F00..U+2FD5, e.g. U+2F42
for 文) or CJK Radicals Supplement code points (U+2ED4 for 门). They look identical but break
search and embeddings. Unicode's Equivalent_Unified_Ideograph property gives the ideograph.
"""

import os
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import um_fixtures as f  # noqa: E402,F401
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()
# A = KANGXI RADICAL SCRIPT (U+2F42), B = CJK RADICAL C-SIMPLIFIED GATE (U+2ED4), C = U+4E2D (already unified)
TOUNICODE = b"""/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def /CMapType 2 def
1 begincodespacerange <00> <FF> endcodespacerange
3 beginbfchar <41> <2F42> <42> <2ED4> <43> <4E2D> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end"""
CONTENT = b"BT /F2 12 Tf 72 700 Td (CAB) Tj ET"
EXTRA = [
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode 7 0 R >>",
    t.stream("", TOUNICODE),
]
PDF = t.simple_page_pdf(CONTENT, resources="<< /Font << /F1 4 0 R /F2 6 0 R >> >>", extra_objects=EXTRA)
EXPECTED = "中文门"


def radicals(s):
    return any(0x2E80 <= ord(c) <= 0x2FDF for c in s)


with t.TempDir() as d:
    path = d.write("radicals.pdf", PDF)
    plain = utils.run("pdftotext", path, "-")
    t.check(EXPECTED in plain and not radicals(plain), "plain: radicals become unified ideographs", repr(plain))
    root = ET.fromstring(utils.run("pdftotext", "-bbox", path, "-").encode("utf-8"))
    words = ["".join(e.itertext()) for e in root.iter() if e.tag.endswith("}word")]
    t.check(EXPECTED in words, "-bbox: radicals become unified ideographs", repr(words))

t.finish()
