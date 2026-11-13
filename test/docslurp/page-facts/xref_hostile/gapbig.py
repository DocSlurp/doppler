# older section lists 0,1 (catalog); main section lists 2 (free) and 4-7; entry 3 in no section
import sys
out_path = sys.argv[1]
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
objs = {1: b"<< /Type /Catalog /Pages 4 0 R >>", 4: b"<< /Type /Pages /Kids [5 0 R] /Count 1 >>",
        5: b"<< /Type /Page /Parent 4 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 7 0 R >> >> >>",
        6: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        7: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
out = bytearray(b"%PDF-1.4\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
p = len(out)
out += b"xref\n0 2\n0000000000 65535 f \n%010d 00000 n \ntrailer\n<< /Size 5000 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (off[1], p)
x = len(out)
out += b"xref\n2 1\n0000000000 00000 f \n4 4\n" + b"".join(b"%010d 00000 n \n" % off[n] for n in (4, 5, 6, 7))
out += b"trailer\n<< /Size 5000 /Root 1 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n" % (p, x)
open(out_path, "wb").write(out)
