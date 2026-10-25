"""Small, original PDFs probing extraction contracts; stdlib only."""
from pathlib import Path

ROOT = Path(__file__).resolve().parent

def pdf(name, content, resources=b"", extra=()):
    stream = lambda data: b"<< /Length %d >>\nstream\n" % len(data) + data + b"\nendstream"
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>",
               b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
               b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /F1 5 0 R >> " + resources + b" >> /Contents 4 0 R >>",
               stream(content), b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>", *extra]
    out = bytearray(b"%PDF-1.7\n")
    offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(out))
        out.extend(b"%d 0 obj\n" % i + obj + b"\nendobj\n")
    xref = len(out)
    out.extend(b"xref\n0 %d\n0000000000 65535 f \n" % len(offsets))
    for offset in offsets[1:]:
        out.extend(b"%010d 00000 n \n" % offset)
    out.extend(b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(offsets), xref))
    (ROOT / name).write_bytes(out)

pdf("named-properties.pdf", b"/P /Props BDC BT /F1 20 Tf 40 340 Td (Named property: MCID 7) Tj ET EMC\n/P << /MCID 8 >> BDC BT /F1 20 Tf 40 290 Td (Direct property: MCID 8) Tj ET EMC",
    b"/Properties << /Props << /MCID 7 >> >>")
pdf("nested-marked.pdf", b"/P << /MCID 7 >> BDC BT /F1 18 Tf 40 340 Td (Before ) Tj ET /Span BMC BT /F1 18 Tf 105 340 Td (inside ) Tj ET EMC BT /F1 18 Tf 161 340 Td (after) Tj ET EMC")
pattern = b"0.2 0.4 0.8 rg 0 0 1 1 re f"
pattern_obj = b"<< /Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 2 2] /XStep 2 /YStep 2 /Resources << >> /Length %d >>\nstream\n" % len(pattern) + pattern + b"\nendstream"
pdf("pattern-grid.pdf", b"BT /F1 18 Tf 20 375 Td (Tiny pattern, many callbacks) Tj ET /Pattern cs /P1 scn 20 20 360 340 re f", b"/Pattern << /P1 6 0 R >>", (pattern_obj,))
print("Created three fixtures in", ROOT)
