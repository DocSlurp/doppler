#!/usr/bin/env python3
"""3-page transform fixture (stdlib): p1 Tm-rotated (30deg, 90deg) + cm-scaled/flipped text and a rotated rule;
p2 page /Rotate 90 with ordinary text + rule; p3 vertical-ish: negative-scale Tm (mirrored). Usage: OUT.pdf"""
import sys, math
objs=[]
def add(b): objs.append(b); return len(objs)
f=add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
def st(d): return b"<< /Length %d >>\nstream\n"%len(d)+d+b"\nendstream"
c1=b"BT /F1 12 Tf 1 0 0 1 100 700 Tm (horizontal) Tj ET\n"
a=math.radians(30); c1+=b"BT /F1 12 Tf %f %f %f %f 100 600 Tm (thirty degrees) Tj ET\n"%(math.cos(a),math.sin(a),-math.sin(a),math.cos(a))
c1+=b"BT /F1 12 Tf 0 1 -1 0 300 400 Tm (ninety up) Tj ET\n"
c1+=b"q 2 0 0 -1 50 300 cm BT /F1 10 Tf 1 0 0 1 10 10 Tm (cm scaled and flipped) Tj ET Q\n"
c1+=b"q 0.866 0.5 -0.5 0.866 300 600 cm 0.5 w 0 0 m 100 0 l S Q\n"
p1c=add(st(c1)); p2c=add(st(b"BT /F1 12 Tf 1 0 0 1 100 700 Tm (page rotate 90) Tj ET\n0.5 w 100 690 m 300 690 l S\n"))
p3c=add(st(b"BT /F1 12 Tf -1 0 0 1 300 700 Tm (mirrored text) Tj ET\n"))
pid=len(objs)+4
res=b"/Resources << /Font << /F1 %d 0 R >> >>"%f
p1=add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Contents %d 0 R %s >>"%(pid,p1c,res))
p2=add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Rotate 90 /Contents %d 0 R %s >>"%(pid,p2c,res))
p3=add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Contents %d 0 R %s >>"%(pid,p3c,res))
pg=add(b"<< /Type /Pages /Kids [%d 0 R %d 0 R %d 0 R] /Count 3 >>"%(p1,p2,p3)); assert pg==pid
cat=add(b"<< /Type /Catalog /Pages %d 0 R >>"%pg)
out=b"%PDF-1.5\n"; offs=[]
for i,o in enumerate(objs): offs.append(len(out)); out+=b"%d 0 obj\n"%(i+1)+o+b"\nendobj\n"
x=len(out); out+=b"xref\n0 %d\n0000000000 65535 f \n"%(len(objs)+1)+b"".join(b"%010d 00000 n \n"%o for o in offs)
out+=b"trailer\n<< /Size %d /Root %d 0 R >>\nstartxref\n%d\n%%%%EOF\n"%(len(objs)+1,cat,x)
open(sys.argv[1],"wb").write(out)
