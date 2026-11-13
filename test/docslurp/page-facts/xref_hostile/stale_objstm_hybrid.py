import sys, struct
HELV=b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
COUR=b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>"
def st(c, d=b""): return b"<< %s/Length %d >>\nstream\n" % (d, len(c)) + c + b"\nendstream"
def objstm(pairs):
    body=b""; hdr=b""
    for n,o in pairs:
        hdr+=b"%d %d " % (n, len(body)); body+=o+b"\n"
    return st(hdr+body, b"/Type /ObjStm /N %d /First %d " % (len(pairs), len(hdr)))
X=20; Y=21; N=30
objs={}
objs[1]=b"<< /Type /Catalog /Pages 2 0 R >>"
objs[2]=b"<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>"
objs[3]=b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 9 0 R >> /XObject << /Im 10 0 R >> >> >>"
objs[4]=b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 40 0 R /Resources << /Font << /F1 9 0 R >> >> >>"
objs[5]=b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 8 0 R /Resources %d 0 R >>" % Y
objs[6]=st(b"q 100 0 0 100 72 400 cm /Im Do Q BT /F1 12 Tf 72 700 Td (PageOne) Tj ET")
objs[8]=st(b"BT /F1 12 Tf 72 700 Td (PageThreeWidth) Tj ET")
objs[9]=HELV
objs[10]=st(b"\x80"*12, b"/Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace %d 0 R " % X)
objs[11]=COUR
objs[N]=objstm([(X,b"/DeviceRGB"),(Y,b"<< /Font << /F1 9 0 R >> >>")])
objs[40]=st(b"BT /F1 12 Tf 72 700 Td (PageTwo) Tj ET")
out=bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"); off={}
for k in sorted(objs): off[k]=len(out); out+=b"%d 0 obj\n"%k+objs[k]+b"\nendobj\n"
# second, unlisted body of the object stream: Y names Courier
out+=b"%d 0 obj\n"%N+objstm([(X,b"/DeviceRGB"),(Y,b"<< /Font << /F1 11 0 R >> >>")])+b"\nendobj\n"
# xref stream for compressed X, Y (hybrid)
rows=b"".join(b"\x02"+struct.pack(">I",N)+struct.pack(">H",i) for i in range(2))
xs=len(out)
out+=b"50 0 obj\n<< /Type /XRef /Size 51 /Index [%d 2] /W [1 4 2] /Length %d >>\nstream\n"%(X,len(rows))+rows+b"\nendstream\nendobj\n"
x=len(out)
listed=[k for k in sorted(objs) if k!=40]
out+=b"xref\n0 1\n0000000000 65535 f \n"+b"".join(b"%d 1\n%010d 00000 n \n"%(k,off[k]) for k in listed)
out+=b"trailer\n<< /Size 51 /Root 1 0 R /XRefStm %d >>\nstartxref\n%d\n%%%%EOF\n"%(xs,x)
open(sys.argv[1],"wb").write(out)
