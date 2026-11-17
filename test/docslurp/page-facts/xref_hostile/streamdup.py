# No xref (a rebuild): one object whose 400 KB dictionary is followed by R repeats of ">>\nstream\n". The rebuild
# listed the object once per "stream" token and parsed it in full for each. usage: streamdup.py out.pdf R
import sys

out_path, R = sys.argv[1], int(sys.argv[2])
out = bytearray(b"%PDF-1.5\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n")
out += b"5 0 obj\n<< /A [" + b"0 " * 200000 + b"] "
out += b">>\nstream\n" * R
out += b"\nendstream\nendobj\ntrailer\n<< /Size 6 /Root 1 0 R >>\n"
open(out_path, "wb").write(out)
