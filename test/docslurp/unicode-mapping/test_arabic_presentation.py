"""Arabic presentation forms (U+FB50-U+FDFF, U+FE70-U+FEFC) from ToUnicode are extracted as base letters.

Shaped Arabic fonts often map glyphs to positional presentation forms (beh final, lam-alef isolated).
Search, embeddings and the plain text output need the base letters; a ligature such as lam-alef
expands to its letters in logical order in plain, -bbox and -tsv output.
"""

import os
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import um_fixtures as f  # noqa: E402,F401  (sets up the import path)
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()
# Drawn left to right (visual order): A = beh final (FE90), B = lam-alef isolated (FEFB),
# then C = alef isolated (FE8D) and D = arabic fatha isolated form (FE76, NFKC is space + mark).
TOUNICODE = b"""/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def /CMapType 2 def
1 begincodespacerange <00> <FF> endcodespacerange
4 beginbfchar <41> <FE90> <42> <FEFB> <43> <FE8D> <44> <FE76> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end"""
WORD = "لاب"  # lam alef beh: logical order of the visual "AB"
CONTENT = b"BT /F2 14 Tf 300 700 Td (AB) Tj ET BT /F2 14 Tf 300 650 Td (DC) Tj ET"
EXTRA = [
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode 7 0 R >>",
    t.stream("", TOUNICODE),
]
PDF = t.simple_page_pdf(CONTENT, resources="<< /Font << /F1 4 0 R /F2 6 0 R >> >>", extra_objects=EXTRA)


def presentation(s):
    return any(0xFB50 <= ord(c) <= 0xFDFF or 0xFE70 <= ord(c) <= 0xFEFF for c in s)


with t.TempDir() as d:
    path = d.write("arabic.pdf", PDF)
    plain = utils.run("pdftotext", path, "-")
    t.check(WORD in plain, "plain: base letters in logical order", repr(plain))
    t.check(not presentation(plain), "plain: no presentation forms", repr(plain))
    t.check("اَ" in plain or "َا" in plain, "plain: isolated fatha keeps its mark, no inserted space", repr(plain))
    root = ET.fromstring(utils.run("pdftotext", "-bbox", path, "-").encode("utf-8"))
    words = ["".join(e.itertext()) for e in root.iter() if e.tag.endswith("}word")]
    t.check(WORD in words, "-bbox: base letters in logical order", repr(words))
    t.check(not any(presentation(w) for w in words), "-bbox: no presentation forms", repr(words))
    rows = [r.split("\t") for r in utils.run("pdftotext", "-tsv", path, "-").split("\n") if r]
    t.check(WORD in [r[11] for r in rows if r[0] == "5"], "-tsv: base letters in logical order", repr(rows))

t.finish()
