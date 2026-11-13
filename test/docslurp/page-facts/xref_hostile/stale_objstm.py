# Hybrid file (classic main table + /XRefStm). Object stream 20 holds the outline root (30) and
# the font's /Encoding (31). Object 20 has a second, unlisted body later in the file whose
# /Differences differ. The page's content (4) is in no section, so the first fetch of it rebuilds
# the table (rebuildOnMissing); the rebuild points 20 at its last body. objStrs is not cleared by
# constructXRef. With page facts, the doc line's guarded /Outlines read parses and caches body 1
# of 20 before the rebuild (no repair refused, so it is kept); without facts, 20 is first parsed
# after the rebuild (body 2). Mode arg: "outline" (doc line) or "none".
import sys, struct
out_path = sys.argv[1]
content = b"BT /F1 24 Tf 72 700 Td (ABCABC) Tj ET"
def objstm(enc):
    o30 = b"<< /Type /Outlines /Count 0 >>"
    o31 = enc
    hdr = (b"30 0 31 %d " % (len(o30) + 1)).ljust(20)
    data = hdr + o30 + b" " + o31 + b"\n"
    return b"<< /Type /ObjStm /N 2 /First 20 /Length %d >>\nstream\n" % len(data) + data + b"endstream"
enc1 = b"<< /Type /Encoding /Differences [65 /X /Y /Z] >>"
enc2 = b"<< /Type /Encoding /Differences [65 /M /N /O] >>"
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body, rec=True):
    if rec: off[n] = len(out)
    out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
put(1, b"<< /Type /Catalog /Pages 2 0 R /Outlines 30 0 R >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>")
put(4, b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream", rec=False)
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding 31 0 R >>")
put(20, objstm(enc1))
rows = b"\x02" + struct.pack(">H", 20) + b"\x00" + b"\x02" + struct.pack(">H", 20) + b"\x01"
x21 = len(out)
out += b"21 0 obj\n<< /Type /XRef /Size 32 /Index [30 2] /W [1 2 1] /Length %d >>\nstream\n" % len(rows) + rows + b"\nendstream\nendobj\n"
put(20, objstm(enc2), rec=False)   # later, unlisted body of 20
xt = len(out)
listed = [1, 2, 3, 5, 20]
out += b"xref\n0 4\n0000000000 65535 f \n" + b"".join(b"%010d 00000 n \n" % off[n] for n in (1, 2, 3))
out += b"5 1\n%010d 00000 n \n" % off[5]
out += b"20 2\n%010d 00000 n \n%010d 00000 n \n" % (off[20], x21)
out += b"trailer\n<< /Size 32 /Root 1 0 R /XRefStm %d >>\nstartxref\n%d\n%%%%EOF\n" % (x21, xt)
open(out_path, "wb").write(out)
