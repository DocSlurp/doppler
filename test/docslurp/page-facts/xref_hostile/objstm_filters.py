# K valid object streams (each holds one page dict) in a file with an intact xref stream, behind a filter the object-stream reader
# must refuse or bound: jbig2 (page-info W x W bitmap), wsflate (Flate+ASCIIHex over W MiB of spaces), wsflateonly (Flate over W MiB of spaces).
# usage: objstm_filters.py out.pdf K W jbig2|wsflate|wsflateonly|flate
import sys, struct, zlib
out_path=sys.argv[1]; K=int(sys.argv[2]); W=int(sys.argv[3]); mode=sys.argv[4]  # jbig2 | wsflate | flate
pi=struct.pack(">IIIIBH",W,W,0,0,0,0); seg=struct.pack(">IBBBI",0,48,0,1,len(pi))+pi
if mode=="jbig2": data=seg; filt=b"/Filter /JBIG2Decode"
elif mode=="wsflate":
    co=zlib.compressobj(9); p=[co.compress(b" "*(1<<20)) for _ in range(W)]; p.append(co.compress(b"10 0 << /Type /Page >>")); p.append(co.flush()); data=b"".join(p); filt=b"/Filter [/FlateDecode /ASCIIHexDecode]"
elif mode=="wsflateonly":
    co=zlib.compressobj(9); p=[co.compress(b" "*(1<<20)) for _ in range(W)]; p.append(co.compress(b"10 0 << /Type /Page >>")); p.append(co.flush()); data=b"".join(p); filt=b"/Filter /FlateDecode"
else: data=zlib.compress(b"10 0 << /Type /Page >>"); filt=b"/Filter /FlateDecode"
out=bytearray(b"%PDF-1.5\n"); off={}
kids=b" ".join(b"%d 0 R"%(10+i) for i in range(K))
objs={1:b"<< /Type /Catalog /Pages 2 0 R >>",2:b"<< /Type /Pages /Kids [%s] /Count %d >>"%(kids,K)}
for n in objs: off[n]=len(out); out+=b"%d 0 obj\n"%n+objs[n]+b"\nendobj\n"
S=1000
for i in range(K):
    off[S+i]=len(out)
    out+=b"%d 0 obj\n<< /Type /ObjStm /N 1 /First 5 %s /Length %d >>\nstream\n"%(S+i,filt,len(data))+data+b"\nendstream\nendobj\n"
xr=len(out); size=S+K+1
rows=bytearray()
for n in range(size):
    if n in (1,2) or n>=S and n<S+K: rows+=struct.pack(">BIH",1,off[n],0)
    elif 10<=n<10+K: rows+=struct.pack(">BIH",2,S+(n-10),0)
    else: rows+=struct.pack(">BIH",0,0,0)
out+=b"%d 0 obj\n<< /Type /XRef /Size %d /W [1 4 2] /Root 1 0 R /Length %d >>\nstream\n"%(S+K,size,len(rows))+bytes(rows)+b"\nendstream\nendobj\n"
out+=b"startxref\n%d\n%%%%EOF\n"%xr
open(out_path,"wb").write(out)
