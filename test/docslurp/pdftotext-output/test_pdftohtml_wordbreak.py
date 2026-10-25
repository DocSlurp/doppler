"""Characterisation test (passes before and after this branch): pdftohtml's
word-break condition in HtmlPage::addChar is `gap too big && rotation differs
from the current string's font rotation`. Despite looking like it needs `||`,
it is correct: for unrotated text the string font's rotation matrix is the zero
matrix, which never equals the text matrix, so a large gap alone splits the
string; for rotated text the x-gap test is meaningless, and the matching
rotation prevents splitting. Changing `&&` to `||` would split rotated text
into one string per few characters. This test pins both behaviours."""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t  # noqa: E402

utils = t.Utils.from_args()
content = b"BT /F1 12 Tf 72 700 Td [(Left) -9000 (Right)] TJ ET BT /F1 12 Tf 0 1 -1 0 300 300 Tm (Rotated text) Tj ET"
with t.TempDir() as d:
    path = d.write("gap.pdf", t.simple_page_pdf(content))
    out = utils.run("pdftohtml", "-xml", "-i", "-stdout", "-q", path)
    texts = re.findall(r"<text [^>]*>(.*?)</text>", out)
    t.check("Left" in texts and "Right" in texts, "a large gap between unrotated words splits the string", repr(texts))
    t.check("Rotated text" in texts, "rotated text stays in one string", repr(texts))
t.finish()
