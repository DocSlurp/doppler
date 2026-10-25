#!/usr/bin/env python3
"""Hand-build a 2-page synthetic PDF (stdlib only) exercising: headings, bold/italic,
link annot, ruled table, borderless table, filled shape, image, math (Symbol font + rise).
Usage: make_fixture.py OUT.pdf"""
import sys, zlib
objs = []
def add(b): objs.append(b); return len(objs)
def stream(d, data, flate=False):
    if flate: data = zlib.compress(data); d += b" /Filter /FlateDecode"
    return b"<< " + d + b" /Length %d >>\nstream\n" % len(data) + data + b"\nendstream"
# fonts 1..6
fonts = [b"Helvetica", b"Helvetica-Bold", b"Helvetica-Oblique", b"Times-Roman", b"Symbol", b"Courier"]
fobj = [add(b"<< /Type /Font /Subtype /Type1 /BaseFont /" + f + b" >>") for f in fonts]  # 1..6
# image: 8x8 RGB gradient
img = bytes([ (x*32, y*32, 128) [i] for y in range(8) for x in range(8) for i in range(3)])
imgo = add(stream(b"/Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceRGB /BitsPerComponent 8", img))
def T(font, size, x, y, s): return b"BT /F%d %g Tf %g %g Td (%s) Tj ET\n" % (font, size, x, y, s.encode('latin1'))
c = b""
c += T(2, 22, 72, 740, "1. Quarterly Results")
c += T(1, 11, 72, 715, "This paragraph has plain text, then ")
c += T(2, 11, 255, 715, "bold")
c += T(1, 11, 280, 715, " and ")
c += T(3, 11, 305, 715, "italic")
c += T(1, 11, 335, 715, " spans, plus a ")
c += T(1, 11, 400, 715, "link to example.org") 
c += T(2, 15, 72, 685, "1.1 Ruled table")
# ruled table 3 cols x 3 rows
x0, y0, cw, rh = 72, 670, 100, 18
rows = [["Region","Q1","Q2"],["North","10.5","12.0"],["South","8.1","9.7"]]
c += b"0.5 w 0 G\n"
for r in range(4): c += b"%g %g m %g %g l S\n" % (x0, y0-r*rh, x0+3*cw, y0-r*rh)
for k in range(4): c += b"%g %g m %g %g l S\n" % (x0+k*cw, y0, x0+k*cw, y0-3*rh)
for r,row in enumerate(rows):
    for k,cell in enumerate(row): c += T(2 if r==0 else 1, 10, x0+k*cw+4, y0-r*rh-13, cell)
c += T(2, 15, 72, 590, "1.2 Borderless table")
for r,row in enumerate([["Item","Qty","Price"],["Widget","4","3.50"],["Gadget","12","1.25"]]):
    for k,cell in enumerate(row): c += T(6, 10, 72+k*110, 570-r*14, cell)
c += T(2, 15, 72, 510, "1.3 Math and shapes")
c += T(4, 12, 72, 490, "E = mc") + b"BT /F4 8 Tf 111 496 Td (2) Tj ET\n"
c += b"BT /F5 12 Tf 150 490 Td (\\141 + \\142 = \\123\\050\\141\\051) Tj ET\n"   # alpha beta Sigma
c += b"0.2 0.4 0.9 rg 72 420 150 40 re f\n0 g\n"
c += T(1, 9, 80, 436, "Filled rectangle (callout)")
c += b"q 96 0 0 96 300 400 cm /Im1 Do Q\n"
c += T(1, 9, 300, 390, "Figure 1. 8x8 gradient")
c += T(1, 8, 72, 40, "Page footer 1 of 2")
p1 = add(stream(b"", c))
c2 = T(2, 22, 72, 740, "2. Second page") + T(1, 11, 72, 715, "Continuation text under a second heading.") + T(1, 8, 72, 40, "Page footer 2 of 2")
p2 = add(stream(b"", c2))
link = add(b"<< /Type /Annot /Subtype /Link /Rect [400 712 520 726] /Border [0 0 0] /A << /S /URI /URI (https://example.org/) >> >>")
pages_id = len(objs) + 3
pg1 = add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Contents %d 0 R /Annots [%d 0 R] /Resources << /Font << " % (pages_id, p1, link)
    + b" ".join(b"/F%d %d 0 R" % (i+1, fobj[i]) for i in range(6)) + b" >> /XObject << /Im1 %d 0 R >> >> >>" % imgo)
pg2 = add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Contents %d 0 R /Resources << /Font << " % (pages_id, p2)
    + b" ".join(b"/F%d %d 0 R" % (i+1, fobj[i]) for i in range(6)) + b" >> >> >>")
pages = add(b"<< /Type /Pages /Kids [%d 0 R %d 0 R] /Count 2 >>" % (pg1, pg2)); assert pages == pages_id, (pages, pages_id)
info = add(b"<< /Title (Synthetic DocSlurp fixture) /Author (radical) >>")
cat = add(b"<< /Type /Catalog /Pages %d 0 R >>" % pages)
out = b"%PDF-1.5\n"; offs = []
for i,o in enumerate(objs): offs.append(len(out)); out += b"%d 0 obj\n" % (i+1) + o + b"\nendobj\n"
xr = len(out); out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs)+1) + b"".join(b"%010d 00000 n \n" % o for o in offs)
out += b"trailer\n<< /Size %d /Root %d 0 R /Info %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs)+1, cat, info, xr)
open(sys.argv[1], "wb").write(out)
