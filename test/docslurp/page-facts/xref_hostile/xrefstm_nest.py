# An older section (behind /Prev) whose trailer's /XRefStm points into its own string /P, where
# another "xref trailer << /XRefStm .. /P ( ... ) >>" begins, n levels deep; the innermost string
# holds F filler bytes. Each nested readXRef re-lexes everything after its start, and the byte
# budget is charged only when each level returns. usage: xrefstm_nest.py n F out.pdf [main]
import sys
n = int(sys.argv[1]); F = int(sys.argv[2]); out_path = sys.argv[3]; in_main = len(sys.argv) > 4 and sys.argv[4] == "main"
content = b"BT /F1 12 Tf 72 740 Td (Hello world) Tj ET"
objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
}
out = bytearray(b"%PDF-1.4\n"); off = {}
for n_ in sorted(objs):
    off[n_] = len(out); out += b"%d 0 obj\n" % n_ + objs[n_] + b"\nendobj\n"
entries = b"0000000000 65535 f \n" + b"".join(b"%010d 00000 n \n" % off[i] for i in range(1, 6))
start = len(out)
PRE = b"xref\ntrailer\n<< /XRefStm %010d /P ("
if in_main:
    PRE0 = b"xref\n0 6\n" + entries + b"trailer\n<< /Size 6 /Root 1 0 R /XRefStm %010d /P ("
else:
    PRE0 = PRE
blob = bytearray()
pos = start
pres = []
for k in range(n):
    pre = PRE0 if k == 0 else PRE
    nxt = pos + len(pre) - 0  # next level begins right after this prefix
    blob += pre % nxt if k < n - 1 else pre.replace(b"/XRefStm %010d ", b"") 
    pos = start + len(blob)
blob += b"x" * F
blob += b") >>\n" * n
out += blob
if in_main:
    sx = start
else:
    main_at = len(out)
    out += b"xref\n0 6\n" + entries + b"trailer\n<< /Size 6 /Root 1 0 R /Prev %d >>\n" % start
    sx = main_at
out += b"startxref\n%d\n%%%%EOF\n" % sx
open(out_path, "wb").write(out)
