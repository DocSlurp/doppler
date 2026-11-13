# xref-stream file. Shared /DecodeParms dict (80) carries a dict nested D deep before /Predictor.
# Object stream Y (60, ASCIIHex, params 80) holds the outline root (40), which only the doc line
# reads. The page content (4, Flate + TIFF predictor, params 80) is drawn. args: out D
import sys, struct, zlib
out_path, D = sys.argv[1], int(sys.argv[2])
content = b"BT /F1 24 Tf 72 700 Td (Hello) Tj ET"
C = len(content)
enc = bytes([content[0]] + [(content[i] - content[i-1]) & 0xff for i in range(1, C)])
cdata = zlib.compress(content)
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body):
    off[n] = len(out); out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
put(1, b"<< /Type /Catalog /Pages 2 0 R /Outlines 40 0 R >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>")
put(4, b"<< /Length %d /Filter /FlateDecode /DecodeParms 80 0 R >>\nstream\n" % len(cdata) + cdata + b"\nendstream")
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
junk = b"/Junk " + b"<< /A " * D + b"1 /B 2 " + b">> " * D
put(80, b"<< %s /Predictor 2 /Colors 1 /BitsPerComponent 8 /Columns %d >>" % (junk, C))
body = b"<< /Type /Outlines /Count 0 >>"
hdr = b"40 0 "
data = (hdr + body).hex().encode() + b">"
put(60, b"<< /Type /ObjStm /N 1 /First %d /Filter /ASCIIHexDecode /DecodeParms 80 0 R /Length %d >>\nstream\n" % (len(hdr), len(data)) + data + b"\nendstream")
xr = len(out)
ents = {0: (0, 0, 255)}
for n in (1, 2, 3, 4, 5, 60, 80): ents[n] = (1, off[n], 0)
ents[40] = (2, 60, 0); ents[90] = (1, xr, 0)
size = 91
rows = b"".join(struct.pack(">BIH", *ents.get(i, (0, 0, 0))) for i in range(size))
out += b"90 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /W [1 4 2] /Length %d >>\nstream\n" % (size, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xr
open(out_path, "wb").write(out)
