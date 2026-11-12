"""pdfregions copy jobs (docslurp/extraction/PROTOCOL.md, mode copy) and the pdftotext -images
attributes that decide them (copy=, orient=, overlaid=, scan=).

A copy job names an image XObject; pdfregions answers only for an image reachable from the
job's page whose dictionary still has a copy class: a DCT stream byte for byte (kind=jpeg),
bilevel samples as packed rows (gray1), other samples decoded once (gray8/rgb8). Everything
else is an error record, never a render.
"""

import os
import re
import subprocess
import sys
import zlib

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()


def run(util, *args, stdin=b""):
    cmd = [os.path.join(utils.bindir, util)] + [str(a) for a in args]
    proc = subprocess.run(cmd, input=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
    return proc.returncode, proc.stdout, proc.stderr


def records(out):
    recs, i = [], 0
    while i < len(out):
        j = out.index(b"\n", i)
        head = out[i:j].decode("ascii").split("\t")
        n = int(head[5])
        recs.append((head, out[j + 1 : j + 1 + n]))
        i = j + 1 + n
    return recs


def copy(path, jobs, *args):
    """Runs copy jobs [(id, page, ref)] and returns (exit status, {id: (status, kind, w, h, payload)})."""
    lines = "".join("%s\t%d\t0\t0\t10\t10\tcopy\t%s\n" % j for j in jobs).encode()
    rc, out, err = run("pdfregions", *args, path, stdin=lines)
    return rc, {h[0]: (h[1], h[2], int(h[3]), int(h[4]), p) for h, p in records(out)}, err


def image(dict_body, data, compress=False):
    return t.stream("/Type /XObject /Subtype /Image " + dict_body, data, compress=compress)


GRAY_JPEG = t.jpeg(160, 200)
RGB_JPEG = t.jpeg(37, 21, 3, lambda bx, by, c: [200, 90, 160][c])
G4 = t.ccitt_g4_white(64, 40)
BITS = bytes([0b10110000, 0b01000000] * 6)  # 12 x 6, two bytes a row
RGB = bytes(range(18))
GRAY = bytes([0, 64, 128, 255, 7, 9])
# a form XObject drawing image 13, nested one level (14 -> 15 -> 13 is two levels)
FORM = t.stream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /Deep 13 0 R >> >>", "/Deep Do")


def objects():
    """Objects 6.. of the fixture (see copy_pdf)."""
    return [
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 6
        image("/Width 37 /Height 21 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode", RGB_JPEG),  # 7
        image("/Width 64 /Height 40 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 64 /Rows 40 >>", G4),  # 8
        image("/Width 12 /Height 6 /ColorSpace /DeviceGray /BitsPerComponent 1", BITS, compress=True),  # 9
        image("/Width 3 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8", RGB, compress=True),  # 10
        image("/Width 3 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8", GRAY, compress=True),  # 11
        # not on the page: a JPEG named by no resource
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 12
        # reachable only through the form 14
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 13
        FORM,  # 14
        # soft-masked: class render
        image("/Width 3 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask 11 0 R", RGB, compress=True),  # 15
        # an inverted DCT (/Decode [1 0]): decoded, not copied
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode /Decode [1 0]", GRAY_JPEG),  # 16
        # a DCT stream whose frame is not the declared size
        image("/Width 99 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 17
        # declared DCT, not a JPEG at all
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", b"\xff\xd8not a jpeg\xff\xd9"),  # 18
    ]


XOBJECTS = " ".join("/I%d %d 0 R" % (n, n) for n in (6, 7, 8, 9, 10, 11, 15, 16, 17, 18)) + " /Fm 14 0 R"
CONTENT = b"q 100 0 0 100 0 0 cm /I6 Do Q q 10 0 0 10 200 200 cm /Fm Do Q " + b" ".join(b"q 10 0 0 10 %d 300 cm /I%d Do Q" % (20 * k, n) for k, n in enumerate((7, 8, 9, 10, 11, 15, 16, 17, 18)))


def copy_pdf():
    return t.simple_page_pdf(CONTENT, resources="<< /Font << /F1 4 0 R >> /XObject << %s >> >>" % XOBJECTS, extra_objects=objects())


def test_copies(path):
    rc, r, err = copy(
        path,
        [("p1.1", 1, "6.0"), ("p1.2", 1, "7.0"), ("p1.3", 1, "8.0"), ("p1.4", 1, "9.0"), ("p1.5", 1, "10.0"), ("p1.6", 1, "11.0"), ("p1.7", 1, "13.0"), ("p1.8", 1, "16.0")],
    )
    t.check(rc == 0, "copy jobs: exit 0", err)
    t.check(r["p1.1"][:4] == ("ok", "jpeg", 160, 200), "grey DCT: kind jpeg, its size", r["p1.1"][:4])
    t.check(r["p1.1"][4] == GRAY_JPEG, "grey DCT: the embedded stream byte for byte")
    t.check(r["p1.2"][:4] == ("ok", "jpeg", 37, 21) and r["p1.2"][4] == RGB_JPEG, "RGB DCT: byte for byte", r["p1.2"][:4])
    t.check(r["p1.3"][:4] == ("ok", "gray1", 64, 40), "CCITT: kind gray1", r["p1.3"][:4])
    t.check(r["p1.3"][4] == b"\xff" * (8 * 40), "CCITT: ceil(w/8)*h bytes, all white")
    t.check(r["p1.4"][:4] == ("ok", "gray1", 12, 6) and r["p1.4"][4] == BITS, "1-bpc grey: the rows as stored", r["p1.4"])
    t.check(r["p1.5"][:4] == ("ok", "rgb8", 3, 2) and r["p1.5"][4] == RGB, "Flate RGB: decoded at native size", r["p1.5"])
    t.check(r["p1.6"][:4] == ("ok", "gray8", 3, 2) and r["p1.6"][4] == GRAY, "Flate grey: gray8", r["p1.6"])
    t.check(r["p1.7"][:2] == ("ok", "jpeg") and r["p1.7"][4] == GRAY_JPEG, "an image inside a form on the page is reachable", r["p1.7"][:4])
    # /Decode [1 0] is not a JPEG to copy: decoded through the map (inverted), grey
    kind = r["p1.8"]
    t.check(kind[:4] == ("ok", "gray8", 160, 200), "DCT with /Decode: decoded, not copied", kind[:4])
    t.check(kind[4][0] == 255 - 28, "DCT with /Decode: inverted samples (block level 28)", kind[4][:4])


def test_refusals(path):
    rc, r, err = copy(path, [("p1.1", 1, "12.0"), ("p1.2", 1, "5.0"), ("p1.3", 1, "4.0"), ("p1.4", 1, "15.0"), ("p1.5", 1, "17.0"), ("p1.6", 1, "18.0"), ("p1.7", 1, "999.0"), ("p1.8", 1, "6.1"), ("p2.1", 2, "6.0")])
    t.check(rc == 0, "refusals: still exit 0 (error records)", err)
    want = {
        "p1.1": "err:copy",  # an image of the file, not named by this page
        "p1.2": "err:copy",  # the content stream: not an image
        "p1.3": "err:copy",  # a font
        "p1.4": "err:copy",  # soft-masked: class render
        "p1.5": "err:jpeg",  # frame size is not /Width
        "p1.6": "err:jpeg",  # not a JPEG
        "p1.7": "err:copy",  # no such object
        "p1.8": "err:copy",  # wrong generation
        "p2.1": "err:page",  # no page 2
    }
    for k, v in want.items():
        t.check(r.get(k, ("missing",))[0] == v, "refused: %s -> %s" % (k, v), r.get(k))
    t.check(all(rec[2] == 0 and rec[4] == b"" for rec in r.values() if rec[0].startswith("err:")), "error records carry nothing")


def test_limits(path):
    rc, r, _ = copy(path, [("p1.1", 1, "6.0")], "-max-copy-bytes", len(GRAY_JPEG) - 1)
    t.check(rc == 0 and r["p1.1"][0] == "err:copysize", "a JPEG over -max-copy-bytes is err:copysize", r)
    rc, r, _ = copy(path, [("p1.1", 1, "6.0")], "-max-copy-bytes", len(GRAY_JPEG))
    t.check(r["p1.1"][0] == "ok", "a JPEG of exactly -max-copy-bytes is copied", r["p1.1"][:4])
    rc, r, _ = copy(path, [("p1.1", 1, "8.0"), ("p1.2", 1, "10.0")], "-max-copy-pixels", 6)
    t.check(r["p1.1"][0] == "err:pixels" and r["p1.2"][0] == "ok", "-max-copy-pixels caps decoded copies", r)
    # the whole-run output cap still holds
    rc, _, _ = copy(path, [("p1.1", 1, "6.0")], "-max-output-bytes", 100)
    t.check(rc == 3, "a copy record past -max-output-bytes exits 3", rc)


def test_bad_job_lines(path):
    for ref in ["0.0", "6", "6.0.0", "6.70000", "06.0", "6.-1", "x.0", "6.", ".0", "1234567890.0"]:
        rc, out, _ = run("pdfregions", path, stdin=("p1.1\t1\t0\t0\t10\t10\tcopy\t%s\n" % ref).encode())
        t.check(rc == 2 and out == b"", "malformed copy ref %r: exit 2, nothing written" % ref, rc)


def test_deep_forms(d):
    # the image sits under 5 nested forms: past the 4 levels a copy job searches
    objs = [image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG)]  # 6
    for k in range(5):
        objs.append(t.stream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /X %d 0 R >> >>" % (6 + k), "/X Do"))  # 7..11
    pdf = t.simple_page_pdf(b"/F Do", resources="<< /XObject << /F 11 0 R >> >>", extra_objects=objs)
    path = d.write("deep.pdf", pdf)
    rc, r, _ = copy(path, [("p1.1", 1, "6.0")])
    t.check(r["p1.1"][0] == "err:copy", "an image 5 forms deep is not searched for", r)
    pdf = t.simple_page_pdf(b"/F Do", resources="<< /XObject << /F 10 0 R >> >>", extra_objects=objs)
    path = d.write("four.pdf", pdf)
    rc, r, _ = copy(path, [("p1.1", 1, "6.0")])
    t.check(r["p1.1"][0] == "ok", "an image 4 forms deep is found", r)


def attributes(path):
    rc, out, _ = run("pdftotext", "-bbox-layout", "-images", path, "-")
    assert rc == 0
    text = out.decode()
    return re.findall(r"<page [^>]*>", text), re.findall(r"<image [^>]*/>", text)


def test_attributes(d):
    # an upright DCT scan, then the same drawn turned (cm with a rotation), flipped, and on a
    # /Rotate 90 page; an inline image is never a copy
    objs = [image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG)]
    res = "<< /Font << /F1 4 0 R >> /XObject << /S 6 0 R >> >>"
    cases = {
        "upright": (b"q 612 0 0 792 0 0 cm /S Do Q", "", "0"),
        "turned": (b"q 0 -792 612 0 0 792 cm /S Do Q", "", "90"),
        "upside-down": (b"q -612 0 0 -792 612 792 cm /S Do Q", "", "180"),
        "flipped": (b"q 612 0 0 -792 0 792 cm /S Do Q", "", "other"),
        "mirrored": (b"q -612 0 0 792 612 0 cm /S Do Q", "", "other"),
        "rotated-page": (b"q 612 0 0 792 0 0 cm /S Do Q", "/Rotate 90", "90"),
    }
    for name, (content, extra, orient) in cases.items():
        path = d.write(name + ".pdf", t.simple_page_pdf(content, resources=res, extra_objects=objs, page_extra=extra))
        pages, images = attributes(path)
        got = re.search(r' orient="([^"]+)"', images[0]) if images else None
        t.check((got.group(1) if got else None) == orient, "%s: orient=%s" % (name, orient), images)
        t.check(' copy="jpeg"' in images[0] and ' scan="single" scanImage="1"' in pages[0], "%s: a single DCT scan, copy=jpeg" % name, (pages, images))
    # inline: no ref, copy=render
    inline = b"q 612 0 0 792 0 0 cm BI /W 2 /H 2 /CS /G /BPC 8 ID \x00\x80\xff\x10 EI Q"
    path = d.write("inline.pdf", t.simple_page_pdf(inline))
    pages, images = attributes(path)
    t.check(len(images) == 1 and ' inline="1"' in images[0] and ' copy="render"' in images[0] and " ref=" not in images[0], "inline image: copy=render, no ref", images)
    # a full-page image under 200 visible words is a backdrop, not a scan
    words = b"".join(b"BT /F1 9 Tf 40 %d Td (word word word word word word word word word word) Tj ET\n" % (760 - 36 * k) for k in range(20))
    path = d.write("backdrop.pdf", t.simple_page_pdf(b"q 612 0 0 792 0 0 cm /S Do Q\n" + words, resources=res, extra_objects=objs))
    pages, _ = attributes(path)
    t.check(" scan=" not in pages[0], "200 visible words over a page image: no scan=", pages)
    # with an invisible (3 Tr) OCR layer it is a scan
    ocr = words.replace(b"BT ", b"BT 3 Tr ")
    path = d.write("ocr.pdf", t.simple_page_pdf(b"q 612 0 0 792 0 0 cm /S Do Q\n" + ocr, resources=res, extra_objects=objs))
    pages, _ = attributes(path)
    t.check(' scan="single" scanImage="1"' in pages[0], "an invisible OCR layer keeps the scan", pages)


def test_overlaid(d):
    # A copy of a scan's pixels shows the page only when nothing else shows over them:
    # pdftotext -images says overlaid="0" only then (a redaction box, a white-out with text, an
    # image patch, a shading, a clip, the page edge, a soft mask or opacity all make it "1").
    objs = [
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 6
        image("/Width 3 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8", GRAY, compress=True),  # 7: a patch
        t.stream("/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Group << /S /Transparency >>", "0 g 0 0 612 792 re f"),  # 8: mask group
    ]
    shading = "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 612 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >>"
    res = (
        "<< /Font << /F1 4 0 R >> /XObject << /S 6 0 R /P 7 0 R >> /Shading << /Sh %s >> "
        "/ExtGState << /Half << /ca 0.5 >> /Mul << /BM /Multiply >> /Mask << /SMask << /S /Luminosity /G 8 0 R >> >> >> >>" % shading
    )
    scan = b"q 612 0 0 792 0 0 cm /S Do Q\n"
    ocr = b"BT 3 Tr /F1 11 Tf 72 700 Td (Scanned words of the page) Tj ET\n"
    cases = {
        "plain": (scan, "0"),
        "ocr-layer": (scan + ocr, "0"),
        "box-underneath": (b"q 0 g 50 300 500 200 re f Q\n" + scan, "0"),
        "clip-restored": (b"q 0 0 10 10 re W n Q q 0 0 m 10 0 l 0 10 l h W n Q\n" + scan, "0"),
        "redaction-box": (scan + b"0 g 50 300 500 200 re f\n", "1"),
        "stroke": (scan + b"0 G 4 w 100 100 m 300 300 l S\n", "1"),
        "white-out-text": (scan + b"1 g 50 300 500 200 re f BT 0 g /F1 24 Tf 200 390 Td (REDACTED) Tj ET\n", "1"),
        "visible-text": (scan + b"BT /F1 11 Tf 72 700 Td (Bates 000123) Tj ET\n", "1"),
        "image-patch": (scan + b"q 500 0 0 200 50 300 cm /P Do Q\n", "1"),
        "shading": (scan + b"q 50 300 500 200 re W n /Sh sh Q\n", "1"),
        "clipped": (b"q 0 92 612 700 re W n " + scan + b"Q\n", "1"),
        "odd-clip": (b"q 0 0 612 792 re 100 100 50 50 re W* n " + scan + b"Q\n", "1"),
        "triangle-clip": (b"q 0 0 m 612 0 l 612 792 l 0 792 l 306 400 l h W n " + scan + b"Q\n", "1"),
        "text-clip": (b"q BT 7 Tr /F1 400 Tf 0 100 Td (W) Tj ET " + scan + b"Q\n", "1"),
        "off-page": (b"q 1224 0 0 792 -612 0 cm /S Do Q\n", "1"),
        "opacity": (b"q /Half gs " + scan + b"Q\n", "1"),
        "blend": (b"q /Mul gs " + scan + b"Q\n", "1"),
        "soft-mask": (b"q /Mask gs " + scan + b"Q\n", "1"),
        "annotation": (scan, "1"),
    }
    for name, (content, want) in cases.items():
        extra = ""
        if name == "annotation":
            # a square annotation's appearance painted over the scan
            objs_a = objs + [t.stream("/Type /XObject /Subtype /Form /BBox [0 0 500 200]", "0 g 0 0 500 200 re f")]  # 9
            extra = "/Annots [<< /Type /Annot /Subtype /Square /Rect [50 300 550 500] /AP << /N 9 0 R >> >>]"
            pdf = t.simple_page_pdf(content, resources=res, extra_objects=objs_a, page_extra=extra)
        else:
            pdf = t.simple_page_pdf(content, resources=res, extra_objects=objs)
        path = d.write("over-%s.pdf" % name, pdf)
        pages, images = attributes(path)
        scan_img = images[0] if images else ""
        got = re.search(r' overlaid="([01])"', scan_img)
        t.check((got.group(1) if got else None) == want, "%s: overlaid=%s" % (name, want), (pages, images))
        t.check(' orient="' in scan_img, "%s: orient= written with a copy class" % name, scan_img)


with t.TempDir() as d:
    path = d.write("copy.pdf", copy_pdf())
    test_copies(path)
    test_refusals(path)
    test_limits(path)
    test_bad_job_lines(path)
    test_deep_forms(d)
    test_attributes(d)
    test_overlaid(d)
t.finish()
