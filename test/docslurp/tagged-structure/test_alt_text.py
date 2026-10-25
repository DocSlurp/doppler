"""Fix 5: TextStringToUtf8 must not append a NUL byte, and must produce UTF-8."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tagged_pdf as tp  # noqa: E402

t = tp.t
utils = t.Utils.from_args()

content = b"/Figure <</MCID 0>> BDC BT /F1 12 Tf 72 700 Td (x) Tj ET EMC"


def alt_line(alt_literal):
    root = tp.Elem("Document", [tp.Elem("Figure", [tp.MC(0, 0)], pg=0, extra="/Alt %s" % alt_literal)])
    pdf = tp.build_tagged([content], root)
    with t.TempDir() as d:
        path = d.write("alt.pdf", pdf)
        out = utils.run("pdfinfo", "-struct", path, binary=True)
    for line in out.splitlines():
        if line.lstrip().startswith(b"Figure"):
            return line.strip()
    return out


line = alt_line("(A chart)")
t.check(line.endswith(b'["A chart"]'), "ASCII alt text is printed without a trailing NUL", line)
t.check(b"?" not in line and b"\0" not in line, "no NUL or placeholder after the alt text", line)

# PDFDocEncoding bytes above 0x7f are converted to UTF-8.
line = alt_line(r"(caf\351 \200 bullet)")
t.check(line.endswith('["café • bullet"]'.encode()), "PDFDocEncoding alt text is UTF-8", line)

# UTF-16BE with byte order mark.
line = alt_line("<FEFF00430068006100720074>")
t.check(line.endswith(b'["Chart"]'), "UTF-16BE alt text", line)

t.finish()
