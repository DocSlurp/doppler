# main lists 0-3,5 (catalog, pages, page, font); the oldest of K older sections lists 4 (content);
# the K-1 between are empty incremental updates. usage: manysec.py K out.pdf
import sys
K = int(sys.argv[1])
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
objs = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
out = bytearray(b"%PDF-1.4\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
prev = len(out)
out += b"xref\n0 1\n0000000000 65535 f \n4 1\n%010d 00000 n \ntrailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (off[4], prev)
for k in range(K - 1):
    at = len(out)
    out += b"xref\n0 0\ntrailer\n<< /Size 6 /Root 1 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n" % (prev, at)
    prev = at
x = len(out)
out += b"xref\n0 4\n0000000000 65535 f \n" + b"".join(b"%010d 00000 n \n" % off[n] for n in (1, 2, 3)) + b"5 1\n%010d 00000 n \n" % off[5]
out += b"trailer\n<< /Size 6 /Root 1 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n" % (prev, x)
open(sys.argv[2], "wb").write(out)
