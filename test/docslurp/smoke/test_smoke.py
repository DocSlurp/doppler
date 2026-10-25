"""Smoke test for the DocSlurp test scaffold: a generated PDF round-trips through pdftotext."""

import os
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()
pdf = t.simple_page_pdf(b"BT /F1 12 Tf 72 700 Td (Hello DocSlurp) Tj ET")
with t.TempDir() as d:
    path = d.write("hello.pdf", pdf)
    out = utils.run("pdftotext", path, "-")
    t.check("Hello DocSlurp" in out, "pdftotext extracts generated text", repr(out))
    info = utils.run("pdfinfo", path)
    t.check("Pages:           1" in info, "pdfinfo reads generated PDF", info)
t.finish()
