"""-bbox, -bbox-layout and -tsv report right-to-left words in logical order, as the
plain text output does, without bidi embedding controls; left-to-right words are unchanged."""

import os
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t, tsv_rows  # noqa: E402

utils = t.Utils.from_args()

# Codes A-D map to final mem, vav, lamed, shin. Drawn left to right as "ABCD" (visual order),
# the logical Hebrew word is shin-lamed-vav-final mem (U+05E9 U+05DC U+05D5 U+05DD).
TOUNICODE = b"""/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def /CMapType 2 def
1 begincodespacerange <00> <FF> endcodespacerange
4 beginbfchar <41> <05DD> <42> <05D5> <43> <05DC> <44> <05E9> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end"""
HEBREW = "שלום"
VISUAL = HEBREW[::-1]
CONTENT = (
    b"BT /F2 14 Tf 300 700 Td (ABCD) Tj ET "  # RTL word, visual order
    b"BT /F1 12 Tf 72 600 Td (Hello 2026) Tj ET"  # LTR word and a number: untouched
)
EXTRA = [
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode 7 0 R >>",
    t.stream("", TOUNICODE),
]
PDF = t.simple_page_pdf(CONTENT, resources="<< /Font << /F1 4 0 R /F2 6 0 R >> >>", extra_objects=EXTRA)
CONTROLS = {"‪", "‫", "‬", "‭", "‮"}


def xml_words(out):
    root = ET.fromstring(out.encode("utf-8"))
    return ["".join(e.itertext()) for e in root.iter() if e.tag.endswith("}word") or e.tag == "word"]


with t.TempDir() as d:
    path = d.write("rtl.pdf", PDF)
    plain = utils.run("pdftotext", path, "-")
    t.check(HEBREW in plain, "plain text has the logical word", repr(plain))
    for flag in ("-bbox", "-bbox-layout"):
        words = xml_words(utils.run("pdftotext", flag, path, "-"))
        t.check(HEBREW in words, flag + ": RTL word in logical order", repr(words))
        t.check(VISUAL not in words, flag + ": no visual-order RTL word", repr(words))
        t.check("Hello" in words and "2026" in words, flag + ": LTR words unchanged", repr(words))
        t.check(not any(c in w for w in words for c in CONTROLS), flag + ": no bidi embedding controls", repr(words))
    texts = [r[11] for r in tsv_rows(utils.run("pdftotext", "-tsv", path, "-")) if r[0] == "5"]
    t.check(HEBREW in texts, "-tsv: RTL word in logical order", repr(texts))
    t.check("Hello" in texts and "2026" in texts, "-tsv: LTR words unchanged", repr(texts))
    t.check(not any(c in w for w in texts for c in CONTROLS), "-tsv: no bidi embedding controls", repr(texts))

t.finish()
