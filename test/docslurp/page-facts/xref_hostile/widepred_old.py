# xref-stream file saved twice: the older xref stream (PNG predictor /Columns C, padded) lists
# every object; the newer one (plain) lists only 1,2 and itself. usage: widepred_old.py C out.pdf
import sys, zlib, struct
C = int(sys.argv[1])
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
objs = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
xat = len(out)
rows = bytearray(b"\x00\x00\x00\x00\x00\xff")
for n in range(1, 6): rows += b"\x01" + struct.pack(">I", off[n]) + b"\x00"
rows += b"\x01" + struct.pack(">I", xat) + b"\x00"
rows += b"\x00" * ((-len(rows)) % C)
enc = bytearray()
for i in range(0, len(rows), C): enc += b"\x00" + rows[i:i + C]
data = zlib.compress(bytes(enc), 9)
out += b"6 0 obj\n<< /Type /XRef /Size 7 /W [1 4 1] /Root 1 0 R /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns %d >> /Length %d >>\nstream\n" % (C, len(data)) + data + b"\nendstream\nendobj\n"
o2 = len(out); out += b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
x2 = len(out)
r2 = b"\x01" + struct.pack(">I", o2) + b"\x00" + b"\x01" + struct.pack(">I", x2) + b"\x00"
out += b"7 0 obj\n<< /Type /XRef /Size 8 /Index [2 1 7 1] /W [1 4 1] /Root 1 0 R /Prev %d /Length %d >>\nstream\n" % (xat, len(r2)) + r2 + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % x2
open(sys.argv[2], "wb").write(out)
