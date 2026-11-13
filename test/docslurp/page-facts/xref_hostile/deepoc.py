# Image XObject /OC 30 0 R; 30 (OCMD over OFF group 20) in object stream 60 next to member 31,
# an array nested DEPTH deep. Classic xref table + xref stream for objstm entries.
import sys, struct
out_path = sys.argv[1]; DEPTH = int(sys.argv[2]); HIDE = sys.argv[3] if len(sys.argv) > 3 else "off"
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET\nq 100 0 0 100 72 72 cm /Im0 Do Q\n"
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body):
    off[n] = len(out); out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
d = b"/D << /Order [20 0 R] /OFF [20 0 R] >>" if HIDE == "off" else b"/D << /Order [20 0 R] >>"
put(1, b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [20 0 R] " + d + b" >> >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 7 0 R >> >> >>")
put(4, b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream")
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
put(7, b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /OC 30 0 R /Length 1 >>\nstream\n\x80\nendstream")
put(20, b"<< /Type /OCG /Name (Layer) >>")
m30 = b"<< /Type /OCMD /OCGs 20 0 R /P /AllOn >> "
m31 = b"[" * DEPTH + b"1" + b"]" * DEPTH + b" "
hdr = (b"30 0 31 %d " % len(m30)).ljust(24)
data = hdr + m30 + m31
put(60, b"<< /Type /ObjStm /N 2 /First %d /Length %d >>\nstream\n" % (len(hdr), len(data)) + data + b"\nendstream")
xr = len(out)
ents = {0: (0, 0, 255)}
for n in off: ents[n] = (1, off[n], 0)
ents[30] = (2, 60, 0); ents[31] = (2, 60, 1); ents[95] = (1, xr, 0)
size = 96
rows = b"".join(struct.pack(">BIH", *ents.get(i, (0, 0, 0))) for i in range(size))
out += b"95 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /W [1 4 2] /Length %d >>\nstream\n" % (size, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xr
open(out_path, "wb").write(out)
