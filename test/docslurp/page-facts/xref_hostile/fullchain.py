# A well-formed file saved K+1 times by a writer that emits a full cross-reference stream each
# time (every slot listed). S slots x K older sections. usage: fullchain.py S K out.pdf
import sys, zlib, struct
S = int(sys.argv[1]); K = int(sys.argv[2]); out_path = sys.argv[3]
content = b"0.5 w 72 700 m 472 700 l S 72 670 m 472 670 l S 72 610 m 72 700 l S 472 610 m 472 700 l S\nBT /F1 12 Tf 72 740 Td (Hello world) Tj ET"
objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
}
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
def table(xnum, xoff):
    rows = bytearray()
    for i in range(S):
        if i in off: rows += b"\x01" + struct.pack(">I", off[i]) + b"\x00"
        elif i == xnum: rows += b"\x01" + struct.pack(">I", xoff) + b"\x00"
        else: rows += b"\x00" + struct.pack(">I", 0) + (b"\xff" if i == 0 else b"\x00")
    return zlib.compress(bytes(rows), 9)
prev = None
for k in range(K + 1):
    num = S - 1 - k  # xref stream object numbers inside the table
    at = len(out)
    data = table(num, at)
    d = b"<< /Type /XRef /Size %d /W [1 4 1] /Root 1 0 R /Filter /FlateDecode /Length %d%s >>" % (S, len(data), b" /Prev %d" % prev if prev is not None else b"")
    out += b"%d 0 obj\n" % num + d + b"\nstream\n" + data + b"\nendstream\nendobj\n"
    prev = at
out += b"startxref\n%d\n%%%%EOF\n" % prev
open(out_path, "wb").write(out)
