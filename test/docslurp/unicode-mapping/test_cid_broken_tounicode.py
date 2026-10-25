"""A broken ToUnicode stream must not wipe out text that the CID collection can supply.

Adobe-Japan1 Type0 font with the UniJIS-UCS2-H CMap showing U+3042 U+65E5.  Without a
ToUnicode stream the text comes from the Adobe-Japan1 CID-to-Unicode table.  A ToUnicode stream
that does not parse used to flip the font into "char code" mode and look the CID-collection
table up with the raw code, extracting an empty string.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import um_fixtures as f  # noqa: E402
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()
SHOW = b"BT /F1 24 Tf 72 700 Td <304265E5> Tj ET"
EXPECTED = "あ日"

GARBAGE = b"this is not a cmap at all"
NO_MAPPINGS = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n1 begincodespacerange <0000> <ffff> endcodespacerange\nendcmap end end"
VALID = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n1 begincodespacerange <0000> <ffff> endcodespacerange\n1 beginbfchar <3042> <3044> endbfchar\nendcmap end end"


def extract(tounicode):
    pdf = f.type0_page(SHOW, "UniJIS-UCS2-H", tounicode)
    with t.TempDir() as d:
        path = d.write("t.pdf", pdf)
        return utils.run("pdftotext", path, "-").strip()


out = extract(None)
t.check(out == EXPECTED, "no ToUnicode: text comes from the Adobe-Japan1 collection", repr(out))
out = extract(GARBAGE)
t.check(out == EXPECTED, "garbage ToUnicode: falls back to the collection map", repr(out))
out = extract(NO_MAPPINGS)
t.check(out == EXPECTED, "ToUnicode without mappings: falls back to the collection map", repr(out))
out = extract(VALID)
t.check(out.startswith("い"), "valid ToUnicode keeps precedence (U+3042 -> U+3044)", repr(out))
# a partial ToUnicode (only U+3042 mapped) leaves U+65E5 to the collection map
out = extract(VALID)
t.check(out == "\u3044\u65e5", "partial ToUnicode: unmapped codes filled from the collection map", repr(out))
t.finish()
