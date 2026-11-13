"""The page-scan copy allow-list (poppler/ScanCopyAudit.h).

A scan's own pixels may be copied only when the one thing that paints on its page is that
image, drawn plainly. pdftotext -images says copyable="1" on the scan image only then, and
pdfregions checks the page again before a copy job (err:copy otherwise), so both must agree.
Every way of hiding scan pixels found by earlier reviews must make the page not copyable;
a plain scan with an invisible OCR layer must still copy byte for byte.
"""

import os
import re
import subprocess
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()
JPEG = t.jpeg(160, 200)
G4 = t.ccitt_g4_white(64, 40)


def image(dict_body, data, compress=False):
    return t.stream("/Type /XObject /Subtype /Image " + dict_body, data, compress=compress)


def icc_gray_white():
    """An ICC v2 grey profile whose curve maps every sample to white."""
    import struct

    desc = b"desc" + b"\0" * 4 + struct.pack(">I", 2) + b"x\0" + b"\0" * 8 + b"\0" * 3 + b"\0" * 67
    wtpt = b"XYZ " + b"\0" * 4 + struct.pack(">iii", int(0.9642 * 65536), 65536, int(0.8249 * 65536))
    curv = b"curv" + b"\0" * 4 + struct.pack(">I", 2) + struct.pack(">HH", 0xFFFF, 0xFFFF)
    tags = [(b"desc", desc), (b"wtpt", wtpt), (b"kTRC", curv)]
    off = 128 + 4 + 12 * len(tags)
    table, body = struct.pack(">I", len(tags)), b""
    for sig, data in tags:
        while (off + len(body)) % 4:
            body += b"\0"
        table += sig + struct.pack(">II", off + len(body), len(data))
        body += data
    size = 128 + len(table) + len(body)
    head = struct.pack(">I", size) + b"lcms" + bytes([2, 0x10, 0, 0]) + b"mntrGRAYXYZ " + b"\0" * 12 + b"acspAPPL" + b"\0" * 24 + struct.pack(">iii", int(0.9642 * 65536), 65536, int(0.8249 * 65536)) + b"\0" * 48
    return head + table + body


# objects 6.. (simple_page_pdf: 1 catalog, 2 pages, 3 page, 4 Helvetica, 5 content)
OBJECTS = [
    image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", JPEG),  # 6: the scan
    image("/Width 3 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8", b"\xff" * 6, compress=True),  # 7: a white patch
    # 8: the scan inside a transparency group
    t.stream("/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Group << /S /Transparency >> /Resources << /XObject << /S 6 0 R >> >>", "q 612 0 0 792 0 0 cm /S Do Q"),
    # 9, 10: a Type 3 font whose glyph paints far from its origin
    "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [1 0 0 1 0 0] /CharProcs << /a 10 0 R >> /Encoding << /Differences [97 /a] >> /FirstChar 97 /LastChar 97 /Widths [1000] /Resources << >> >>",
    t.stream("", "1000 0 d0 -5300 0 5000 792 re f"),
    # 11: the scan inside a plain form (no group)
    t.stream("/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Resources << /XObject << /S 6 0 R >> >>", "q 612 0 0 792 0 0 cm /S Do Q"),
    # 12: a form that paints a box over the scan
    t.stream("/Type /XObject /Subtype /Form /BBox [0 0 612 792]", "0 g 50 300 500 200 re f"),
    # 13: the scan with an ICC profile that maps it to white; 14 the profile
    image("/Width 160 /Height 200 /ColorSpace [/ICCBased 14 0 R] /BitsPerComponent 8 /Filter /DCTDecode", JPEG),
    t.stream("/N 1 /Alternate /DeviceGray", icc_gray_white()),
    # 15: CalGray
    image("/Width 160 /Height 200 /ColorSpace [/CalGray << /WhitePoint [0.9505 1 1.089] /Gamma 2.2 >>] /BitsPerComponent 8 /Filter /DCTDecode", JPEG),
    # 16: an annotation appearance that paints a box
    t.stream("/Type /XObject /Subtype /Form /BBox [0 0 500 200]", "0 g 0 0 500 200 re f"),
    # 17: an optional content group (hidden by the catalog in some cases)
    "<< /Type /OCG /Name (Scan) >>",
    # 18: the scan with its own /OC
    image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode /OC 17 0 R", JPEG),
    # 19: a bilevel stencil mask scan
    image("/Width 64 /Height 40 /ImageMask true /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 64 /Rows 40 >>", G4),
    # 20: an empty annotation appearance
    t.stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", ""),
    # 21..27: images Gfx refuses to draw (render blank), whose copy would show the whole scan,
    # and 28 the control that Gfx draws
    image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode /ImageMask 0", JPEG),  # 21
    image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode /IM /false", JPEG),  # 22
    image("/Width 64 /Height 40 /ColorSpace /DeviceGray /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 64 /Rows 40 >>", G4),  # 23
    image("/Width 64 /Height 40 /ColorSpace /DeviceGray /BitsPerComponent 1.0 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 64 /Rows 40 >>", G4),  # 24
    image("/Width 64 /Height 40 /ColorSpace /DeviceGray /BitsPerComponent 0 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 64 /Rows 40 >>", G4),  # 25
    image("/Width 64 /Height 40 /ColorSpace /DeviceGray /BitsPerComponent 64 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 64 /Rows 40 >>", G4),  # 26
    image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8.0 /Filter /DCTDecode", JPEG),  # 27
    image("/Width 64 /Height 40 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 64 /Rows 40 >>", G4),  # 28
    # 29, 30: a tall and a wide strip, 20000 samples along one axis (25 or 33 to a point on
    # a full page): a 1 pt rounding slack would hide 25 rows or columns
    image("/Width 10 /Height 20000 /ColorSpace /DeviceGray /BitsPerComponent 8", b"\xff" * 200000, compress=True),  # 29
    image("/Width 20000 /Height 10 /ColorSpace /DeviceGray /BitsPerComponent 8", b"\xff" * 200000, compress=True),  # 30
]

SHADING = "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 612 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >>"
RES = (
    "<< /Font << /F1 4 0 R /T3 9 0 R >> /XObject << /S 6 0 R /P 7 0 R /G 8 0 R /F 11 0 R /Box 12 0 R /I 13 0 R /C 15 0 R /O 18 0 R /M 19 0 R"
    " /R1 21 0 R /R2 22 0 R /R3 23 0 R /R4 24 0 R /R5 25 0 R /R6 26 0 R /R7 27 0 R /R8 28 0 R /Tall 29 0 R /Wide 30 0 R >>"
    " /Shading << /Sh %s >> /Properties << /oc1 17 0 R >>"
    " /ExtGState << /Half << /ca 0.5 >> /Zero << /ca 0 >> /Mul << /BM /Multiply >> /Mask << /SMask << /S /Luminosity /G 12 0 R >> >>"
    " /Lines << /Type /ExtGState /LW 3 /LC 1 /LJ 1 /ML 4 /D [[] 0] /RI /Perceptual /FL 1 /SA true /CA 1 /ca 1 /BM /Normal /SMask /None /OP false /op false /OPM 1 >>"
    " /Over << /OP true /op true >> /Tr << /TR /Identity >> /Unknown << /XYZ 1 >> >> >>" % SHADING
)
SCAN = b"q 612 0 0 792 0 0 cm /S Do Q\n"
OCR = b"BT 3 Tr /F1 11 Tf 72 700 Td (Scanned words of the page) Tj ET\n"
HIDDEN = "/OCProperties << /OCGs [17 0 R] /D << /OFF [17 0 R] >> >>"
SHOWN = "/OCProperties << /OCGs [17 0 R] /D << /ON [17 0 R] >> >>"

# name: (content, page_extra, catalog_extra, the scan object, copyable)
CASES = {
    # allowed
    "plain": (SCAN, "", "", 6, True),
    "ocr-layer": (SCAN + OCR, "", "", 6, True),
    "ocr-before": (OCR + SCAN, "", "", 6, True),
    "ocr-mode7-after": (SCAN + OCR.replace(b"3 Tr", b"7 Tr"), "", "", 6, True),
    "page-clip": (b"q 0 0 612 792 re W n " + SCAN + b"Q\n", "", "", 6, True),
    "form": (b"/F Do\n" + OCR, "", "", 6, True),
    "harmless-gs": (b"q /Lines gs " + SCAN + b"Q\n", "", "", 6, True),
    "marked-content": (b"/Artifact BMC /Span << /MCID 0 >> BDC " + SCAN + b"EMC EMC\n", "", "", 6, True),
    "shown-layer": (b"/OC /oc1 BDC " + SCAN + b"EMC\n", "", SHOWN, 6, True),
    "colour-and-path": (b"1 0 0 rg 0 0 1 RG 2 w 10 10 m 20 20 l n " + SCAN, "", "", 6, True),
    "empty-link": (SCAN, "/Annots [<< /Type /Annot /Subtype /Link /Rect [50 300 550 500] /Border [0 0 0] >>]", "", 6, True),
    "stencil-black": (b"0 g q 612 0 0 792 0 0 cm /M Do Q\n", "", "", 19, True),
    # denied: paint over, under or around the scan
    "redaction-box": (SCAN + b"0 g 50 300 500 200 re f\n", "", "", 6, False),
    "box-underneath": (b"q 1 g 0 0 612 792 re f Q\n" + SCAN, "", "", 6, False),
    "white-out-image": (SCAN + b"q 500 0 0 200 50 300 cm /P Do Q\n", "", "", 6, False),
    "inline-image": (SCAN + b"q 500 0 0 200 50 300 cm BI /W 1 /H 1 /CS /G /BPC 8 ID \xff EI Q\n", "", "", 6, False),
    "visible-text": (SCAN + b"BT /F1 11 Tf 72 700 Td (Bates 000123) Tj ET\n", "", "", 6, False),
    "page-number": (SCAN + OCR + b"BT 0 Tr /F1 9 Tf 300 20 Td (17) Tj ET\n", "", "", 6, False),
    "invisible-white-text-mode0": (SCAN + b"BT 1 g /F1 11 Tf 72 700 Td (white) Tj ET\n", "", "", 6, False),
    "stroke-text": (SCAN + b"BT 1 Tr /F1 11 Tf 72 700 Td (x) Tj ET\n", "", "", 6, False),
    "mode7-before": (b"q BT 7 Tr /F1 400 Tf 0 100 Td (W) Tj ET " + SCAN + b"Q\n", "", "", 6, False),
    "shading": (SCAN + b"q 50 300 500 200 re W n /Sh sh Q\n", "", "", 6, False),
    "stroke": (SCAN + b"0 G 4 w 100 100 m 300 300 l S\n", "", "", 6, False),
    "anisotropic-stroke": (SCAN + b"q 1 J 0 j 1 M 10000 0 0 0.0001 0 0 cm 0 G 1 w 0.53 0 m 0.53 7920000 l S Q\n", "", "", 6, False),
    "form-paint": (SCAN + b"/Box Do\n", "", "", 6, False),
    "type3-glyph": (SCAN + b"BT /T3 1 Tf 5300 0 Td (a) Tj ET\n", "", "", 6, False),
    "type3-invisible": (SCAN + b"BT 3 Tr /T3 1 Tf 5300 0 Td (a) Tj ET\n", "", "", 6, False),
    "twice": (SCAN + SCAN, "", "", 6, False),
    # denied: clips
    "clipped": (b"q 0 92 612 700 re W n " + SCAN + b"Q\n", "", "", 6, False),
    "odd-clip": (b"q 0 0 612 792 re 100 100 50 50 re W* n " + SCAN + b"Q\n", "", "", 6, False),
    "triangle-clip": (b"q 0 0 m 612 0 l 612 792 l 0 792 l 306 400 l h W n " + SCAN + b"Q\n", "", "", 6, False),
    "crop-box": (SCAN, "/CropBox [0 0 600 780]", "", 6, False),
    "off-page": (b"q 1224 0 0 792 -612 0 cm /S Do Q\n", "", "", 6, False),
    "turned": (b"q 0 -792 612 0 0 792 cm /S Do Q\n", "", "", 6, False),
    # denied: how it is composited
    "group": (b"/G Do\n", "", "", 6, False),
    "group-ca0": (b"q /Zero gs /G Do Q\n", "", "", 6, False),
    "opacity": (b"q /Half gs " + SCAN + b"Q\n", "", "", 6, False),
    "blend": (b"q /Mul gs " + SCAN + b"Q\n", "", "", 6, False),
    "soft-mask": (b"q /Mask gs " + SCAN + b"Q\n", "", "", 6, False),
    "overprint": (b"q /Over gs " + SCAN + b"Q\n", "", "", 6, False),
    "transfer": (b"q /Tr gs " + SCAN + b"Q\n", "", "", 6, False),
    "unknown-gs-key": (b"q /Unknown gs " + SCAN + b"Q\n", "", "", 6, False),
    "page-group": (SCAN, "/Group << /S /Transparency /CS /DeviceRGB >>", "", 6, False),
    "stencil-white": (b"1 g q 612 0 0 792 0 0 cm /M Do Q\n", "", "", 19, False),
    # denied: colour
    "icc": (b"q 612 0 0 792 0 0 cm /I Do Q\n", "", "", 13, False),
    "calgray": (b"q 612 0 0 792 0 0 cm /C Do Q\n", "", "", 15, False),
    "default-gray": (SCAN, "", "", 6, False),
    # denied: annotations and optional content
    "annotation": (SCAN, "/Annots [<< /Type /Annot /Subtype /Square /Rect [50 300 550 500] /AP << /N 16 0 R >> >>]", "", 6, False),
    "empty-annotation": (SCAN, "/Annots [<< /Type /Annot /Subtype /Square /Rect [0 0 10 10] /AP << /N 20 0 R >> >>]", "", 6, False),
    "link-border": (SCAN, "/Annots [<< /Type /Annot /Subtype /Link /Rect [50 300 550 500] /Border [0 0 2] /C [0 0 0] >>]", "", 6, False),
    # an annotation drawn only when printing (Print with NoView), or with no flags, still puts
    # something on the page; a Hidden one never does; a link or popup without an appearance
    # or border paints nothing, but other types build an appearance when they have none
    "print-only-annotation": (SCAN, "/Annots [<< /Type /Annot /Subtype /Square /F 36 /Rect [50 300 550 500] /Border [0 0 0] /AP << /N 16 0 R >> >>]", "", 6, False),
    "noview-annotation": (SCAN, "/Annots [<< /Type /Annot /Subtype /Square /F 32 /Rect [50 300 550 500] /Border [0 0 0] /AP << /N 16 0 R >> >>]", "", 6, True),
    "hidden-annotation": (SCAN, "/Annots [<< /Type /Annot /Subtype /Square /F 38 /Rect [50 300 550 500] /AP << /N 16 0 R >> >>]", "", 6, True),
    "square-no-appearance": (SCAN, "/Annots [<< /Type /Annot /Subtype /Square /F 4 /Rect [50 300 550 500] /IC [0 0 0] >>]", "", 6, False),
    "print-only-link-border": (SCAN, "/Annots [<< /Type /Annot /Subtype /Link /F 36 /Rect [50 300 550 500] /Border [0 0 2] /C [0 0 0] >>]", "", 6, False),
    "empty-popup": (SCAN, "/Annots [<< /Type /Annot /Subtype /Popup /F 4 /Rect [50 300 550 500] >>]", "", 6, True),
    "hidden-layer": (b"/OC /oc1 BDC " + SCAN + b"EMC\n" + OCR, "", HIDDEN, 6, False),
    "own-oc": (b"q 612 0 0 792 0 0 cm /O Do Q\n", "", SHOWN, 18, False),
    # denied: an image Gfx refuses to draw is no scan (the render is blank, a copy would not be)
    "imagemask-int": (b"q 612 0 0 792 0 0 cm /R1 Do Q\n" + OCR, "", "", 21, False),
    "imagemask-name": (b"q 612 0 0 792 0 0 cm /R2 Do Q\n" + OCR, "", "", 22, False),
    "no-bpc": (b"q 612 0 0 792 0 0 cm /R3 Do Q\n" + OCR, "", "", 23, False),
    "real-bpc": (b"q 612 0 0 792 0 0 cm /R4 Do Q\n" + OCR, "", "", 24, False),
    "bpc-0": (b"q 612 0 0 792 0 0 cm /R5 Do Q\n" + OCR, "", "", 25, False),
    "bpc-64": (b"q 612 0 0 792 0 0 cm /R6 Do Q\n" + OCR, "", "", 26, False),
    "real-bpc-dct": (b"q 612 0 0 792 0 0 cm /R7 Do Q\n" + OCR, "", "", 27, False),
    "bpc-1": (b"q 612 0 0 792 0 0 cm /R8 Do Q\n" + OCR, "", "", 28, True),
    # a refused image next to the scan: the refused one paints nothing, the scan is still not
    # copied (a dictionary that rules a copy out denies the page wherever it is drawn)
    # the rounding slack is at most half an image pixel: 0.9 pt hides over 20 rows of a strip with 25
    # to a point, on any side, and a CropBox 0.9 pt short does the same
    "strip-full": (b"q 612 0 0 792 0 0 cm /Tall Do Q\n", "", "", 29, True),
    "strip-within-half-row": (b"q 0 0.01 612 791.98 re W n 612 0 0 792 0 0 cm /Tall Do Q\n", "", "", 29, True),
    "strip-clip-top": (b"q 0 0 612 791.1 re W n 612 0 0 792 0 0 cm /Tall Do Q\n", "", "", 29, False),
    "strip-clip-bottom": (b"q 0 0.9 612 791.1 re W n 612 0 0 792 0 0 cm /Tall Do Q\n", "", "", 29, False),
    "strip-clip-left": (b"q 0.9 0 611.1 792 re W n 612 0 0 792 0 0 cm /Wide Do Q\n", "", "", 30, False),
    "strip-clip-right": (b"q 0 0 611.1 792 re W n 612 0 0 792 0 0 cm /Wide Do Q\n", "", "", 30, False),
    "strip-crop-box": (b"q 612 0 0 792 0 0 cm /Tall Do Q\n", "/CropBox [0 0 612 791.1]", "", 29, False),
    "strip-crop-box-left": (b"q 612 0 0 792 0 0 cm /Wide Do Q\n", "/CropBox [0.9 0 612 792]", "", 30, False),
    # a coarse scan (4 pt to a pixel) keeps the full point of slack
    "coarse-clip-0.9": (b"q 0 0 612 791.1 re W n " + SCAN + b"Q\n", "", "", 6, True),
    "refused-and-scan": (SCAN + b"q 100 0 0 100 0 0 cm /R1 Do Q\n", "", "", 6, False),
}


def attrs(path):
    out = utils.run("pdftotext", "-bbox-layout", "-images", path, "-")
    return re.findall(r"<page [^>]*>", out), re.findall(r"<image [^>]*/>", out)


def copy_job(path, obj):
    proc = subprocess.run([os.path.join(utils.bindir, "pdfregions"), path], input=b"p1.1\t1\t0\t0\t10\t10\tcopy\t%d.0\n" % obj, capture_output=True, timeout=60)
    head, _, payload = proc.stdout.partition(b"\n")
    return proc.returncode, head.decode().split("\t"), payload


with t.TempDir() as d:
    for name, (content, page_extra, catalog_extra, obj, want) in CASES.items():
        res = RES
        if name == "default-gray":
            res = RES.replace("/Shading", "/ColorSpace << /DefaultGray [/ICCBased 14 0 R] >> /Shading", 1)
        path = d.write(name + ".pdf", t.simple_page_pdf(content, resources=res, extra_objects=OBJECTS, page_extra=page_extra, catalog_extra=catalog_extra))
        pages, images = attrs(path)
        marked = [i for i in images if ' copyable="1"' in i]
        rc, head, payload = copy_job(path, obj)
        if want:
            t.check(len(marked) == 1 and ' ref="%d 0"' % obj in marked[0] and ' scan="single"' in pages[0], "%s: pdftotext copyable=\"1\" on the scan" % name, (pages, images))
            t.check(rc == 0 and head[1] == "ok", "%s: pdfregions copies it" % name, head)
        else:
            t.check(not marked, "%s: pdftotext writes no copyable=" % name, images)
            t.check(rc == 0 and head[1] == "err:copy", "%s: pdfregions refuses the copy (err:copy)" % name, head)
        if name in ("plain", "ocr-layer"):
            t.check(head[2] == "jpeg" and payload[: int(head[5])] == JPEG, "%s: the copy is the stored JPEG byte for byte" % name, head)
    # the job's object must be the image the page draws: another image of the file is refused
    path = d.write("other.pdf", t.simple_page_pdf(SCAN, resources=RES, extra_objects=OBJECTS))
    rc, head, _ = copy_job(path, 13)
    t.check(head[1] == "err:copy", "a copy job for an image the page does not draw: err:copy", head)
t.finish()
