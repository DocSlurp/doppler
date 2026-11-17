import sys, struct, zlib
# K ObjStms each with N zero-length members; page k lives in ObjStm k idx 0.  usage: out K N [mode]
out_path=sys.argv[1]; K=int(sys.argv[2]); N=int(sys.argv[3]); mode=sys.argv[4] if len(sys.argv)>4 else "members"
S=1000
out=bytearray(b"%PDF-1.5\n"); off={}
kids=b" ".join(b"%d 0 R"%(10+i) for i in range(K))
objs={1:b"<< /Type /Catalog /Pages 2 0 R >>",2:b"<< /Type /Pages /Kids [%s] /Count %d >>"%(kids,K)}
for n in objs: off[n]=len(out); out+=b"%d 0 obj\n"%n+objs[n]+b"\nendobj\n"
for i in range(K):
    num=10+i
    if mode=="members":
        hdr=(b"%d 0 "%num)+b"5 0 "*(N-1)
        body=b""
    elif mode=="ws":   # whitespace body 16MiB
        hdr=b"%d 0 "%num; body=b" "*(16<<20)
    first=len(hdr)
    data=zlib.compress(hdr+body,9)
    off[S+i]=len(out)
    out+=b"%d 0 obj\n<< /Type /ObjStm /N %d /First %d /Filter /FlateDecode /Length %d >>\nstream\n"%(S+i,N if mode=="members" else 1,first,len(data))+data+b"\nendstream\nendobj\n"
xr=len(out); size=S+K+1
rows=bytearray()
for n in range(size):
    if n in (1,2) or S<=n<S+K: rows+=struct.pack(">BIH",1,off[n],0)
    elif 10<=n<10+K: rows+=struct.pack(">BIH",2,S+(n-10),0)
    else: rows+=struct.pack(">BIH",0,0,0)
out+=b"%d 0 obj\n<< /Type /XRef /Size %d /W [1 4 2] /Root 1 0 R /Length %d >>\nstream\n"%(S+K,size,len(rows))+bytes(rows)+b"\nendstream\nendobj\n"
out+=b"startxref\n%d\n%%%%EOF\n"%xr
open(out_path,"wb").write(out)
