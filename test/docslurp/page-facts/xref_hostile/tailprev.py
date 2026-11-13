# main section lists 1,2,3,5; an older section (behind /Prev) lists 4 and sits at the very end
# of the file, its trailer followed only by "%%EOF". mode "eof" | "startxref"
import sys
mode = sys.argv[2]
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
objs = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
out = bytearray(b"%PDF-1.4\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
x = len(out)
main = b"xref\n0 4\n0000000000 65535 f \n" + b"".join(b"%010d 00000 n \n" % off[n] for n in (1, 2, 3)) + b"5 1\n%010d 00000 n \n" % off[5]
main += b"trailer\n<< /Size 6 /Root 1 0 R /Prev 0000000000 >>\nstartxref\n%d\n" % x
out += main
y = len(out)
out += b"xref\n4 1\n%010d 00000 n \ntrailer\n<< /Size 6 >>\n" % off[4]
if mode == "startxref": out += b"startxref\n%d\n" % x
out += b"%%EOF\n"
out = out.replace(b"/Prev 0000000000", b"/Prev %010d" % y)
open(sys.argv[1], "wb").write(out)
