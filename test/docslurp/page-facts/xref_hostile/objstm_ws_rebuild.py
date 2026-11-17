# No xref (a rebuild): K object streams, each Flate over MB MiB of whitespace; the rebuild's header read must stop early.
import sys, zlib
out_path=sys.argv[1]; K=int(sys.argv[2]); MB=int(sys.argv[3])
co=zlib.compressobj(9); p=[co.compress(b" "*(1<<20)) for _ in range(MB)]; p.append(co.flush()); z=b"".join(p)
out=bytearray(b"%PDF-1.5\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n")
for k in range(K):
    out+=b"%d 0 obj\n<< /Type /ObjStm /N 1 /First 0 /Filter /FlateDecode /Length %d >>\nstream\n"%(10+k,len(z))+z+b"\nendstream\nendobj\n"
out+=b"trailer\n<< /Size %d /Root 1 0 R >>\n"%(10+K)
open(out_path,"wb").write(out)
