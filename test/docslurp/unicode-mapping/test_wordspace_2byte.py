"""Tw (word spacing) applies only to the single-byte code 32, never to a 2-byte code 0x0020.

Identity-H font where CID 0x20 is an ordinary glyph (ToUnicode: U+0043 'C').  TextPage::addChar
used to subtract Tw from the advance of any code equal to 0x20, giving the glyph a negative
width and scrambling the word's bounding box.  Output with Tw=30 must equal output with Tw=0.
"""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import um_fixtures as f  # noqa: E402
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()
TOUNICODE = (
    b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n"
    b"1 begincodespacerange <0000> <ffff> endcodespacerange\n"
    b"3 beginbfchar <0001> <0041> <0020> <0043> <0002> <0042> endbfchar\n"
    b"endcmap end end"
)


def words(tw):
    content = b"BT /F1 12 Tf 72 700 Td %d Tw <000100200002> Tj ET" % tw
    pdf = f.type0_page(content, "Identity-H", TOUNICODE, ordering="Identity")
    with t.TempDir() as d:
        path = d.write("t.pdf", pdf)
        text = utils.run("pdftotext", path, "-").strip()
        bbox = utils.run("pdftotext", "-bbox", path, "-")
    found = re.findall(r'<word xMin="([\d.-]+)" yMin="([\d.-]+)" xMax="([\d.-]+)" yMax="([\d.-]+)">([^<]*)</word>', bbox)
    return text, found


text0, words0 = words(0)
text30, words30 = words(30)
t.check(text0 == "ACB", "Tw=0 extracts ACB", repr(text0))
t.check(text30 == "ACB", "Tw=30 does not scramble the 2-byte text", repr(text30))
t.check(words0 == words30, "Tw=30 gives the same word boxes as Tw=0", "Tw=0:  %r\nTw=30: %r" % (words0, words30))
for tw, ws in ((0, words0), (30, words30)):
    ok = len(ws) == 1 and float(ws[0][2]) > float(ws[0][0])
    t.check(ok, "Tw=%d: one word with positive width" % tw, repr(ws))
t.finish()
