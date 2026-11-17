# K page references into one object stream that fails to parse after decoding First bytes (header of spaces): it must not be decoded again for each reference.
# usage: objstm_failing.py out.pdf K First
import sys, struct, zlib
# K page refs (type 2, idx 0) all pointing at ONE object stream that fails (header = 4MiB of spaces, no tokens)
out_path=sys.argv[1]; K=int(sys.argv[2]); first=int(sys.argv[3]); mode=sys.argv[4] if len(sys.argv)>4 else "ws"
S=10+K
kids=b" ".join(b"%d 0 R"%(10+i) for i in range(K))
out=bytearray(b"%PDF-1.5\n"); off={}
for n,b in {1:b"<< /Type /Catalog /Pages 2 0 R >>",2:b"<< /Type /Pages /Kids [%s] /Count %d >>"%(kids,K)}.items():
    off[n]=len(out); out+=b"%d 0 obj\n"%n+b+b"\nendobj\n"
data=zlib.compress(b" "*first+b"x",9) if mode=="ws" else zlib.compress(b"1 0 "+b" "*first,9)
off[S]=len(out); out+=b"%d 0 obj\n<< /Type /ObjStm /N 1 /First %d /Filter /FlateDecode /Length %d >>\nstream\n"%(S,first,len(data))+data+b"\nendstream\nendobj\n"
xr=len(out); xnum=S+1; size=xnum+1
rows=bytearray()
for n in range(size):
    if 10<=n<10+K: rows+=struct.pack(">BII",2,S,0)
    elif n in off: rows+=struct.pack(">BII",1,off[n],0)
    elif n==xnum: rows+=struct.pack(">BII",1,xr,0)
    else: rows+=struct.pack(">BII",0,0,0)
out+=b"%d 0 obj\n<< /Type /XRef /Size %d /W [1 4 4] /Root 1 0 R /Length %d >>\nstream\n"%(xnum,size,len(rows))+bytes(rows)+b"\nendstream\nendobj\n"
out+=b"startxref\n%d\n%%%%EOF\n"%xr
open(out_path,"wb").write(out)
