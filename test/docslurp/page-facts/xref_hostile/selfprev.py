import sys
HELV=b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
def st(c): return b"<< /Length %d >>\nstream\n" % len(c) + c + b"\nendstream"
objs={1:b"<< /Type /Catalog /Pages 2 0 R >>",2:b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
3:b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im 6 0 R >> >> >>",
4:st(b"0 0 0 RG 1 w 72 500 m 472 500 l S q 612 792 0 0 0 0 cm /Im Do Q BT /F1 12 Tf 72 700 Td (Alpha) Tj ET"),5:HELV,
6:b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceGray /Length 4 >>\nstream\n\x80\x80\x80\x80\nendstream"}
out=bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n"); off={}
for n in sorted(objs): off[n]=len(out); out+=b"%d 0 obj\n"%n+objs[n]+b"\nendobj\n"
x=len(out)
out+=b"xref\n0 7\n0000000000 65535 f \n"+b"".join(b"%010d 00000 n \n"%off[n] for n in range(1,7))
mode=sys.argv[2]
prev = b" /Prev %d"%x if mode=="self" else (b" /Prev 9999999" if mode=="eof" else b"")
out+=b"trailer\n<< /Size 7 /Root 1 0 R%s >>\nstartxref\n%d\n%%%%EOF\n"%(prev,x)
open(sys.argv[1],"wb").write(out)
