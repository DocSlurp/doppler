"""Writes the page-scan fixtures of tests/page_scans.rs into the folder given as argv[1].

Built with test/docslurp/docslurp_testlib.py (stdlib only: a baseline JPEG writer and a CCITT
G4 stream), so no binary fixture is checked in:

  scans.pdf   p1 a grey DCT scan (no text), p2 the same with an invisible OCR layer, p3 a
              CCITT G4 scan, p4 a Flate RGB scan, p5 the DCT turned 90 degrees,
              p6 four DCT strips (tiled), p7 text with a small DCT picture (not a scan)
  scan.jpg    the JPEG stream embedded on pages 1 and 2
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
