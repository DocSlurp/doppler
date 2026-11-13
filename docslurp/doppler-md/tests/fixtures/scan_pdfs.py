"""Writes the page-scan fixtures of tests/page_scans.rs into the folder given as argv[1].

Built with test/docslurp/docslurp_testlib.py (stdlib only: a baseline JPEG writer and a CCITT
G4 stream), so no binary fixture is checked in:

  scans.pdf   p1 a grey DCT scan (no text), p2 the same with an invisible OCR layer, p3 a
              CCITT G4 scan, p4 a Flate RGB scan, p5 the DCT turned 90 degrees,
              p6 four DCT strips (tiled), p7 text with a small DCT picture (not a scan)
  scan.jpg    the JPEG stream embedded on pages 1 and 2
  redacted.pdf  scans a copy would un-redact: p1 the DCT scan under a black box, p2 a Flate
              scan under a black box, p3 the DCT scan with a white-out and visible text, p4
              the DCT scan cut by a clip; p5 the plain DCT scan (copied); p6 the DCT scan with an
              invisible OCR layer and a visible page number (rendered: the accepted cost of the
              copy allow-list)
  budget.pdf  p1 text with two block pictures, p2 and p3 the DCT scan (a figure budget of one
              must not stop the scans after it in the same batch)
  huge.pdf    a 4200 x 4200 Flate grey scan, past the copy cap (rendered instead)
  slow.pdf    a 1000 x 1000 Flate grey scan whose lossless encode at the highest effort takes
              seconds (an encode past the deadline is dropped)
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "..", "test", "docslurp"))
import docslurp_testlib as t  # noqa: E402

out = sys.argv[1]
jpeg = t.jpeg(160, 200)
strip = t.jpeg(160, 50)
g4 = t.ccitt_g4_white(800, 1000)
# an RGB page image finer than --scan-max-dpi (8.5 x 11 in at about 141 dpi; the test lowers
# the cap to 72 dpi), a gradient
flate_w, flate_h = 1200, 1553
flate = bytes(((x + y) % 256, x % 256, y % 256)[c] for y in range(flate_h) for x in range(flate_w) for c in range(3))


def img(body, data, compress=False):
    return t.stream("/Type /XObject /Subtype /Image " + body, data, compress=compress)


ocr = b"".join(b"BT 3 Tr /F1 11 Tf 72 %d Td (Scanned line %d of the page) Tj ET\n" % (700 - 20 * k, k) for k in range(5))
text = b"".join(b"BT /F1 11 Tf 72 %d Td (A paragraph line %d about the photograph below) Tj ET\n" % (720 - 14 * k, k) for k in range(6))
pages = [
    (b"q 612 0 0 792 0 0 cm /S Do Q", "S"),
    (b"q 612 0 0 792 0 0 cm /S Do Q\n" + ocr, "S"),
    (b"q 612 0 0 792 0 0 cm /C Do Q", "C"),
    (b"q 612 0 0 792 0 0 cm /R Do Q", "R"),
    (b"q 0 -792 612 0 0 792 cm /S Do Q", "S"),
    (b"".join(b"q 612 0 0 198 0 %d cm /T Do Q " % (198 * k) for k in range(4)), "T"),
    (text + b"q 200 0 0 250 72 300 cm /S Do Q", "S"),
]
n = len(pages)
# objects: 1 catalog, 2 pages, 3.. the pages, then the font, the images and the contents
page_nums = list(range(3, 3 + n))
font = 3 + n
images = {"S": font + 1, "C": font + 2, "R": font + 3, "T": font + 4}
contents = list(range(font + 5, font + 5 + n))
objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % p for p in page_nums), n),
]
for i, (_, name) in enumerate(pages):
    objects.append(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 %d 0 R >> /XObject << /%s %d 0 R >> >> /Contents %d 0 R >>"
        % (font, name, images[name], contents[i])
    )
objects.append("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
objects.append(img("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", jpeg))
objects.append(img("/Width 800 /Height 1000 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 800 /Rows 1000 >>", g4))
objects.append(img("/Width %d /Height %d /ColorSpace /DeviceRGB /BitsPerComponent 8" % (flate_w, flate_h), flate, compress=True))
objects.append(img("/Width 160 /Height 50 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", strip))
for content, _ in pages:
    objects.append(t.stream("", content))
with open(os.path.join(out, "scans.pdf"), "wb") as f:
    f.write(t.build_pdf(objects))
with open(os.path.join(out, "scan.jpg"), "wb") as f:
    f.write(jpeg)


# redacted.pdf: objects 1 catalog, 2 pages, 3..8 pages, 9 font, 9 the DCT scan, 10 a Flate
# scan, 11.. contents
small_w, small_h = 153, 198
small = bytes((x * 255 // small_w) if c == 0 else 200 for y in range(small_h) for x in range(small_w) for c in range(3))
box = b"0 g 50 300 500 200 re f\n"
red_pages = [
    (b"q 612 0 0 792 0 0 cm /S Do Q\n" + box, "S"),
    (b"q 612 0 0 792 0 0 cm /R Do Q\n" + box, "R"),
    (b"q 612 0 0 792 0 0 cm /S Do Q\n1 g 50 300 500 200 re f BT 0 g /F1 24 Tf 200 390 Td (WITHHELD) Tj ET\n", "S"),
    (b"q 0 0 612 700 re W n 612 0 0 792 0 0 cm /S Do Q\n", "S"),
    (b"q 612 0 0 792 0 0 cm /S Do Q\n", "S"),
    (b"q 612 0 0 792 0 0 cm /S Do Q\n" + ocr + b"BT 0 Tr /F1 9 Tf 300 20 Td (17) Tj ET\n", "S"),
]
m = len(red_pages)
red_font = 3 + m
red_images = {"S": red_font + 1, "R": red_font + 2}
red_contents = list(range(red_font + 3, red_font + 3 + m))
objs = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % (3 + i) for i in range(m)), m),
]
for i, (_, name) in enumerate(red_pages):
    objs.append(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 %d 0 R >> /XObject << /%s %d 0 R >> >> /Contents %d 0 R >>"
        % (red_font, name, red_images[name], red_contents[i])
    )
objs.append("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
objs.append(img("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", jpeg))
objs.append(img("/Width %d /Height %d /ColorSpace /DeviceRGB /BitsPerComponent 8" % (small_w, small_h), small, compress=True))
for content, _ in red_pages:
    objs.append(t.stream("", content))
with open(os.path.join(out, "redacted.pdf"), "wb") as f:
    f.write(t.build_pdf(objs))


def simple_doc(pages_content, xobjects, mediabox="0 0 612 792"):
    """A PDF of len(pages_content) pages sharing the font and the image objects in
    `xobjects` (name -> stream object). Objects: 1 catalog, 2 pages, pages, font, images,
    contents."""
    m = len(pages_content)
    font_n = 3 + m
    names = list(xobjects)
    img_n = {name: font_n + 1 + k for k, name in enumerate(names)}
    first_content = font_n + 1 + len(names)
    res = "<< /Font << /F1 %d 0 R >> /XObject << %s >> >>" % (font_n, " ".join("/%s %d 0 R" % (k, v) for k, v in img_n.items()))
    objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % (3 + i) for i in range(m)), m),
    ]
    for i in range(m):
        objs.append("<< /Type /Page /Parent 2 0 R /MediaBox [%s] /Resources %s /Contents %d 0 R >>" % (mediabox, res, first_content + i))
    objs.append("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    objs.extend(xobjects[k] for k in names)
    objs.extend(t.stream("", c) for c in pages_content)
    return t.build_pdf(objs)


dct = img("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", jpeg)
dct_strip = img("/Width 160 /Height 50 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", strip)
with open(os.path.join(out, "budget.pdf"), "wb") as f:
    f.write(simple_doc([text + b"q 200 0 0 250 72 300 cm /S Do Q q 320 0 0 100 72 100 cm /T Do Q", b"q 612 0 0 792 0 0 cm /S Do Q", b"q 612 0 0 792 0 0 cm /U Do Q"], {"S": dct, "T": dct_strip, "U": img("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", jpeg)}))
huge = img("/Width 4200 /Height 4200 /ColorSpace /DeviceGray /BitsPerComponent 8", bytes(4200 * 4200), compress=True)
with open(os.path.join(out, "huge.pdf"), "wb") as f:
    f.write(simple_doc([b"q 612 0 0 792 0 0 cm /H Do Q"], {"H": huge}))
noise = bytes(((x * 7 + y * 13) ^ (x * y >> 5)) & 255 for y in range(1000) for x in range(1000))
slow = img("/Width 1000 /Height 1000 /ColorSpace /DeviceGray /BitsPerComponent 8", noise, compress=True)
with open(os.path.join(out, "slow.pdf"), "wb") as f:
    f.write(simple_doc([b"q 1000 0 0 1000 0 0 cm /N Do Q"], {"N": slow}, mediabox="0 0 1000 1000"))
