# xref-stream file. Object stream Y (60) holds the outline root (40) and the integer /Length (41)
# of object stream X (50). X holds the font's /Encoding (31). Y's own dictionary carries an
# array nested D deep before /N. A fetch of a member of Y parses Y's dictionary at a parser
# recursion depth that depends on how Y was reached: directly (doc line's /Outlines read) or
# through X's /Length (drawing). args: out D
import sys, struct
out_path, D = sys.argv[1], int(sys.argv[2])
content = b"BT /F1 24 Tf 72 700 Td (ABCABC) Tj ET"
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body):
    off[n] = len(out); out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
def objstm(members, extra=b"", length=None):
    bodies = [b for _, b in members]
    hdr = b""; pos = 0
    for (num, b) in members:
        hdr += b"%d %d " % (num, pos); pos += len(b) + 1
    hdr = hdr.ljust(len(hdr) + 1)
    data = hdr + b" ".join(bodies) + b"\n"
    L = b"%d" % len(data) if length is None else length
    return b"<< /Type /ObjStm %s /N %d /First %d /Length %s >>\nstream\n" % (extra, len(members), len(hdr), L) + data + b"endstream", len(data)
put(1, b"<< /Type /Catalog /Pages 2 0 R /Outlines 40 0 R >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>")
put(4, b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream")
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding 31 0 R >>")
xbody, xlen = objstm([(31, b"<< /Type /Encoding /Junk endstream /Differences [65 /X /Y /Z] >>")], length=b"41 0 R")
put(50, xbody)
junk = b"/Junk " + b"<< /A " * D + b"1 /B 2 " + b">> " * D
ybody, _ = objstm([(40, b"<< /Type /Outlines /Count 0 >>"), (41, b"%d" % xlen)], extra=junk)
put(60, ybody)
xr = len(out)
ents = {0: (0, 0, 255)}
for n in (1, 2, 3, 4, 5, 50, 60): ents[n] = (1, off[n], 0)
ents[31] = (2, 50, 0); ents[40] = (2, 60, 0); ents[41] = (2, 60, 1); ents[70] = (1, xr, 0)
size = 71
rows = b"".join(struct.pack(">BIH", *ents.get(i, (0, 0, 0))) for i in range(size))
out += b"70 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /W [1 4 2] /Length %d >>\nstream\n" % (size, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xr
open(out_path, "wb").write(out)
