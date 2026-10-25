#!/usr/bin/env python3
"""1-page stdlib PDF: ruled table whose header cell 'Revenue' spans cols 2-3 (inner vertical rule omitted in header row). Usage: OUT.pdf"""
import sys
objs=[]
def add(b): objs.append(b); return len(objs)
f=add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"); fb=add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>")
def T(fo,x,y,s): return b"BT /F%d 10 Tf %g %g Td (%s) Tj ET\n"%(fo,x,y,s.encode())
c=b"0.5 w 0 G\n"; x0,y0,cw,rh=72,700,100,18
for r in range(4): c+=b"%g %g m %g %g l S\n"%(x0,y0-r*rh,x0+3*cw,y0-r*rh)
for k in (0,1,3): c+=b"%g %g m %g %g l S\n"%(x0+k*cw,y0,x0+k*cw,y0-3*rh)
c+=b"%g %g m %g %g l S\n"%(x0+2*cw,y0-rh,x0+2*cw,y0-3*rh)   # inner rule only below header row
for (fo,x,y,s) in [(2,76,687,"Region"),(2,176,687,"Revenue"),(1,76,669,"North"),(1,176,669,"10.5"),(1,276,669,"12.0"),(1,76,651,"South"),(1,176,651,"8.1"),(1,276,651,"9.7")]: c+=T(fo,x,y,s)
cont=add(b"<< /Length %d >>\nstream\n"%len(c)+c+b"\nendstream")
pid=len(objs)+2
pg=add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Contents %d 0 R /Resources << /Font << /F1 %d 0 R /F2 %d 0 R >> >> >>"%(pid,cont,f,fb))
pgs=add(b"<< /Type /Pages /Kids [%d 0 R] /Count 1 >>"%pg); assert pgs==pid
cat=add(b"<< /Type /Catalog /Pages %d 0 R >>"%pgs)
out=b"%PDF-1.5\n"; offs=[]
for i,o in enumerate(objs): offs.append(len(out)); out+=b"%d 0 obj\n"%(i+1)+o+b"\nendobj\n"
x=len(out); out+=b"xref\n0 %d\n0000000000 65535 f \n"%(len(objs)+1)+b"".join(b"%010d 00000 n \n"%o for o in offs)
out+=b"trailer\n<< /Size %d /Root %d 0 R >>\nstartxref\n%d\n%%%%EOF\n"%(len(objs)+1,cat,x)
open(sys.argv[1],"wb").write(out)
