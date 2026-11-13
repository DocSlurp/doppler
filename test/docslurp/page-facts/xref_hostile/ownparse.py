# Two object streams whose /Length each live in the other (lenstack technique).
#   Z (50): members 43 (Y's /Length), 44 (font dict F1); Z /Length 41 0 R
#   Y (60): members 41 (Z's /Length), 92 (OCMD over an OFF group: hidden); Y /Length 43 0 R
# Y's data has a bare "endstream" between 41 and 92.
# Drawing reaches Z first (font F1): Z's /Length -> 41 -> Y, whose /Length 43 is blocked (Z on
# the stack) -> Y cut at the fake endstream -> 92 missing -> BDC /OC 92 visible (not cached).
# Image geometry (facts or -images) evaluates the image's /OC 92 first with its own parse:
# Y reached first -> full -> OCMD hidden -> cached in ocmdCache -> drawing's BDC then hidden.
import sys, struct
out_path = sys.argv[1]
content = b"q 100 0 0 100 72 400 cm /Im0 Do Q BT /F1 24 Tf 72 700 Td (Record quarter) Tj /OC /MC0 BDC 0 -40 Td (Expansion to London) Tj EMC ET"
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body):
    off[n] = len(out); out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
def objstm_data(members):
    hdr = b""; pos = 0; bodies = b""
    for num, b in members:
        hdr += b"%d %d " % (num, pos); bodies += b; pos += len(b)
    return hdr, bodies
put(1, b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [20 0 R] /D << /OFF [20 0 R] /Order [20 0 R] >> >> >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 44 0 R >> /XObject << /Im0 7 0 R >> /Properties << /MC0 92 0 R >> >> >>")
put(4, b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream")
put(7, b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /OC 92 0 R /Length 1 >>\nstream\n\x80\nendstream")
put(20, b"<< /Type /OCG /Name (Layer) >>")
def ydata(zlen):
    m41 = (b"%d\nendstream\n" % zlen).ljust(24)
    hdr, bodies = objstm_data([(41, m41), (92, b"<< /Type /OCMD /OCGs [20 0 R] /P /AllOn >> ")])
    hdr = hdr.ljust(24)
    return hdr, hdr + bodies
def zdata(ylen):
    m43 = (b"%d " % ylen).ljust(12)
    hdr, bodies = objstm_data([(43, m43), (44, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> ")])
    hdr = hdr.ljust(24)
    return hdr, hdr + bodies
yh, yd = ydata(0); zh, zd = zdata(0)
for _ in range(3):
    yh, yd = ydata(len(zd)); zh, zd = zdata(len(yd))
put(50, b"<< /Type /ObjStm /N 2 /First %d /Length 41 0 R >>\nstream\n" % len(zh) + zd + b"\nendstream")
put(60, b"<< /Type /ObjStm /N 2 /First %d /Length 43 0 R >>\nstream\n" % len(yh) + yd + b"\nendstream")
xr = len(out)
ents = {0: (0, 0, 255)}
for n in off: ents[n] = (1, off[n], 0)
ents[43] = (2, 50, 0); ents[44] = (2, 50, 1); ents[41] = (2, 60, 0); ents[92] = (2, 60, 1); ents[95] = (1, xr, 0)
size = 96
rows = b"".join(struct.pack(">BIH", *ents.get(i, (0, 0, 0))) for i in range(size))
out += b"95 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /W [1 4 2] /Length %d >>\nstream\n" % (size, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xr
open(out_path, "wb").write(out)
