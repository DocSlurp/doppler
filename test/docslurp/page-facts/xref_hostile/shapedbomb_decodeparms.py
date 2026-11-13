# N image draws, each image's /OC a direct OCMD whose /OCGs is an indirect array kept in a
# Flate object stream that also holds a STR-byte string. Drawing never fetches it (images are
# not drawn by text devices); each guarded scope (one per draw) parses the object stream.
import sys, struct, zlib
out_path = sys.argv[1]; N = int(sys.argv[2]); STR = int(sys.argv[3])
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET\n" + b"q 10 0 0 10 72 72 cm /Im0 Do Q\n" * N
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body):
    off[n] = len(out); out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
put(1, b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [20 0 R] /D << /Order [20 0 R] >> >> >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 7 0 R >> >> >>")
cz = zlib.compress(content, 9)
put(4, b"<< /Length %d /Filter /FlateDecode >>\nstream\n" % len(cz) + cz + b"\nendstream")
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
put(7, b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /OC << /Type /OCMD /VE 90 0 R >> /Length 1 >>\nstream\n\x80\nendstream")
put(20, b"<< /Type /OCG /Name (Layer) >>")
m90 = b"[/Or 20 0 R] "
m91 = b"<" + b"0" * STR + b">"
hdr = b"90 0 91 %d " % len(m90)
data = hdr + m90 + m91
dz = zlib.compress(data, 9)
put(50, b"<< /Type /ObjStm /N 2 /First %d /Filter /FlateDecode /DecodeParms 90 0 R /Length %d >>\nstream\n" % (len(hdr), len(dz)) + dz + b"\nendstream")
xr = len(out)
ents = {0: (0, 0, 255)}
for n in off: ents[n] = (1, off[n], 0)
ents[90] = (2, 50, 0); ents[91] = (2, 50, 1); ents[92] = (1, xr, 0)
size = 93
rows = b"".join(struct.pack(">BIH", *ents.get(i, (0, 0, 0))) for i in range(size))
out += b"92 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /W [1 4 2] /Length %d >>\nstream\n" % (size, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xr
open(out_path, "wb").write(out)
