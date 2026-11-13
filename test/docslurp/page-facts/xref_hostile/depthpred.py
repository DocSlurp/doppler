# Page 1's widget /V (A, in S_1) reaches S_n through a chain of n object streams, each one's
# /Length a member of the next; S_n is entered near the parser's recursion limit, so its /Length
# lookup fails and it is cut at a bare "endstream" (no blocked fetch, no refused repair: the
# shaped parse is kept in the Facts cache). Page 2's widget /V (KV, in S_n after the bare
# endstream) then reads that cut parse; a run of page 2 alone parses S_n at depth 1 (whole).
import sys
out_path = sys.argv[1]; n = int(sys.argv[2])
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(num, body):
    off[num] = len(out); out.extend(b"%d 0 obj\n" % num + body + b"\nendobj\n")
c = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
put(1, b"<< /Type /Catalog /Pages 2 0 R >>")
put(2, b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> /Annots [8 0 R] >>")
put(4, b"<< /Length %d >>\nstream\n" % len(c) + c + b"\nendstream")
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
put(6, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> /Annots [9 0 R] >>")
# object numbers: S_k = 1000+k (stream), A = 3000, L_k (S_k's length, member of S_{k+1}) = 5000+k, KV = 4000
A, KV = 3000, 4000
put(8, b"<< /Type /Annot /Subtype /Widget /FT /Tx /T (a) /Rect [72 500 200 520] /V %d 0 R >>" % A)
put(9, b"<< /Type /Annot /Subtype /Widget /FT /Tx /T (b) /Rect [72 500 200 520] /V %d 0 R >>" % KV)
members = {}  # k -> list of (num, bytes)
members[1] = [(A, b"(Expansion) ")]
for k in range(1, n):
    members.setdefault(k + 1, []).append((5000 + k, None))  # S_k's length lives in S_{k+1}
members[n].append((KV, b"(Beat London) "))
import zlib
data = {}
def build(k, lens):
    hdr = b""; bodies = b""
    ms = members[k]
    # K = S_n: its predecessor's length first, then a bare endstream, then KV
    for num, b in ms:
        if b is None:
            b = (b"%d " % lens.get(num - 5000, 0)).ljust(12)
            if k == n:
                b = (b"%d" % lens.get(num - 5000, 0)).ljust(11) + b"/"
        hdr += b"%d %d " % (num, len(bodies)); bodies += b
    hdr = hdr.ljust(64)
    if k == n:
        payload = hdr + bodies
        return hdr, zlib.compress(b"\x00" + payload, 9), len(payload)
    return hdr, hdr + bodies
lens = {}
for _ in range(3):
    for k in range(1, n + 1):
        data[k] = build(k, lens)
    lens = {k: len(data[k][1]) for k in data}
for k in range(1, n + 1):
    if k == n:
        hdr, d, cols = data[k]
        put(1000 + k, b"<< /Type /ObjStm /N %d /First %d /Filter [/FlateDecode] /DecodeParms [<< /Predictor 12 /Columns %d >>] /Length %d >>\nstream\n" % (len(members[k]), len(hdr), cols, len(d)) + d + b"\nendstream")
        continue
    hdr, d = data[k]
    put(1000 + k, b"<< /Type /ObjStm /N %d /First %d /Length %d 0 R >>\nstream\n" % (len(members[k]), len(hdr), 5000 + k) + d + b"\nendstream")
import struct
ents = {0: (0, 0, 255)}
for num in off: ents[num] = (1, off[num], 0)
for k, ms in members.items():
    for i, (num, _) in enumerate(ms): ents[num] = (2, 1000 + k, i)
size = 6000
xr = len(out); ents[5999] = (1, xr, 0)
rows = b"".join(struct.pack(">BIH", *ents.get(i, (0, 0, 0))) for i in range(size))
out += b"5999 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /W [1 4 2] /Length %d >>\nstream\n" % (size, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xr
open(out_path, "wb").write(out)
