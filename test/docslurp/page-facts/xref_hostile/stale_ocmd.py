# Classic table. OCMD 12 has a listed body (/OCGs [10], 10 off by default: hidden) and a second,
# unlisted body later in the file (/OCGs [11], on: visible). Page 1 draws an image XObject with
# /OC 12: drawing (pdftotext without -images) never evaluates its /OC, page facts do
# (factsOcIsVisible, under a RepairGuard, no repair refused) and keep the answer in ocmdCache.
# Page 2's content (21) is in no section: its first fetch rebuilds the table, which points 12 at
# its last body. Page 2 draws a form with /OC 12. ocmdCache is not dropped on a rebuild.
import sys
out_path = sys.argv[1]
HELV = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"
def st(data, extra=b""):
    return b"<< %s/Length %d >>\nstream\n" % (extra, len(data)) + data + b"\nendstream"
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body, rec=True):
    if rec: off[n] = len(out)
    out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
put(1, b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [10 0 R 11 0 R] /D << /OFF [10 0 R] >> >> >>")
put(2, b"<< /Type /Pages /Kids [3 0 R 20 0 R] /Count 2 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im 6 0 R >> >> >>")
put(4, st(b"q 100 0 0 100 72 400 cm /Im Do Q BT /F1 12 Tf 72 700 Td (Page one) Tj ET"))
put(5, HELV)
put(6, st(b"\x00" * 12, b"/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB /OC 12 0 R "))
put(10, b"<< /Type /OCG /Name (Draft) >>")
put(11, b"<< /Type /OCG /Name (Final) >>")
put(12, b"<< /Type /OCMD /OCGs [10 0 R] >>")
put(20, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 21 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Fm 22 0 R >> >> >>")
put(21, st(b"BT /F1 12 Tf 72 700 Td (Page two) Tj ET /Fm Do"), rec=False)
put(22, st(b"BT /F1 12 Tf 72 600 Td (Quarterly beat) Tj ET", b"/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /OC 12 0 R "))
put(12, b"<< /Type /OCMD /OCGs [11 0 R] >>", rec=False)
xt = len(out)
listed = sorted(off)
out += b"xref\n0 1\n0000000000 65535 f \n" + b"".join(b"%d 1\n%010d 00000 n \n" % (n, off[n]) for n in listed)
out += b"trailer\n<< /Size 23 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % xt
open(out_path, "wb").write(out)
