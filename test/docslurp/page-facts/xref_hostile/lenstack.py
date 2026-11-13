# xref-stream file. Two object streams whose /Length each live in the other:
#   Z (50): members 40 (outline root), 43 (Y's /Length); Z /Length 41 0 R
#   Y (60): members 41 (Z's /Length), 42 (/Info /Title); Y /Length 43 0 R
# Y's data contains a bare "endstream" token between member 41 and member 42.
# Whichever of Y/Z is reached first resolves the other's /Length through refsBeingFetched-blocked
# recursion, so its own /Length falls back to a lexical endstream scan.
#   header path (Info /Title -> Y first): Y keeps its true /Length (43 resolved via Z); Title present
#   doc line path (/Outlines -> Z -> Y): Y's /Length 43 is blocked (Z on the stack) -> Y cut at the
#   fake endstream -> Title missing; cached in objStrsGuarded and then used by the XHTML header.
import sys, struct
out_path = sys.argv[1]
content = b"BT /F1 24 Tf 72 700 Td (Hello) Tj ET"
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body):
    off[n] = len(out); out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
def objstm_data(members):
    hdr = b""; pos = 0; bodies = b""
    for num, b in members:
        hdr += b"%d %d " % (num, pos); bodies += b; pos += len(b)
    return hdr, bodies
put(1, b"<< /Type /Catalog /Pages 2 0 R /Outlines 40 0 R >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>")
put(4, b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream")
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
put(20, b"<< /Title 42 0 R /Producer 42 0 R >>")
# Y data: member 41 = Z's length (filled later), then junk "endstream" then member 42
def ydata(zlen):
    m41 = b"%d\nendstream\n" % zlen
    m41 = m41.ljust(24)
    hdr, bodies = objstm_data([(41, m41), (42, b"(Quarterly beat London) ")])
    hdr = hdr.ljust(24)
    return hdr, hdr + bodies
def zdata(ylen):
    m40 = b"<< /Type /Outlines /Count 0 >> "
    m43 = (b"%d " % ylen).ljust(12)
    hdr, bodies = objstm_data([(40, m40), (43, m43)])
    hdr = hdr.ljust(24)
    return hdr, hdr + bodies
# fixed-width numbers so lengths are stable
yh, yd = ydata(0)
zh, zd = zdata(0)
for _ in range(3):
    yh, yd = ydata(len(zd))
    zh, zd = zdata(len(yd))
put(50, b"<< /Type /ObjStm /N 2 /First %d /Length 41 0 R >>\nstream\n" % len(zh) + zd + b"\nendstream")
put(60, b"<< /Type /ObjStm /N 2 /First %d /Length 43 0 R >>\nstream\n" % len(yh) + yd + b"\nendstream")
xr = len(out)
ents = {0: (0, 0, 255)}
for n in (1, 2, 3, 4, 5, 20, 50, 60): ents[n] = (1, off[n], 0)
ents[40] = (2, 50, 0); ents[43] = (2, 50, 1); ents[41] = (2, 60, 0); ents[42] = (2, 60, 1); ents[70] = (1, xr, 0)
size = 71
rows = b"".join(struct.pack(">BIH", *ents.get(i, (0, 0, 0))) for i in range(size))
out += b"70 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /Info 20 0 R /W [1 4 2] /Length %d >>\nstream\n" % (size, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xr
open(out_path, "wb").write(out)
