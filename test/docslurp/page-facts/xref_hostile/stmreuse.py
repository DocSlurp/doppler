# Many tiny classic older sections, each naming the SAME xref stream via /XRefStm. The stream
# lists K entries (W [4 8 8], PNG Paeth predictor, multi-Flate). args: out N K layers
import sys, zlib
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
raw = (b"\x04" + b"\x00" * 20) * K
data = raw
for _ in range(layers): data = zlib.compress(data, 9)
filt = b"[" + b"/FlateDecode " * layers + b"]"
spos = len(out)
out += b"9 0 obj\n<< /Type /XRef /Size %d /Index [0 %d] /W [4 8 8] /Filter %s /DecodeParms [%s<< /Predictor 12 /Columns 20 >>] /Length %d >>\nstream\n" % (K, K, filt, b"null " * (layers - 1), len(data)) + data + b"\nendstream\nendobj\n"
prev = None
for s in range(N):
    pos = len(out)
    t = b"xref\ntrailer\n<<"
    if prev is not None: t += b"/Prev %d" % prev
    t += b"/XRefStm %d>>\n" % spos
    out += t; prev = pos
mainpos = len(out)
out += b"xref\n0 6\n0000000000 65535 f \n" + b"".join(b"%010d 00000 n \n" % off[n] for n in range(1, 6))
out += b"trailer\n<< /Size %d /Root 1 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n" % (K, prev, mainpos)
open(out_path, "wb").write(out)
