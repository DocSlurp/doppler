"""Fix 2: /ActualText is honoured on any marked-content tag, and nested ActualText works."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tagged_pdf as tp  # noqa: E402

t = tp.t
utils = t.Utils.from_args()


def text_of(body, resources_extra=""):
    content = b"BT /F1 12 Tf 72 700 Td " + body + b" ET"
    res = "<< /Font << /F1 4 0 R >> %s >>" % resources_extra
    pdf = t.simple_page_pdf(content, resources=res)
    with t.TempDir() as d:
        path = d.write("at.pdf", pdf)
        return utils.run("pdftotext", path, "-").replace("\f", "").strip()


# ActualText on a non-Span tag (here /P with an MCID).
out = text_of(b"/P <</MCID 0 /ActualText (PARA)>> BDC (yyyy) Tj EMC")
t.check(out == "PARA", "ActualText on /P is honoured", repr(out))

# ActualText on arbitrary tag names.
for tag in ("Figure", "Link", "H1", "Artifact", "MyTag"):
    out = text_of(b"/%s <</ActualText (REPL)>> BDC (original) Tj EMC" % tag.encode())
    t.check(out == "REPL", "ActualText on /%s is honoured" % tag, repr(out))

# Named property list carrying ActualText.
out = text_of(b"/P /MC0 BDC (yyyy) Tj EMC", resources_extra="/Properties << /MC0 << /MCID 0 /ActualText (NAMED) >> >>")
t.check(out == "NAMED", "ActualText in a named property list (/P) is honoured", repr(out))

# Still works on /Span.
out = text_of(b"/Span <</ActualText (SPAN)>> BDC (yyyy) Tj EMC")
t.check(out == "SPAN", "ActualText on /Span still honoured", repr(out))

# Nested ActualText: the outermost replacement wins, inner glyphs are suppressed.
out = text_of(b"/Span <</ActualText (OUTER)>> BDC (vv) Tj /Span <</ActualText (IN)>> BDC (zz) Tj EMC (ww) Tj EMC")
t.check(out == "OUTER", "nested ActualText: outermost wins", repr(out))

out = text_of(b"/P <</ActualText (OUTER)>> BDC /Span <</ActualText (IN)>> BDC (zz) Tj EMC /Span <</Lang (en)>> BDC (ww) Tj EMC EMC")
t.check(out == "OUTER", "nested ActualText with a plain span in between", repr(out))

# Non-ActualText marked content in between must not end the ActualText early.
out = text_of(b"/P <</ActualText (OUTER)>> BDC (aa) Tj /Span <</Lang (en)>> BDC (bb) Tj EMC (cc) Tj EMC (after) Tj")
t.check(out == "OUTERafter", "plain nested span does not end ActualText", repr(out))

# Sequential (non-nested) ActualText sequences are both replaced.
out = text_of(b"/Span <</ActualText (ONE)>> BDC (xx) Tj EMC ( ) Tj /P <</ActualText (TWO)>> BDC (yy) Tj EMC")
t.check(out == "ONE TWO", "sequential ActualText sequences", repr(out))

# Text following a nested pair is not swallowed.
out = text_of(b"/Span <</ActualText (OUTER)>> BDC /Span <</ActualText (IN)>> BDC (zz) Tj EMC EMC ( tail) Tj")
t.check(out == "OUTER tail", "text after nested ActualText survives", repr(out))

t.finish()
