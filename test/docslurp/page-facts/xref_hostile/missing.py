# classic single-section file; the page's content stream (4) and font (5) are absent from the
# xref table (subsections 0-3 only), present in the body. mode: "single" | "prev"
import sys
out_path, mode = sys.argv[1], sys.argv[2]
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
objs = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
out = bytearray(b"%PDF-1.4\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
if mode == "single":
    x = len(out)
    out += b"xref\n0 4\n0000000000 65535 f \n" + b"".join(b"%010d 00000 n \n" % off[n] for n in (1, 2, 3))
    out += b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % x
else:
    # older section lists 1 (catalog) only; main lists 2,3; 4,5 in none
    p = len(out)
    out += b"xref\n0 2\n0000000000 65535 f \n%010d 00000 n \ntrailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (off[1], p)
    x = len(out)
    out += b"xref\n2 2\n" + b"".join(b"%010d 00000 n \n" % off[n] for n in (2, 3))
    out += b"trailer\n<< /Size 6 /Root 1 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n" % (p, x)
open(out_path, "wb").write(out)
