# No xref at all (a rebuild): K /Type /ObjStm streams behind /Filter /JBIG2Decode (page-info W x H), whose headers the rebuild reads.
# usage: objstm_jbig2.py out.pdf K W H
import sys, struct
out_path=sys.argv[1]; K=int(sys.argv[2]); W=int(sys.argv[3]); H=int(sys.argv[4]); extra=sys.argv[5].encode() if len(sys.argv)>5 else b"/Filter /JBIG2Decode"
pi=struct.pack(">IIIIBH",W,H,0,0,0,0); seg=struct.pack(">IBBBI",0,48,0,1,len(pi))+pi
out=bytearray(b"%PDF-1.5\n")
out+=b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n"
for k in range(K):
    out+=b"%d 0 obj\n<< /Type /ObjStm /N 1 /First 0 %s /Length %d >>\nstream\n"%(10+k,extra,len(seg))+seg+b"\nendstream\nendobj\n"
out+=b"trailer\n<< /Size %d /Root 1 0 R >>\n"%(10+K)
open(out_path,"wb").write(out)
