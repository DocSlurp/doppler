# N older xref streams, each /W [4 8 8] /Index [0 K], data all zero, Flate (optionally double).
# All page objects are in the main (classic? no: stream) section. args: out N K layers
import sys, zlib, struct
out_path, N, K, layers = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
content = b"BT /F1 24 Tf 72 700 Td (Hello) Tj ET"
def put(n, body):
    off[n] = len(out); out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
put(1, b"<< /Type /Catalog /Pages 2 0 R >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>")
put(4, b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream")
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
raw = (b"\x04" + b"\x00"*20) * K
data = raw
for _ in range(layers): data = zlib.compress(data, 9)
filt = b"[" + b"/FlateDecode " * layers + b"]"
prev = None
objnum = 100
for s in range(N):
    pos = len(out)
    d = b"<< /Type /XRef /Size %d /Index [0 %d] /W [4 8 8] /Filter %s /DecodeParms [%s<< /Predictor 12 /Columns 20 >>] /Length %d" % (K, K, filt, b"null " * (layers-1), len(data))
    if prev is not None: d += b" /Prev %d" % prev
    d += b" >>"
    out += b"%d 0 obj\n" % objnum + d + b"\nstream\n" + data + b"\nendstream\nendobj\n"
    prev = pos; objnum += 1
# main xref stream listing 0..5 and itself
mainpos = len(out)
rows = b"".join(b"\x01" + struct.pack(">I", off[n]) + b"\x00" for n in range(1, 6))
rows = b"\x00\x00\x00\x00\x00\xff" + rows + b"\x01" + struct.pack(">I", mainpos) + b"\x00"
out += b"%d 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /Index [0 6 %d 1] /W [1 4 1] /Prev %d /Length %d >>\nstream\n" % (objnum, max(K, objnum+1), objnum, prev, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % mainpos
open(out_path, "wb").write(out)
