"""Fix 1: marked-content begin/end must stay balanced, so tagged text is not truncated."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tagged_pdf as tp  # noqa: E402

t = tp.t
utils = t.Utils.from_args()


def struct_text(content, extra_resources=""):
    """Return (struct text lines, pdftotext output) for a one-page tagged PDF."""
    pdf = tp.build_tagged([content], tp.Elem("Document", [tp.Elem("P", [tp.MC(0, 0)], pg=0)]), page_resources=extra_resources)
    with t.TempDir() as d:
        path = d.write("mc.pdf", pdf)
        info = utils.run("pdfinfo", "-struct-text", path)
        txt = utils.run("pdftotext", path, "-")
    quoted = [line.strip() for line in info.splitlines() if line.strip().startswith('"')]
    return quoted, txt


# Inline-dictionary property list nested inside an MCID.
q, _ = struct_text(b"/P<</MCID 0>>BDC BT /F1 12 Tf 72 700 Td (Hello ) Tj /Span<</Lang(fr)>>BDC (Bonjour ) Tj EMC (World) Tj ET EMC")
t.check(q == ['"Hello Bonjour World"'], "inline-dict Span nested in MCID keeps trailing text", q)

# Named property list (/Properties resource) nested inside an MCID.
q, _ = struct_text(
    b"/P<</MCID 0>>BDC BT /F1 12 Tf 72 700 Td (Hello ) Tj /OC /oc1 BDC (Layer ) Tj EMC (World) Tj ET EMC",
    extra_resources="/Properties << /oc1 << /Type /OCG /Name (L) >> >>",
)
t.check(q == ['"Hello Layer World"'], "named /OC property list nested in MCID keeps trailing text", q)

q, _ = struct_text(
    b"/P<</MCID 0>>BDC BT /F1 12 Tf 72 700 Td (Hello ) Tj /Span /MC0 BDC (Named ) Tj EMC (World) Tj ET EMC",
    extra_resources="/Properties << /MC0 << /Lang (fr) >> >>",
)
t.check(q == ['"Hello Named World"'], "named /Span property list nested in MCID keeps trailing text", q)

# Marked content without properties (BMC) nested inside an MCID.
q, _ = struct_text(b"/P<</MCID 0>>BDC BT /F1 12 Tf 72 700 Td (Hello ) Tj /Artifact BMC (Plain ) Tj EMC (World) Tj ET EMC")
t.check(q == ['"Hello Plain World"'], "BMC nested in MCID keeps trailing text", q)

# Two levels of non-MCID nesting.
q, _ = struct_text(b"/P<</MCID 0>>BDC BT /F1 12 Tf 72 700 Td (A ) Tj /Span<</Lang(fr)>>BDC (B ) Tj /Span<</Lang(de)>>BDC (C ) Tj EMC (D ) Tj EMC (E) Tj ET EMC")
t.check(q == ['"A B C D E"'], "two nested levels keep all text", q)

# The text after the MCID must not leak into the element.
q, txt = struct_text(b"BT /F1 12 Tf 72 700 Td /P<</MCID 0>>BDC (In ) Tj /Span<</Lang(fr)>>BDC (Mid ) Tj EMC (End) Tj EMC (After) Tj ET")
t.check(q == ['"In Mid End"'], "text after EMC is not part of the element", q)
t.check("After" in txt, "pdftotext still extracts the text after the MCID", txt)

t.finish()
