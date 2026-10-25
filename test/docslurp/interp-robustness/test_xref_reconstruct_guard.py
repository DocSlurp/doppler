"""Guard: a reference past the end of the xref table still triggers reconstruction.

XRef::fetch() rebuilds the xref table when it meets an object number >= /Size.
That costs a full scan of the file (about +20% CPU on a 37 MB scan with a single
dangling reference), and it is tempting to return null instead.  It is not safe:
the same path is what recovers objects that exist in the file but are missing
from a damaged xref table (checked below).  It is also what recovers encrypted
files with object streams and a damaged startxref: the first reconstruction at
open time cannot decrypt the object streams, and only the one triggered from
fetch() can (checked by hand with qpdf, not here).  This test pins the current
behaviour so a future "optimization" has to keep these properties.
"""

import os
import re
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(here)))
sys.path.insert(0, here)
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()

with t.TempDir() as d:
    # 1. A dangling font reference does not break the other fonts on the page.
    resources = "<< /Font << /F1 99 0 R /F2 4 0 R >> >>"
    content = b"BT /F1 12 Tf 72 700 Td (LOST) Tj /F2 12 Tf 0 -50 Td (VALID) Tj ET"
    path = d.write("dangling.pdf", t.simple_page_pdf(content, resources=resources))
    out = utils.run("pdftotext", path, "-")
    t.check("VALID" in out, "dangling ref: other fonts still extract", repr(out))

    # 2. Objects 4 (font) and 5 (contents) are in the file but missing from the
    #    xref table (which only lists 0-3) and from /Size: they must be recovered.
    pdf = t.simple_page_pdf(b"BT /F1 12 Tf 72 700 Td (RECOVERED) Tj ET")
    pdf = pdf.replace(b"xref\n0 6\n", b"xref\n0 4\n")
    pdf = re.sub(rb"(xref\n0 4\n(?:\d{10} \d{5} [nf] \n){4})(?:\d{10} \d{5} n \n){2}", rb"\1", pdf)
    pdf = pdf.replace(b"/Size 6", b"/Size 4")
    assert b"0 4\n" in pdf and b"/Size 4" in pdf
    path = d.write("short-xref.pdf", pdf)
    out = utils.run("pdftotext", path, "-")
    t.check("RECOVERED" in out, "objects missing from the xref table are recovered", repr(out))
t.finish()
