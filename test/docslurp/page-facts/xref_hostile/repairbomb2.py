# N image draws, each /OC a direct OCMD /VE 90 0 R; 90 lives in Flate object stream 60 (with a
# STR-byte member 91). Stream 60's /DecodeParms is 80 0 R, an entry inside /Size that no xref
# section lists: a guarded fetch refuses the rebuild (suppressedRepairs++), so the guarded parse
# of 60 is never kept and every guarded fetch re-decodes it.
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
m90 = b"[/Or 20 0 R] "; m91 = b"<" + b"0" * STR + b">"
hdr = (b"90 0 91 %d " % len(m90)).ljust(24)
yd = zlib.compress(hdr + m90 + m91, 9)
put(60, b"<< /Type /ObjStm /N 2 /First %d /Filter /FlateDecode /DecodeParms 80 0 R /Length %d >>\nstream\n" % (len(hdr), len(yd)) + yd + b"\nendstream")
xs = len(out)
rows = struct.pack(">BIH", 2, 60, 0) + struct.pack(">BIH", 2, 60, 1)
put(94, b"<< /Type /XRef /Size 96 /Index [90 2] /W [1 4 2] /Length %d >>\nstream\n" % len(rows) + rows + b"\nendstream")
xr = len(out)
def ent(i):
    if i == 0: return b"0000000000 65535 f \n"
    if i in off: return b"%010d 00000 n \n" % off[i]
    return b"0000000000 00000 f \n"
t = b"xref\n0 80\n" + b"".join(ent(i) for i in range(80)) + b"81 9\n" + b"".join(ent(i) for i in range(81, 90)) + b"92 4\n" + b"".join(ent(i) for i in range(92, 96))
out += t + b"trailer\n<< /Size 96 /Root 1 0 R /XRefStm %d >>\nstartxref\n%d\n%%%%EOF\n" % (off[94], xr)
open(out_path, "wb").write(out)
