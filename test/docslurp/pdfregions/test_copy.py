"""pdfregions copy jobs (docslurp/extraction/PROTOCOL.md, mode copy) and the pdftotext -images
attributes that decide them (copy=, orient=, overlaid=, scan=).

A copy job names an image XObject; pdfregions answers only for an image reachable from the
job's page whose dictionary still has a copy class: a DCT stream byte for byte (kind=jpeg),
bilevel samples as packed rows (gray1), other samples decoded once (gray8/rgb8). Everything
else is an error record, never a render. The page must also pass the copy allow-list (the
image is the only thing it paints, test_allowlist.py), so each fixture draws one image a page.
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


def pages_pdf(contents, resources, extra_objects=()):
    """A PDF with one page per content stream, all sharing <resources>. Objects: 1 catalog, 2
    pages, 3 page 1, 4 Helvetica, 5 content 1, 6.. extra_objects, then a page and its content
    for each further page."""
    extra = list(extra_objects)
    n = 6 + len(extra)
    kids = ["3 0 R"] + ["%d 0 R" % (n + 2 * k) for k in range(len(contents) - 1)]
    page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources %s /Contents %%d 0 R >>" % resources
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join(kids), len(contents)),
        page % 5,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        t.stream("", contents[0]),
    ] + extra
    for k, c in enumerate(contents[1:]):
        objects += [page % (n + 2 * k + 1), t.stream("", c)]
    return t.build_pdf(objects)


def one_each(names):
    """One page per XObject name, drawing it alone in a 10 pt square."""
    return [b"q 10 0 0 10 0 0 cm /%s Do Q" % n.encode() for n in names]


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
# one draw a page: page 1 I6, page 2 the form (image 13), pages 3.. I7, I8, I9, I10, I11, I15, I16, I17, I18
CONTENTS = [b"q 100 0 0 100 0 0 cm /I6 Do Q", b"q 10 0 0 10 200 200 cm /Fm Do Q"] + [b"q 10 0 0 10 20 300 cm /I%d Do Q" % n for n in (7, 8, 9, 10, 11, 15, 16, 17, 18)]
PAGE_OF = {6: 1, 13: 2, 7: 3, 8: 4, 9: 5, 10: 6, 11: 7, 15: 8, 16: 9, 17: 10, 18: 11}


def copy_pdf():
    return pages_pdf(CONTENTS, "<< /Font << /F1 4 0 R >> /XObject << %s >> >>" % XOBJECTS, objects())


def job(k, obj, page=None):
    page = page or PAGE_OF[obj]
    return ("p%d.%d" % (page, k), page, "%d.0" % obj)


def test_copies(path):
    objs = (6, 13, 7, 8, 9, 10, 11, 16)  # in page order
    keys = dict(zip(objs, ("p1.1", "p1.7", "p1.2", "p1.3", "p1.4", "p1.5", "p1.6", "p1.8")))
    rc, got, err = copy(path, [job(1, n) for n in objs])
    r = {keys[n]: got["p%d.1" % PAGE_OF[n]] for n in objs}
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
    jobs = [("p1.1", 1, "12.0"), ("p1.2", 1, "5.0"), ("p1.3", 1, "4.0"), ("p1.7", 1, "999.0"), ("p1.8", 1, "6.1"), ("p1.9", 1, "7.0"), ("p8.1", 8, "15.0"), ("p10.1", 10, "17.0"), ("p11.1", 11, "18.0"), ("p99.1", 99, "6.0")]
    rc, got, err = copy(path, jobs)
    r = {"p1.4": got.get("p8.1"), "p1.5": got.get("p10.1"), "p1.6": got.get("p11.1"), "p2.1": got.get("p99.1")}
    r.update({k: got.get(k) for k in ("p1.1", "p1.2", "p1.3", "p1.7", "p1.8", "p1.9")})
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
        "p1.9": "err:copy",  # named by the page's resources, but not what the page draws
        "p2.1": "err:page",  # no page 99
    }
    for k, v in want.items():
        t.check(r.get(k, ("missing",))[0] == v, "refused: %s -> %s" % (k, v), r.get(k))
    t.check(all(rec[2] == 0 and rec[4] == b"" for rec in r.values() if rec[0].startswith("err:")), "error records carry nothing")


def test_limits(path):
    rc, r, _ = copy(path, [("p1.1", 1, "6.0")], "-max-copy-bytes", len(GRAY_JPEG) - 1)
    t.check(rc == 0 and r["p1.1"][0] == "err:copysize", "a JPEG over -max-copy-bytes is err:copysize", r)
    rc, r, _ = copy(path, [("p1.1", 1, "6.0")], "-max-copy-bytes", len(GRAY_JPEG))
    t.check(r["p1.1"][0] == "ok", "a JPEG of exactly -max-copy-bytes is copied", r["p1.1"][:4])
    rc, r, _ = copy(path, [job(1, 8), job(1, 10)], "-max-copy-pixels", 6)
    t.check(r["p4.1"][0] == "err:pixels" and r["p6.1"][0] == "ok", "-max-copy-pixels caps decoded copies", r)
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
        # 9: the scan inside a transparency group (drawForm resets opacity and blend mode inside)
        t.stream("/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Group << /S /Transparency >> /Resources << /XObject << /S 6 0 R >> >>", "q 612 0 0 792 0 0 cm /S Do Q"),
        # 10, 11: a Type 3 font whose glyph paints far from its origin and em box
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [1 0 0 1 0 0] /CharProcs << /a 11 0 R >> /Encoding << /Differences [97 /a] >> /FirstChar 97 /LastChar 97 /Widths [1000] /Resources << >> >>",
        t.stream("", "1000 0 d0 -5300 0 5000 792 re f"),
    ]
    shading = "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 612 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >>"
    res = (
        "<< /Font << /F1 4 0 R /T3 10 0 R >> /XObject << /S 6 0 R /P 7 0 R /G 9 0 R >> /Shading << /Sh %s >> "
        "/ExtGState << /Half << /ca 0.5 >> /Zero << /ca 0 >> /Mul << /BM /Multiply >> /Mask << /SMask << /S /Luminosity /G 8 0 R >> >> >> >>" % shading
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
        # a transparency group around the scan: the group's /ca 0 (or blend mode, or soft mask)
        # applies when it is composited, which a text device never sees
        "group-ca0": (b"q /Zero gs /G Do Q\n", "1"),
        "group": (b"/G Do\n", "1"),
        # a Type 3 glyph whose origin is off the page but whose charproc paints over the scan
        "type3-glyph": (scan + b"BT /T3 1 Tf 5300 0 Td (a) Tj ET\n", "1"),
        # a stroke under a CTM that stretches the pen 10^8 times more along x than y: drawn from
        # off the page, its ink covers the scan
        "anisotropic-stroke": (scan + b"q 1 J 0 j 1 M 10000 0 0 0.0001 0 0 cm 0 G 1 w 0.53 0 m 0.53 7920000 l S Q\n", "1"),
    }
    for name, (content, want) in cases.items():
        extra = ""
        if name == "annotation":
            # a square annotation's appearance painted over the scan
            objs_a = objs + [t.stream("/Type /XObject /Subtype /Form /BBox [0 0 500 200]", "0 g 0 0 500 200 re f")]  # 12
            extra = "/Annots [<< /Type /Annot /Subtype /Square /Rect [50 300 550 500] /AP << /N 12 0 R >> >>]"
            pdf = t.simple_page_pdf(content, resources=res, extra_objects=objs_a, page_extra=extra)
        else:
            pdf = t.simple_page_pdf(content, resources=res, extra_objects=objs)
        path = d.write("over-%s.pdf" % name, pdf)
        pages, images = attributes(path)
        scan_img = images[0] if images else ""
        got = re.search(r' overlaid="([01])"', scan_img)
        t.check((got.group(1) if got else None) == want, "%s: overlaid=%s" % (name, want), (pages, images))
        t.check(' orient="' in scan_img, "%s: orient= written with a copy class" % name, scan_img)


def segments(data):
    """The marker codes of a JPEG's segments up to its first scan, and whether a COM or APPn
    other than JFIF APP0 / Adobe APP14 appears anywhere."""
    codes, i = [], 2
    while i + 4 <= len(data) and data[i] == 0xFF:
        m = data[i + 1]
        codes.append(m)
        if m == 0xDA:
            break
        i += 2 + ((data[i + 2] << 8) | data[i + 3])
    return codes


def test_jpeg_metadata(d):
    # Exif (orientation 6, a path in a string), an XMP packet, a comment, and a non-canonical
    # Adobe segment: the copy keeps the frame, tables and scan, a canonical JFIF and Adobe
    exif = b"Exif\0\0MM\0*\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0\x06\0\0\0\0/home/victim/private/path.pdf"
    xmp = b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta>GPS 51.5 -0.12</x:xmpmeta>"
    com = b"SECRET-COMMENT /home/victim/private/path.pdf"

    def seg(marker, body):
        return bytes([0xFF, marker]) + (len(body) + 2).to_bytes(2, "big") + body

    soi_jfif = GRAY_JPEG[:20]  # SOI and the canonical JFIF APP0 of t.jpeg
    rest = GRAY_JPEG[20:]
    sos = rest.index(b"\xff\xda")
    dirty = soi_jfif + seg(0xE1, exif) + seg(0xE1, xmp) + seg(0xFE, com) + rest[:sos] + seg(0xFE, com) + rest[sos:]
    rgb_rest = RGB_JPEG[20:]
    adobe = RGB_JPEG[:2] + seg(0xEE, b"Adobe\x00\x65\x80\x00\x12\x34\x01extra") + seg(0xE2, b"ICC_PROFILE\0\x01\x01junk") + rgb_rest
    objs = [
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", dirty),  # 6
        image("/Width 37 /Height 21 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode", adobe),  # 7
    ]
    pdf = pages_pdf(one_each(["A", "B"]), "<< /XObject << /A 6 0 R /B 7 0 R >> >>", objs)
    path = d.write("exif.pdf", pdf)
    rc, r, err = copy(path, [("p1.1", 1, "6.0"), ("p2.1", 2, "7.0")])
    r["p1.2"] = r["p2.1"]
    t.check(rc == 0 and r["p1.1"][:4] == ("ok", "jpeg", 160, 200), "a JPEG with metadata is still copied", r.get("p1.1", ("missing",))[:4])
    t.check(r["p1.1"][4] == GRAY_JPEG, "Exif, XMP and comments (before the frame and before the scan) are dropped; the rest is the stream as stored")
    t.check(b"SECRET" not in r["p1.1"][4] and b"victim" not in r["p1.1"][4] and b"Exif" not in r["p1.1"][4], "no document string reaches the copy")
    clean = r["p1.2"][4]
    canon = b"\xff\xee\x00\x0eAdobe\x00\x64\x00\x00\x00\x00\x01"
    t.check(r["p1.2"][0] == "ok" and canon in clean and b"extra" not in clean and b"ICC_PROFILE" not in clean, "Adobe APP14 becomes canonical (its transform kept); ICC is dropped", clean[:40])
    t.check(segments(clean)[:2] == [0xEE, 0xDB] or 0xE2 not in segments(clean), "no APP2 left", segments(clean))
    # a stream with metadata after EOI, or another image inside: still refused
    nested = GRAY_JPEG[:-2] + GRAY_JPEG + b"\xff\xd9"
    pdf = t.simple_page_pdf(b"q 10 0 0 10 0 0 cm /A Do Q", resources="<< /XObject << /A 6 0 R >> >>", extra_objects=[image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", nested)])
    rc, r, _ = copy(d.write("nested.pdf", pdf), [("p1.1", 1, "6.0")])
    t.check(r["p1.1"][0] == "err:jpeg", "a JPEG with another JPEG inside is refused", r["p1.1"][:4])


def decode_dc_jpeg(data):
    """The colour of the first 8x8 block of a DC-only baseline JPEG (t.jpeg's kind: every block
    flat) as a libjpeg viewer shows it: the colour transform chosen as libjpeg chooses it
    (jdapimin.c: a JFIF APP0 with 14 data bytes means YCbCr; else an Adobe APP14 with 12, by its
    transform; else component ids 'R','G','B' mean RGB; else YCbCr)."""
    i, jfif, adobe, quant, dc_tables, comps, scan_at = 2, False, None, {}, {}, [], None
    while i + 4 <= len(data):
        m, n = data[i + 1], (data[i + 2] << 8) | data[i + 3]
        body = data[i + 4 : i + 2 + n]
        if m == 0xE0 and n - 2 >= 14 and body[:5] == b"JFIF\0":
            jfif = True
        elif m == 0xEE and n - 2 >= 12 and body[:5] == b"Adobe":
            adobe = body[11]
        elif m == 0xDB:
            quant[body[0] & 15] = body[1]  # the DC entry
        elif m == 0xC4 and body[0] >> 4 == 0:
            counts, vals, codes, code, k = body[1:17], body[17:], {}, 0, 0
            for length in range(1, 17):
                for _ in range(counts[length - 1]):
                    codes[(length, code)] = vals[k]
                    code, k = code + 1, k + 1
                code <<= 1
            dc_tables[body[0] & 15] = codes
        elif m == 0xC0:
            comps = [(body[6 + 3 * c], body[8 + 3 * c]) for c in range(body[5])]
        elif m == 0xDA:
            scan_at = i + 2 + n
            break
        i += 2 + n
    bits = []
    for b in data[scan_at : scan_at + 64]:
        bits.extend((b >> (7 - k)) & 1 for k in range(8))
    pos, samples = 0, []
    for cid, tq in comps:
        code, length = 0, 0
        while (length, code) not in dc_tables[0]:
            code, length, pos = (code << 1) | bits[pos], length + 1, pos + 1
        size = dc_tables[0][(length, code)]
        diff = 0
        for _ in range(size):
            diff, pos = (diff << 1) | bits[pos], pos + 1
        if size and diff < 1 << (size - 1):
            diff -= (1 << size) - 1
        pos += 1  # end of block (the AC code "0")
        samples.append(max(0, min(255, round(128 + diff * quant[tq] / 8))))
    ids = [cid for cid, _ in comps]
    rgb = len(comps) == 3 and (not jfif and ((adobe is not None and adobe == 0) or (adobe is None and ids == [82, 71, 66])))
    if len(comps) == 1 or rgb:
        return samples
    y, cb, cr = samples
    return [max(0, min(255, round(v))) for v in (y + 1.402 * (cr - 128), y - 0.344136 * (cb - 128) - 0.714136 * (cr - 128), y + 1.772 * (cb - 128))]


def ppm_mean(data):
    """The mean RGB of a binary PPM (P6, maxval 255)."""
    fields, i = [], 0
    while len(fields) < 4:
        while data[i : i + 1].isspace():
            i += 1
        j = i
        while not data[j : j + 1].isspace():
            j += 1
        fields.append(data[i:j])
        i = j
    w, h = int(fields[1]), int(fields[2])
    px = data[i + 1 : i + 1 + 3 * w * h]
    return [sum(px[c::3]) / (w * h) for c in range(3)]


def test_jpeg_colours(d):
    # The copy must decode in a viewer (libjpeg) to the colours poppler renders. poppler's
    # DCTStream lets an Adobe APP14 decide the colour transform even beside JFIF, and sees JFIF
    # only in an APP0 with 14 data bytes; a viewer lets JFIF win. So: an RGB JPEG (ids 'R','G',
    # 'B') with a short "JFIF" APP0 must not gain a JFIF segment, and an Adobe transform-0 JPEG
    # with a JFIF APP0 must lose it.
    def seg(marker, body):
        return bytes([0xFF, marker]) + (len(body) + 2).to_bytes(2, "big") + body

    base = t.jpeg(64, 64, 3, lambda bx, by, c: [200, 60, 90][c])
    sof, sos = base.index(b"\xff\xc0"), base.index(b"\xff\xda")
    rgb_ids = bytearray(base)
    for c, cid in enumerate(b"RGB"):
        rgb_ids[sof + 10 + 3 * c] = cid
        rgb_ids[sos + 5 + 2 * c] = cid
    rgb_ids = bytes(rgb_ids)
    jfif = base[2:20]
    cases = {
        "rgb-ids-short-jfif": rgb_ids[:2] + seg(0xE0, b"JFIF\0") + rgb_ids[20:],
        "adobe-0-with-jfif": base[:2] + jfif + seg(0xEE, b"Adobe\x00\x64\x00\x00\x00\x00\x00") + base[20:],
        "adobe-1-with-jfif": base[:2] + jfif + seg(0xEE, b"Adobe\x00\x64\x00\x00\x00\x00\x01") + base[20:],
        "jfif": base,
        "rgb-ids": rgb_ids[:2] + rgb_ids[20:],
    }
    try:
        from PIL import Image  # optional: a second, real libjpeg decode
        import io
    except ImportError:
        Image = None
    for name, data in cases.items():
        pdf = t.simple_page_pdf(b"q 64 0 0 64 0 0 cm /A Do Q", resources="<< /XObject << /A 6 0 R >> >>", mediabox=(0, 0, 64, 64), extra_objects=[image("/Width 64 /Height 64 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode", data)])
        path = d.write(name + ".pdf", pdf)
        rc, r, _ = copy(path, [("p1.1", 1, "6.0")])
        t.check(r["p1.1"][:2] == ("ok", "jpeg"), "%s: copied" % name, r["p1.1"][:4])
        rc, ppm, _ = run("pdftoppm", "-r", "72", path)
        rendered = ppm_mean(ppm)
        copied = decode_dc_jpeg(r["p1.1"][4])
        diff = sum(abs(a - b) for a, b in zip(rendered, copied)) / 3
        t.check(diff < 2, "%s: the copy decodes to the rendered colour (mean abs diff %.2f)" % (name, diff), (rendered, copied))
        if Image is not None:
            im = Image.open(io.BytesIO(r["p1.1"][4])).convert("RGB")
            px = list(im.getdata())
            pil = [sum(p[c] for p in px) / len(px) for c in range(3)]
            diff = sum(abs(a - b) for a, b in zip(rendered, pil)) / 3
            t.check(diff < 2, "%s: libjpeg (PIL) decodes the copy to the rendered colour (mean abs diff %.2f)" % (name, diff), (rendered, pil))


def icc_gray(trc):
    """A minimal ICC v2 grey display profile whose kTRC is the curve <trc> (16-bit entries)."""
    import struct

    desc = b"desc" + b"\0" * 4 + struct.pack(">I", 2) + b"x\0" + b"\0" * 8 + b"\0" * 3 + b"\0" * 67
    wtpt = b"XYZ " + b"\0" * 4 + struct.pack(">iii", int(0.9642 * 65536), 65536, int(0.8249 * 65536))
    curv = b"curv" + b"\0" * 4 + struct.pack(">I", len(trc)) + b"".join(struct.pack(">H", v) for v in trc)
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
    assert len(head) == 128
    return head + table + body


def test_color_spaces(d):
    # stored samples are copied only in a Device space: an ICC profile (here a constant-white
    # curve, which renders the page white), CalGray, or a DefaultGray of the resources can make
    # a viewer show something else, so those are decoded through their colour space or rendered
    objs = [
        image("/Width 160 /Height 200 /ColorSpace [/ICCBased 9 0 R] /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 6
        image("/Width 160 /Height 200 /ColorSpace [/CalGray << /WhitePoint [0.9505 1 1.089] /Gamma 2.2 >>] /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 7
        image("/Width 64 /Height 40 /ColorSpace [/ICCBased 9 0 R] /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 64 /Rows 40 >>", G4),  # 8
        t.stream("/N 1 /Alternate /DeviceGray", icc_gray([0xFFFF, 0xFFFF])),  # 9
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 10
    ]
    content = b"q 200 0 0 250 0 0 cm /A Do Q q 200 0 0 250 200 0 cm /B Do Q q 100 0 0 60 400 0 cm /C Do Q"
    res = "<< /XObject << /A 6 0 R /B 7 0 R /C 8 0 R >> >>"
    path = d.write("icc.pdf", t.simple_page_pdf(content, resources=res, extra_objects=objs))
    _, images = attributes(path)
    classes = [re.search(r' copy="(\w+)"', i).group(1) for i in images]
    t.check(classes == ["decode", "decode", "decode"], "ICCBased DCT, CalGray DCT, ICCBased CCITT: copy=decode, not jpeg or bilevel", images)
    # the copy allow-list refuses ICC-based and calibrated scans outright: they are rendered
    path = d.write("icc-pages.pdf", pages_pdf(one_each(["A", "B", "C"]), res, objs))
    rc, r, _ = copy(path, [("p1.1", 1, "6.0"), ("p2.1", 2, "7.0"), ("p3.1", 3, "8.0")])
    t.check([r[k][0] for k in ("p1.1", "p2.1", "p3.1")] == ["err:copy"] * 3, "ICC-based and calibrated images are not copied (err:copy)", {k: v[:4] for k, v in r.items()})
    # a Device space that a DefaultGray of the resources replaces: rendered
    res = "<< /ColorSpace << /DefaultGray [/ICCBased 9 0 R] >> /XObject << /A 10 0 R >> >>"
    path = d.write("default-gray.pdf", t.simple_page_pdf(b"q 612 0 0 792 0 0 cm /A Do Q", resources=res, extra_objects=objs))
    _, images = attributes(path)
    t.check(len(images) == 1 and ' copy="render"' in images[0], "DeviceGray under a DefaultGray: copy=render", images)


def test_jpeg_structure(d):
    # a copy must be a JPEG a decoder accepts, not only one framed as segments: a Huffman table
    # that is not a prefix code, a quantisation table id past 3, a scan naming a component the
    # frame lacks, or a sequential scan with a spectral range other than 0..63 is err:jpeg
    def mutate(marker, at, value):
        k = GRAY_JPEG.index(bytes([0xFF, marker]))
        b = bytearray(GRAY_JPEG)
        b[k + at] = value
        return bytes(b)

    bad = [
        mutate(0xC4, 5 + 1, 3),  # DHT: three codes of length 1 (with the counts after, still framed)
        mutate(0xDB, 4, 0x07),  # DQT: table id 7
        mutate(0xDA, 5, 0x09),  # SOS: component 9
        mutate(0xDA, 5 + 2 + 1, 62),  # SOS: Se 62 in a baseline scan
    ]
    objs = [image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", data) for data in bad]
    res = "<< /XObject << %s >> >>" % " ".join("/I%d %d 0 R" % (6 + k, 6 + k) for k in range(len(bad)))
    path = d.write("bad-jpeg.pdf", pages_pdf(one_each(["I%d" % (6 + k) for k in range(len(bad))]), res, objs))
    rc, r, _ = copy(path, [("p%d.1" % (k + 1), k + 1, "%d.0" % (6 + k)) for k in range(len(bad))])
    got = [r["p%d.1" % (k + 1)][0] for k in range(len(bad))]
    t.check(rc == 0 and got == ["err:jpeg"] * len(bad), "bogus Huffman table, DQT id, SOS component, baseline Se: err:jpeg", got)


def test_jbig2_regions(d):
    # the page size alone does not bound a JBIG2 decode: a 10x10 page with a 60000x60000 generic
    # region (some 150 bytes) is refused before decoding; a region that fits is decoded
    import struct

    def seg(num, typ, data):
        return struct.pack(">IBBBI", num, typ, 0, 1, len(data)) + data

    page = seg(0, 48, struct.pack(">IIIIBH", 10, 10, 0, 0, 0, 0))

    def region(w, h):
        return seg(1, 38, struct.pack(">IIIIB", w, h, 0, 0, 0) + bytes([0x08, 3, 0xFF, 0xFD, 0xFF, 2, 0xFE, 0xFE, 0xFE]) + b"\x00" * 64 + b"\xff\xac")

    pattern = seg(1, 16, bytes([0, 255, 255]) + struct.pack(">I", 0xFFFFFFFF) + b"\x00" * 16)
    objs = [
        image("/Width 10 /Height 10 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode", page + region(60000, 60000)),  # 6
        image("/Width 10 /Height 10 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode", page + pattern),  # 7
        image("/Width 10 /Height 10 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode /DecodeParms << /JBIG2Globals 9 0 R >>", page + region(10, 10)),  # 8
        t.stream("", region(60000, 60000)),  # 9: globals with a huge region
        image("/Width 10 /Height 10 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode", page + region(10, 10)),  # 10
    ]
    res = "<< /XObject << %s >> >>" % " ".join("/I%d %d 0 R" % (n, n) for n in (6, 7, 8, 10))
    path = d.write("jbig2-regions.pdf", pages_pdf(one_each(["I%d" % n for n in (6, 7, 8, 10)]), res, objs))
    import time

    start = time.monotonic()
    rc, r, _ = copy(path, [("p%d.1" % (k + 1), k + 1, "%d.0" % n) for k, n in enumerate((6, 7, 8, 10))])
    secs = time.monotonic() - start
    got = [r["p%d.1" % k][0] for k in (1, 2, 3, 4)]
    t.check(rc == 0 and got == ["err:decode", "err:decode", "err:decode", "ok"] and secs < 10, "JBIG2 region or pattern dictionary larger than the page (in the stream or its globals): refused before decoding", (got, secs))


def test_codec_sizes(d):
    # the codec's own size, not the dictionary's, decides: a 100x100 JPX whose codestream is
    # 16000x16000 (raw and in a JP2 box), a JBIG2 page of 5000x5000 for a 10x10 image, and a
    # CCITT stream of 100000 columns, are err:pixels without decoding
    siz = b"\xff\x4f\xff\x51\x00\x29\x00\x00" + b"".join(v.to_bytes(4, "big") for v in (16000, 16000, 0, 0, 16000, 16000, 0, 0)) + b"\x00\x01\x07\x01\x01"
    j2k = siz + b"\xff\x90junk"
    box = lambda kind, body: (len(body) + 8).to_bytes(4, "big") + kind + body
    jp2 = box(b"jP  ", b"\r\n\x87\n") + box(b"ftyp", b"jp2 \0\0\0\0jp2 ") + box(b"jp2c", j2k)
    page_info = (0).to_bytes(4, "big") + b"\x30\x00\x01" + (19).to_bytes(4, "big") + (5000).to_bytes(4, "big") + (5000).to_bytes(4, "big") + bytes(8) + b"\x00\x00\x00"
    objs = [
        image("/Width 100 /Height 100 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /JPXDecode", j2k),  # 6
        image("/Width 100 /Height 100 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /JPXDecode", jp2),  # 7
        image("/Width 10 /Height 10 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode", page_info),  # 8
        image("/Width 64 /Height 40 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 100000 /Rows 40 >>", G4),  # 9
        # a JPX under a Flate filter: never decoded unchecked
        image("/Width 100 /Height 100 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter [/JPXDecode /FlateDecode]", zlib.compress(j2k)),  # 10
    ]
    # a DCT stream to decode (/Decode [1 0]) whose progressive frame says 60000 x 60000: the
    # render decodes something else (slowly) than a 100 x 100 copy would
    sof = GRAY_JPEG.index(b"\xff\xc0")
    huge = GRAY_JPEG[: sof + 5] + (60000).to_bytes(2, "big") * 2 + GRAY_JPEG[sof + 9 :]
    objs.append(image("/Width 100 /Height 100 /ColorSpace /DeviceGray /BitsPerComponent 8 /Decode [1 0] /Filter /DCTDecode", huge))  # 11
    pdf = pages_pdf(one_each(["I%d" % n for n in range(6, 12)]), "<< /XObject << %s >> >>" % " ".join("/I%d %d 0 R" % (n, n) for n in range(6, 12)), objs)
    path = d.write("codecs.pdf", pdf)
    rc, r, err = copy(path, [("p%d.1" % (n - 5), n - 5, "%d.0" % n) for n in range(6, 12)])
    t.check(rc == 0, "codec size checks: exit 0", err)
    got = {"p1.%d" % int(k[1:].split(".")[0]): v[0] for k, v in r.items()}
    t.check(got.get("p1.1") == "err:pixels" and got.get("p1.2") == "err:pixels", "JPX codestream larger than /Width x /Height: err:pixels", got)
    t.check(got.get("p1.3") == "err:pixels", "JBIG2 page larger than the image: err:pixels", got)
    t.check(got.get("p1.4") == "err:pixels", "CCITT /Columns not /Width: err:pixels", got)
    t.check(got.get("p1.5") in ("err:copy", "err:decode"), "a codec under another filter is not decoded", got)
    t.check(got.get("p1.6") == "err:pixels", "JPEG frame larger than /Width x /Height (decoded, not copied): err:pixels", got)


def mh_bits(bits):
    out = bytearray()
    for k in range(0, len(bits), 8):
        out.append(int(bits[k : k + 8].ljust(8, "0"), 2))
    return bytes(out)


def test_ccitt_black(d):
    # Modified Huffman (/K 0), 16 x 2: row 1 white 4, black 8, white 4; row 2 white 0, black 16.
    # gray1 rows are 0 black: F0 0F, then 00 00
    mh = mh_bits("1011" "000101" "1011" + "00110101" "0000010111")
    objs = [image("/Width 16 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K 0 /Columns 16 /Rows 2 >>", mh)]
    pdf = t.simple_page_pdf(b"q 10 0 0 10 0 0 cm /A Do Q", resources="<< /XObject << /A 6 0 R >> >>", extra_objects=objs)
    rc, r, _ = copy(d.write("ccitt-black.pdf", pdf), [("p1.1", 1, "6.0")])
    t.check(r["p1.1"][:4] == ("ok", "gray1", 16, 2) and r["p1.1"][4] == b"\xf0\x0f\x00\x00", "CCITT black runs: 0 is black in gray1", r["p1.1"])
    # /BlackIs1 true reads the same bits the other way round
    objs = [image("/Width 16 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K 0 /Columns 16 /Rows 2 /BlackIs1 true >>", mh)]
    pdf = t.simple_page_pdf(b"q 10 0 0 10 0 0 cm /A Do Q", resources="<< /XObject << /A 6 0 R >> >>", extra_objects=objs)
    rc, r, _ = copy(d.write("ccitt-black1.pdf", pdf), [("p1.1", 1, "6.0")])
    t.check(r["p1.1"][4] == b"\x0f\xf0\xff\xff", "CCITT /BlackIs1: inverted", r["p1.1"])


def test_reachable_shallow(d):
    # form F (holding form G, holding the image) is named first at the end of a deep chain
    # A -> A2 -> A3 -> F, where G is past the search, and then by form B on the page: the
    # search reaches F at its shallow depth through B and finds the image
    form = lambda target: t.stream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /X %d 0 R >> >>" % target, "/X Do")
    objs = [
        image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode", GRAY_JPEG),  # 6
        form(6),  # 7: G
        form(7),  # 8: F
        form(8),  # 9: A3
        form(9),  # 10: A2
        form(10),  # 11: A
        form(8),  # 12: B
    ]
    # (A is named first in the resources, so the search meets F deep first; only B is drawn, as
    # the copy allow-list wants the image drawn once)
    pdf = t.simple_page_pdf(b"/B Do", resources="<< /XObject << /A 11 0 R /B 12 0 R >> >>", extra_objects=objs)
    rc, r, _ = copy(d.write("shallow.pdf", pdf), [("p1.1", 1, "6.0")])
    t.check(r["p1.1"][0] == "ok", "a form reached deep first and shallow later is searched at its shallow depth", r["p1.1"][:4])


def pgm_pixels(data):
    """The samples of a binary PGM (P5, maxval 255): (width, height, bytes)."""
    fields, i = [], 0
    while len(fields) < 4:
        while data[i : i + 1].isspace():
            i += 1
        j = i
        while not data[j : j + 1].isspace():
            j += 1
        fields.append(data[i:j])
        i = j
    w, h = int(fields[1]), int(fields[2])
    return w, h, data[i + 1 : i + 1 + w * h]


def test_jpx_dictionary_bits(d):
    # A JPEG 2000 stream decodes to 8 bits a component (JPXStream::getImageParams), and Splash
    # draws it so whatever the dictionary's /BitsPerComponent says. With /BitsPerComponent 1 it
    # must not be classed bilevel and copied as 1-bit rows packed from those 8-bit bytes: every
    # byte here is 0xF0 | (a bit of hidden text), near white when rendered, while the rows packed
    # from the same bytes would show the text. If copied at all, the copy is the Decode path's,
    # and its pixels are the render's at native size.
    try:
        from PIL import Image, features
        import io

        if not features.check("jpg_2000"):
            raise ImportError
    except ImportError:
        print("SKIP test_jpx_dictionary_bits: Pillow without JPEG 2000")
        return
    w, h = 64, 48
    hidden = bytes((0x0F if (x // 8 + y // 6) % 2 else 0x00) for y in range(h) for x in range(w))
    samples = bytes(0xF0 | v for v in hidden)
    out = io.BytesIO()
    Image.frombytes("L", (w, h), samples).save(out, "JPEG2000", irreversible=False, no_jp2=True)
    j2k = out.getvalue()
    for bpc in ("/BitsPerComponent 1", "/BitsPerComponent 8", ""):
        name = "jpx-bpc%s.pdf" % (bpc.split()[-1] if bpc else "-none")
        obj = image("/Width %d /Height %d /ColorSpace /DeviceGray %s /Filter /JPXDecode" % (w, h, bpc), j2k)
        pdf = t.simple_page_pdf(b"q %d 0 0 %d 0 0 cm /A Do Q" % (w, h), resources="<< /XObject << /A 6 0 R >> >>", mediabox=(0, 0, w, h), extra_objects=[obj])
        path = d.write(name, pdf)
        _, images = attributes(path)
        t.check(len(images) == 1 and ' copy="bilevel"' not in images[0], "%s: a JPX stream is never copy=bilevel" % name, images)
        rc, r, _ = copy(path, [("p1.1", 1, "6.0")])
        status, kind, cw, ch, payload = r["p1.1"]
        t.check(kind != "gray1", "%s: never copied as 1-bit rows" % name, r["p1.1"][:4])
        if status != "ok":
            continue
        t.check((kind, cw, ch) == ("gray8", w, h), "%s: copied through the Decode path (gray8, native size)" % name, r["p1.1"][:4])
        rc, pgm, _ = run("pdftoppm", "-r", "72", "-gray", path)
        rw, rh, rendered = pgm_pixels(pgm)
        diff = sum(abs(a - b) for a, b in zip(payload, rendered)) / max(1, len(rendered))
        t.check((rw, rh) == (w, h) and len(payload) == len(rendered) and diff < 2, "%s: the copy is the render (mean abs diff %.2f)" % (name, diff), (rw, rh, len(payload)))


def test_rebuilt_xref(d):
    # A cross-reference entry that points at the wrong object makes poppler rebuild the table
    # from a scan of the file when it is fetched, and the rebuilt table resolves image 6 to the
    # file's later body of it (B) where the table pointed at the first (A). The page audit
    # fetches the broken entry (the content stream), so a copy fetched before the audit would
    # be body A while the page draws body B. A document whose table was rebuilt is never
    # copyable: pdftotext says xrefRebuilt="1" (and <xref rebuilt="1"/>) and no copyable="1";
    # pdfregions answers err:copy.
    body = lambda which: image("/Width 160 /Height 200 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode /Note (%s)" % which, GRAY_JPEG)
    objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /S 6 0 R >> >> /Contents 5 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        t.stream("", b"q 612 0 0 792 0 0 cm /S Do Q\n"),
        body("A"),
    ]
    for broken in (False, True):
        o = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offs = []
        for n, b in enumerate(objs, 1):
            offs.append(len(o))
            o += b"%d 0 obj\n" % n + b + b"\nendobj\n"
        o += b"6 0 obj\n" + body("B") + b"\nendobj\n"  # a second body of object 6
        xref = len(o)
        o += b"xref\n0 7\n0000000000 65535 f \n"
        for n, off in enumerate(offs, 1):
            o += b"%010d 00000 n \n" % (offs[3] if broken and n == 5 else off)
        o += b"trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % xref
        name = "rebuilt.pdf" if broken else "rebuilt-control.pdf"
        path = d.write(name, bytes(o))
        rc, out, _ = run("pdftotext", "-bbox-layout", "-images", path, "-")
        text = out.decode()
        images = re.findall(r"<image [^>]*/>", text)
        if broken:
            t.check(' xrefRebuilt="1"' in text and '<xref rebuilt="1"/>' in text, "%s: pdftotext says the table was rebuilt" % name, text[:400])
            t.check(len(images) == 1 and ' copyable="1"' not in images[0], "%s: not copyable" % name, images)
        else:
            t.check("xrefRebuilt" not in text and "<xref" not in text, "%s: no rebuilt marker" % name, text[:400])
            t.check(len(images) == 1 and ' copyable="1"' in images[0], "%s: copyable" % name, images)
        rc, r, _ = copy(path, [("p1.1", 1, "6.0")])
        want = "err:copy" if broken else "ok"
        t.check(r["p1.1"][0] == want, "%s: copy job %s" % (name, want), r["p1.1"][:4])


with t.TempDir() as d:
    path = d.write("copy.pdf", copy_pdf())
    test_copies(path)
    test_refusals(path)
    test_limits(path)
    test_bad_job_lines(path)
    test_deep_forms(d)
    test_attributes(d)
    test_overlaid(d)
    test_jpeg_metadata(d)
    test_jpeg_colours(d)
    test_codec_sizes(d)
    test_color_spaces(d)
    test_jpeg_structure(d)
    test_jbig2_regions(d)
    test_ccitt_black(d)
    test_reachable_shallow(d)
    test_jpx_dictionary_bits(d)
    test_rebuilt_xref(d)
t.finish()
