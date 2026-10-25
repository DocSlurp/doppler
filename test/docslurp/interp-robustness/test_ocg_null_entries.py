"""Null or dangling entries in the OCG arrays must not hide the entries after them.

OCGs::OCGs() used to `break` out of the /OCGs, /D /ON and /D /OFF loops at the
first entry that was not a usable reference, although the code itself says
null entries occur.  `/OFF [null 6 0 R]` then left the hidden layer visible
and its text leaked into pdftotext, while `/OFF [6 0 R null]` hid it.
"""

import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(here)))
sys.path.insert(0, here)
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()

# Objects 6 and 7 are the layers "ALPHA" and "BETA".
LAYERS = ["<< /Type /OCG /Name (A) >>", "<< /Type /OCG /Name (B) >>"]
RESOURCES = "<< /Font << /F1 4 0 R >> /Properties << /L1 6 0 R /L2 7 0 R >> >>"
CONTENT = (
    b"BT /F1 12 Tf 72 700 Td (PLAIN) Tj ET "
    b"/OC /L1 BDC BT /F1 12 Tf 72 650 Td (ALPHA) Tj ET EMC "
    b"/OC /L2 BDC BT /F1 12 Tf 72 600 Td (BETA) Tj ET EMC"
)

# name -> (/OCProperties body, words that must be extracted)
CASES = [
    ("OFF [6 0 R null] (control)", "/OCGs [6 0 R 7 0 R] /D << /OFF [6 0 R null] >>", ["PLAIN", "BETA"]),
    ("OFF [null 6 0 R]", "/OCGs [6 0 R 7 0 R] /D << /OFF [null 6 0 R] >>", ["PLAIN", "BETA"]),
    ("OFF [null 6 0 R null 7 0 R]", "/OCGs [6 0 R 7 0 R] /D << /OFF [null 6 0 R null 7 0 R] >>", ["PLAIN"]),
    ("OFF [42 6 0 R] (integer entry)", "/OCGs [6 0 R 7 0 R] /D << /OFF [42 6 0 R] >>", ["PLAIN", "BETA"]),
    ("OFF [99 0 R 6 0 R] (not a group)", "/OCGs [6 0 R 7 0 R] /D << /OFF [99 0 R 6 0 R] >>", ["PLAIN", "BETA"]),
    ("OCGs [null 6 0 R 7 0 R]", "/OCGs [null 6 0 R 7 0 R] /D << /OFF [6 0 R] >>", ["PLAIN", "BETA"]),
    ("OCGs [99 0 R 6 0 R 7 0 R]", "/OCGs [99 0 R 6 0 R 7 0 R] /D << /OFF [6 0 R] >>", ["PLAIN", "BETA"]),
    ("BaseState OFF, ON [null 7 0 R]", "/OCGs [6 0 R 7 0 R] /D << /BaseState /OFF /ON [null 7 0 R] >>", ["PLAIN", "BETA"]),
    ("BaseState OFF, ON [99 0 R 7 0 R]", "/OCGs [6 0 R 7 0 R] /D << /BaseState /OFF /ON [99 0 R 7 0 R] >>", ["PLAIN", "BETA"]),
]

with t.TempDir() as d:
    for i, (name, props, expected) in enumerate(CASES):
        pdf = t.simple_page_pdf(CONTENT, resources=RESOURCES, extra_objects=LAYERS, catalog_extra="/OCProperties << %s >>" % props)
        path = d.write("ocg%d.pdf" % i, pdf)
        words = utils.run("pdftotext", path, "-").split()
        t.check(sorted(words) == sorted(expected), "%s: extracted %s" % (name, expected), repr(words))
t.finish()
