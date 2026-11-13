# Broken startxref (rebuild at open). T lines "trailer << /A [" ... each array runs to the end of
# the file (never closed), so every trailer's parse reads the rest of the file. usage: T pad out
import sys
T, P, out_path = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
out = bytearray(b"%PDF-1.4\n")
out += b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
out += b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n"
out += b"4 0 obj\n<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream\nendobj\n5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
out += b"trailer\n<< /Size 6 /Root 1 0 R >>\n"
for i in range(T):
    out += b"trailer\n<< /Root 1 0 R /A [\n"
out += b"0 " * P + b"\n"
out += b"startxref\n999999999\n%%EOF\n"
open(out_path, "wb").write(out)
