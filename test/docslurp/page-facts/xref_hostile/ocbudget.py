# Form XObject with a direct OCMD dict (/OCGs of K refs) invoked N times, then an image XObject
# whose /OC is an indirect OCMD over an OFF group.
import sys
out_path = sys.argv[1]; K = int(sys.argv[2]); N = int(sys.argv[3])
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET\n" + b"q /Fm0 Do Q\n" * N + b"q 100 0 0 100 72 400 cm /Im0 Do Q\n"
ocgs = b"[" + b" ".join(b"20 0 R" for _ in range(K)) + b"]"
form = b"0 0 m 10 10 l S"
objs = {1: b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [20 0 R] /D << /OFF [20 0 R] /Order [20 0 R] >> >> >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Fm0 6 0 R /Im0 7 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        6: b"<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /OC << /Type /OCMD /OCGs " + ocgs + b" /P /AnyOff >> /Length %d >>\nstream\n" % len(form) + form + b"\nendstream",
        7: b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /OC 30 0 R /Length 1 >>\nstream\n\x80\nendstream",
        20: b"<< /Type /OCG /Name (Layer) >>",
        30: b"<< /Type /OCMD /OCGs [20 0 R] /P /AllOn >>"}
out = bytearray(b"%PDF-1.5\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
x = len(out); size = 31
out += b"xref\n0 %d\n" % size
for i in range(size):
    out += (b"%010d 00000 n \n" % off[i]) if i in off else b"0000000000 65535 f \n"
out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (size, x)
open(out_path, "wb").write(out)
