import sys
HELV=b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
def st(c, d=b""): return b"<< %s/Length %d >>\nstream\n" % (d, len(c)) + c + b"\nendstream"
def build(listed_missing=(30,), page2_contents=30, use_form_p3=True):
    objs = {}
    objs[1] = b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [20 0 R] /D << /ON [20 0 R] >> >> >>"
    objs[2] = b"<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>"
    res = b"/Font << /F1 9 0 R >> /XObject << /Im 10 0 R /Fm 11 0 R >>"
    objs[3] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << %s >> >>" % res
    objs[4] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents %d 0 R /Resources << %s >> >>" % (page2_contents, res)
    objs[5] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 8 0 R /Resources << %s >> >>" % res
    objs[6] = st(b"q 100 0 0 100 72 400 cm /Im Do Q BT /F1 12 Tf 72 700 Td (PageOne) Tj ET")
    objs[8] = st(b"/Fm Do BT /F1 12 Tf 72 700 Td (PageThree) Tj ET")
    objs[9] = HELV
    objs[10] = st(b"\x00"*12, b"/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB /OC 21 0 R ")
    objs[11] = st(b"0 0 0 RG 1 w 72 500 m 472 500 l S 72 450 m 472 450 l S BT /F1 12 Tf 72 600 Td (InsideForm) Tj ET", b"/Type /XObject /Subtype /Form /BBox [0 0 612 792] /OC 21 0 R /Resources << /Font << /F1 9 0 R >> >> ")
    objs[20] = b"<< /Type /OCG /Name (L) >>"
    objs[21] = b"<< /Type /OCMD /OCGs [20 0 R] /P /AnyOn >>"
    objs[30] = st(b"BT /F1 12 Tf 72 700 Td (PageTwo) Tj ET")
    out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
    for n in sorted(objs):
        off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
    out += b"21 0 obj\n<< /Type /OCMD /OCGs [20 0 R] /P /AllOff >>\nendobj\n"
    x = len(out)
    listed = [n for n in sorted(objs) if n not in listed_missing]
    out += b"xref\n0 1\n0000000000 65535 f \n" + b"".join(b"%d 1\n%010d 00000 n \n" % (n, off[n]) for n in listed)
    out += b"trailer\n<< /Size 31 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % x
    return bytes(out)
open(sys.argv[1], "wb").write(build())
