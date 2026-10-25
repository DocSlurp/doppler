#!/usr/bin/env python3
"""Round-2 fixture: MCID collision between page content and a reused Form XObject, plus ActualText spans.
Struct tree is intentionally absent: the question is only what scope key the content-side join needs."""
import sys
objs = {}
def stream(d, body): return (d % len(body)).encode() + b"\nstream\n" + body + b"\nendstream"
page_ops = b"""/P <</MCID 0>> BDC BT /F1 12 Tf 72 700 Td (PAGE0) Tj ET EMC
q 1 0 0 1 72 650 cm /Fm1 Do Q
q 1 0 0 1 72 600 cm /Fm1 Do Q
/P <</MCID 1>> BDC BT /F1 12 Tf 72 550 Td /Span <</ActualText (fi)>> BDC (ab) Tj EMC ( tail) Tj ET EMC
/P <</MCID 2>> BDC BT /F1 12 Tf 72 520 Td /Span /MCA BDC (zz) Tj EMC ( after) Tj ET EMC
/P <</MCID 3>> BDC BT /F1 12 Tf 72 490 Td (lig) Tj /Span <</ActualText (X)>> BDC (ab) Tj /P <</MCID 4>> BDC (cd) Tj EMC EMC ( end) Tj ET EMC
"""
form_ops = b"/P <</MCID 0>> BDC BT /F1 12 Tf 0 0 Td (FORM0) Tj ET EMC"
objs[1] = b"<< /Type /Catalog /Pages 2 0 R >>"
objs[2] = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"
objs[3] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Fm1 6 0 R >> /Properties << /MCA << /ActualText (YY) >> >> >> >>"
objs[4] = stream("<< /Length %d >>", page_ops)
objs[5] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>"
objs[6] = stream("<< /Type /XObject /Subtype /Form /BBox [0 0 300 50] /Resources << /Font << /F1 5 0 R >> >> /Length %d >>", form_ops)
out = bytearray(b"%PDF-1.7\n"); offs = {}
for n in sorted(objs):
    offs[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
x = len(out); out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs)+1)
for n in sorted(objs): out += b"%010d 00000 n \n" % offs[n]
out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs)+1, x)
open(sys.argv[1], "wb").write(out)
