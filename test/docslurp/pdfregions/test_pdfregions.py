"""pdfregions renders page regions for pdftomarkdown (docslurp/extraction/PROTOCOL.md).

Covers record framing for raster and svg jobs, the error records, strict job
parsing (exit 2), the limits (pixels, timeout, memory), passwords, rotated
pages, coordinate agreement with pdftotext -images and pdftoppm crops, and
the ref attribute that pdftotext -images adds for image XObjects.
"""

import hashlib
import os
import re
import subprocess
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402

OK, OPEN_ERROR, USAGE, LIMIT = 0, 1, 2, 3
MARKER = "ZZDOCMARKER"

utils = t.Utils.from_args()


def run(util, *args, stdin=b"", timeout=120, stdout=subprocess.PIPE):
    cmd = [os.path.join(utils.bindir, util)] + [str(a) for a in args]
    proc = subprocess.run(cmd, input=stdin, stdout=stdout, stderr=subprocess.PIPE, timeout=timeout)
    return proc.returncode, proc.stdout, proc.stderr


def regions(path, jobs, *args, timeout=120):
    data = "".join("\t".join(str(f) for f in job) + "\n" for job in jobs).encode()
    return run("pdfregions", *args, path, stdin=data, timeout=timeout)


def parse_records(out, strict=True):
    """Records as (header fields, payload); fails on a truncated record."""
    recs = []
    i = 0
    while i < len(out):
        j = out.index(b"\n", i)
        head = out[i:j].decode("ascii").split("\t")
        assert len(head) == 6, head
        n = int(head[5])
        if strict:
            assert j + 1 + n <= len(out), "truncated record"
        recs.append((head, out[j + 1 : j + 1 + n]))
        i = j + 1 + n
    return recs


def image_xobject(rgb=b"\xff\x00\x00"):
    return t.stream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8", rgb * 4)


# A red image XObject at user (100, 600)-(244, 672), a blue square far from it,
# an inline green image, and document strings carrying the marker.
CONTENT = b"q 144 0 0 72 100 600 cm /Im1 Do Q  0 0 1 rg 400 100 50 50 re f  q 20 0 0 20 300 300 cm BI /W 2 /H 2 /CS /RGB /BPC 8 ID " + b"\x00\xff\x00" * 4 + b" EI Q BT /F1 12 Tf 72 72 Td (" + MARKER.encode() + b") Tj ET"
RESOURCES = "<< /Font << /F1 4 0 R >> /XObject << /Im1 6 0 R >> >>"


def figure_pdf(page_extra=""):
    return t.simple_page_pdf(CONTENT, resources=RESOURCES, extra_objects=[image_xobject()], page_extra=page_extra)


def image_boxes(path):
    rc, out, _ = run("pdftotext", "-bbox-layout", "-images", path, "-")
    assert rc == 0
    return [
        (float(m.group(1)), float(m.group(2)), float(m.group(3)), float(m.group(4)), m.group(5))
        for m in re.finditer(rb'<image xMin="([-0-9.]+)" yMin="([-0-9.]+)" xMax="([-0-9.]+)" yMax="([-0-9.]+)" [^>]*?(?: ref="([0-9 ]+)")?/>', out)
    ]


def ppm_crop(path, page, dpi, x, y, w, h):
    rc, out, _ = run("pdftoppm", "-f", page, "-l", page, "-r", dpi, "-x", x, "-y", y, "-W", w, "-H", h, path)
    assert rc == 0
    magic, dims, maxval, pixels = out.split(b"\n", 3)
    return tuple(map(int, dims.split())), pixels


def check_raster_matches_ppm(path, label, box, dpi):
    import math

    x0, y0, x1, y1 = box
    rc, out, err = regions(path, [("p1.1", 1, "%.2f" % x0, "%.2f" % y0, "%.2f" % x1, "%.2f" % y1, "raster", dpi)])
    t.check(rc == OK, "%s: raster exit 0" % label, repr((rc, err)))
    (head, payload), = parse_records(out)
    s = dpi / 72.0
    px, py = math.floor(float("%.2f" % x0) * s + 1e-6), math.floor(float("%.2f" % y0) * s + 1e-6)
    w, h = int(head[3]), int(head[4])
    t.check(head[1:3] == ["ok", "rgb8"] and len(payload) == w * h * 3, "%s: rgb8 record framing" % label, repr(head))
    dims, pixels = ppm_crop(path, 1, dpi, px, py, w, h)
    t.check(dims == (w, h) and pixels == payload, "%s: pixels identical to pdftoppm -x -y -W -H" % label, repr((dims, (w, h))))
    return w, h, payload


def crop_size(box, dpi):
    import math

    s = dpi / 72.0
    # half away from zero, as C llround and Rust f64::round (Python's round is half-even)
    return (max(1, math.floor((box[2] - box[0]) * s + 0.5)), max(1, math.floor((box[3] - box[1]) * s + 0.5)))


def mostly(payload, rgb, share=0.9):
    px = [payload[i : i + 3] for i in range(0, len(payload), 3)]
    return sum(1 for p in px if abs(p[0] - rgb[0]) < 40 and abs(p[1] - rgb[1]) < 40 and abs(p[2] - rgb[2]) < 40) >= share * len(px)


with t.TempDir() as d:
    path = d.write("figure.pdf", figure_pdf())

    # --- pdftotext -images: ref for XObjects, none for inline images ---------
    boxes = image_boxes(path)
    t.check(len(boxes) == 2, "pdftotext -images: two images", repr(boxes))
    xobj = [b for b in boxes if b[4]]
    t.check(len(xobj) == 1 and xobj[0][4] == b"6 0", 'pdftotext -images: ref="6 0" on the XObject only', repr(boxes))
    rc, plain, _ = run("pdftotext", "-bbox-layout", path, "-")
    t.check(b"<image" not in plain and b"ref=" not in plain, "pdftotext without -images: no <image>")
    box = xobj[0][:4]
    t.check(box == (100.0, 120.0, 244.0, 192.0), "pdftotext -images: XObject box", repr(box))

    # --- raster: framing, pixels, agreement with pdftoppm ---------------------
    for dpi in (72, 150, 300):
        w, h, payload = check_raster_matches_ppm(path, "upright %d dpi" % dpi, box, dpi)
        t.check((w, h) == crop_size(box, dpi), "upright %d dpi: crop size is round(region size at dpi)" % dpi, repr((w, h)))
        t.check(mostly(payload, (255, 0, 0), 0.99), "upright %d dpi: crop is the red figure" % dpi)

    # --- svg: root, viewBox, clipped content -----------------------------------
    rc, out, err = regions(path, [("p1.1", 1, "100.00", "120.00", "244.00", "192.00", "svg", 0), ("p1.2", 1, "100", "120", "244", "192", "raster", 72)])
    recs = parse_records(out)
    t.check(rc == OK and len(recs) == 2, "svg + raster on one page: two records", repr((rc, err)))
    head, svg = recs[0]
    t.check(head == ["p1.1", "ok", "svg", "0", "0", str(len(svg))], "svg record header", repr(head))
    root = re.search(rb"<svg[^>]*>", svg).group(0)
    t.check(b'width="144pt"' in root and b'height="72pt"' in root and b'viewBox="100 120 144 72"' in root, "svg root: size in pt, viewBox = region", root)
    t.check(svg.rstrip().endswith(b"</svg>") and svg.count(b"<svg") == 1, "svg is one complete document")
    t.check(b"0%, 0%, 100%" not in svg and b"BT" not in svg, "svg: content outside the region is dropped (no blue square)")
    t.check(recs[1][0][1] == "ok" and mostly(recs[1][1], (255, 0, 0), 0.99), "raster after svg on the same page")
    rsvg = subprocess.run(["rsvg-convert", "-d", "72", "-p", "72"], input=svg, capture_output=True) if os.path.exists("/usr/bin/rsvg-convert") else None
    if rsvg is not None:
        t.check(rsvg.returncode == 0 and rsvg.stdout.startswith(b"\x89PNG"), "svg renders with rsvg-convert", rsvg.stderr)

    # --- error records -----------------------------------------------------------
    rc, out, err = regions(path, [("p1.1", 1, 100, 120, 244, 192, "raster", 600), ("p1.2", 1, 9000, 9000, 9010, 9010, "raster", 150), ("p1.3", 1, -50, -50, -1, -1, "svg", 0), ("p2.1", 2, 0, 0, 10, 10, "raster", 150), ("p7.g1", 7, 0, 0, 10, 10, "svg", 0)], "-max-pixels", 100000)
    recs = parse_records(out)
    t.check(rc == OK, "error records: exit 0", repr((rc, err)))
    want = [("p1.1", "err:pixels"), ("p1.2", "err:empty"), ("p1.3", "err:empty"), ("p2.1", "err:page"), ("p7.g1", "err:page")]
    t.check([(h[0], h[1]) for h, _ in recs] == want, "error codes in job order", repr([h for h, _ in recs]))
    t.check(all(h[2:] == ["-", "0", "0", "0"] and p == b"" for h, p in recs), "error records: kind -, zero sizes, no payload")
    area = crop_size(box, 300)[0] * crop_size(box, 300)[1]
    t.check(area == 600 * 300, "300 dpi crop is 600 x 300 pixels", repr(area))
    rc, out, _ = regions(path, [("p1.1", 1, 100, 120, 244, 192, "raster", 300)], "-max-pixels", area - 1)
    t.check(parse_records(out)[0][0][1] == "err:pixels", "-max-pixels: one pixel over is err:pixels")
    rc, out, _ = regions(path, [("p1.1", 1, 100, 120, 244, 192, "raster", 300)], "-max-pixels", area)
    t.check(parse_records(out)[0][0][1] == "ok", "-max-pixels: exactly at the limit is ok")
    # a region overhanging the page is cut to the page
    rc, out, _ = regions(path, [("p1.1", 1, -20, -20, 10.5, 10, "raster", 72), ("p1.2", 1, 600, 780, 700, 900, "svg", 0)])
    recs = parse_records(out)
    t.check(recs[0][0][1:5] == ["ok", "rgb8", "11", "10"], "overhanging raster is cut to the page", repr(recs[0][0]))
    t.check(b'viewBox="600 780 12 12"' in recs[1][1], "overhanging svg viewBox is cut to the page", recs[1][1][:300])

    # a huge page: offsets beyond an int and crops beyond -max-pixels stay error records
    big = d.write("big.pdf", t.simple_page_pdf(b"1 0 0 rg 0 0 100 100 re f", mediabox=(0, 0, 900000000, 900000000)))
    rc, out, err = regions(big, [("p1.1", 1, 0, 0, 999999999, 999999999, "raster", 600), ("p1.2", 1, 899999000, 899999000, 899999010, 899999010, "raster", 600), ("p1.3", 1, 0, 899999900, 50, 899999950, "raster", 72), ("p1.4", 1, 0, 0, 999999999, 10, "svg", 0)])
    recs = parse_records(out)
    t.check(rc == OK and [h[1] for h, _ in recs][:3] == ["err:pixels", "err:render", "ok"] and len(recs) == 4, "huge page: error records, no crash", repr((rc, [h for h, _ in recs], err)))

    # --- malformed jobs: exit 2, nothing on stdout ------------------------------
    good = "p1.1\t1\t1\t2\t3\t4\traster\t150\n"
    bad_inputs = {
        "bad id": "x1.1\t1\t1\t2\t3\t4\traster\t150\n",
        "id too long": "p1234567890.1\t1\t1\t2\t3\t4\traster\t150\n",
        "id with g twice": "p1.gg1\t1\t1\t2\t3\t4\traster\t150\n",
        "page 0": "p1.1\t0\t1\t2\t3\t4\traster\t150\n",
        "page sign": "p1.1\t+1\t1\t2\t3\t4\traster\t150\n",
        "nan": "p1.1\t1\tnan\t2\t3\t4\traster\t150\n",
        "inf": "p1.1\t1\t1\tinf\t3\t4\traster\t150\n",
        "exponent": "p1.1\t1\t1e1\t2\t30\t4\traster\t150\n",
        "hex": "p1.1\t1\t0x1\t2\t3\t4\traster\t150\n",
        "empty number": "p1.1\t1\t\t2\t3\t4\traster\t150\n",
        "trailing dot": "p1.1\t1\t1.\t2\t3\t4\traster\t150\n",
        "x1 == x0": "p1.1\t1\t3\t2\t3\t4\traster\t150\n",
        "y1 < y0": "p1.1\t1\t1\t5\t3\t4\traster\t150\n",
        "bad mode": "p1.1\t1\t1\t2\t3\t4\tpng\t150\n",
        "dpi too high": "p1.1\t1\t1\t2\t3\t4\traster\t601\n",
        "dpi too low": "p1.1\t1\t1\t2\t3\t4\traster\t35\n",
        "seven fields": "p1.1\t1\t1\t2\t3\t4\traster\n",
        "nine fields": "p1.1\t1\t1\t2\t3\t4\traster\t150\tx\n",
        "spaces": "p1.1 1 1 2 3 4 raster 150\n",
        "crlf": "p1.1\t1\t1\t2\t3\t4\traster\t150\r\n",
        "blank line": good + "\n",
        "no final newline": good + good.rstrip("\n"),
        "long line": "p1.1\t1\t1\t2\t3\t4\traster\t150" + "0" * 300 + "\n",
        "nul": "p1.1\t1\t1\x00\t2\t3\t4\traster\t150\n",
        "unsorted pages": "p2.1\t2\t1\t2\t3\t4\traster\t150\n" + good,
        "5001 jobs": good * 5001,
    }
    for label, data in bad_inputs.items():
        rc, out, err = run("pdfregions", path, stdin=data.encode())
        t.check(rc == USAGE and out == b"", "malformed (%s): exit 2, empty stdout" % label, repr((rc, out[:80], err)))
        t.check(err.startswith(b"pdfregions: ") and err.count(b"\n") == 1, "malformed (%s): one fixed diagnostic" % label, repr(err))
    rc, out, _ = run("pdfregions", path, stdin=("p1.1\t2\t1\t2\t3\t4\tsvg\t0\n" * 5000).encode())
    t.check(rc == OK and len(parse_records(out)) == 5000, "5000 jobs: accepted", repr(rc))
    rc, out, _ = run("pdfregions", path, stdin=b"")
    t.check(rc == OK and out == b"", "no jobs: exit 0, empty stdout")
    rc, out, _ = run("pdfregions", path, stdin=b"p1.1\t1\t1.5\t-2.25\t3.000000001\t4\tsvg\t9999\n")
    t.check(rc == OK and parse_records(out)[0][0][1] == "ok", "svg ignores dpi; decimals and negatives accepted", repr(out[:80]))

    # --- usage ---------------------------------------------------------------------
    for args in ((), (path, path), ("-max-pixels", 0, path), ("-max-memory", -1, path), ("-timeout", "1x", path), ("-nope", path)):
        rc, out, _ = run("pdfregions", *args, stdin=good.encode())
        t.check(rc == USAGE and out == b"", "usage %r: exit 2" % (args,), repr(rc))
    rc, out, err = run("pdfregions", os.path.join(d.path, "missing.pdf"), stdin=good.encode())
    t.check(rc == OPEN_ERROR and out == b"", "missing file: exit 1", repr((rc, err)))
    notpdf = d.write("not.pdf", b"hello " + MARKER.encode())
    rc, out, err = run("pdfregions", notpdf, stdin=good.encode())
    t.check(rc == OPEN_ERROR and out == b"" and MARKER.encode() not in err, "not a PDF: exit 1, no document data on stderr", repr((rc, err)))

    # --- rotated pages: crops follow pdftotext's rotated coordinates ------------
    for rot in (90, 180, 270):
        rpath = d.write("rot%d.pdf" % rot, figure_pdf("/Rotate %d" % rot))
        rboxes = [b for b in image_boxes(rpath) if b[4]]
        t.check(len(rboxes) == 1, "rotate %d: one XObject box" % rot, repr(rboxes))
        rbox = rboxes[0][:4]
        w, h, payload = check_raster_matches_ppm(rpath, "rotate %d" % rot, rbox, 150)
        t.check(mostly(payload, (255, 0, 0), 0.99), "rotate %d: crop is the red figure" % rot)
        t.check((w > h) == (rot == 180), "rotate %d: crop orientation follows the page" % rot, repr((w, h)))
        rc, out, _ = regions(rpath, [("p1.1", 1, *("%.2f" % v for v in rbox), "svg", 0)])
        head, svg = parse_records(out)[0]
        want = 'viewBox="%g %g %g %g"' % (rbox[0], rbox[1], rbox[2] - rbox[0], rbox[3] - rbox[1])
        t.check(want.encode() in svg, "rotate %d: svg viewBox = pdftotext box" % rot, svg[:300])
        if rsvg is not None:
            png = subprocess.run(["rsvg-convert", "-d", "72", "-p", "72", "-f", "png"], input=svg, capture_output=True).stdout
            t.check(png.startswith(b"\x89PNG"), "rotate %d: svg renders" % rot)

    # --- passwords -----------------------------------------------------------------
    PAD = bytes.fromhex("28bf4e5e4e758a4164004e56fffa01082e2e00b6d0683e802f0ca9fe6453697a")

    def rc4(key, data):
        s = list(range(256))
        j = 0
        for i in range(256):
            j = (j + s[i] + key[i % len(key)]) % 256
            s[i], s[j] = s[j], s[i]
        i = j = 0
        out = bytearray()
        for c in data:
            i = (i + 1) % 256
            j = (j + s[i]) % 256
            s[i], s[j] = s[j], s[i]
            out.append(c ^ s[(s[i] + s[j]) % 256])
        return bytes(out)

    def encrypted_pdf(user_pw, owner_pw):
        """One page drawing a black rectangle; the content stream is RC4 encrypted (R2)."""
        padded = lambda pw: (pw + PAD)[:32]  # noqa: E731
        file_id = bytes(range(16))
        perms = -4
        o_value = rc4(hashlib.md5(padded(owner_pw)).digest()[:5], padded(user_pw))
        key = hashlib.md5(padded(user_pw) + o_value + (perms & 0xFFFFFFFF).to_bytes(4, "little") + file_id).digest()[:5]
        u_value = rc4(key, PAD)
        content = b"0 0 0 rg 100 100 200 200 re f"
        obj_key = hashlib.md5(key + (4).to_bytes(3, "little") + (0).to_bytes(2, "little")).digest()[:10]
        objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>",
            t.stream("", rc4(obj_key, content)),
            "<< /Filter /Standard /V 1 /R 2 /O <%s> /U <%s> /P %d >>" % (o_value.hex(), u_value.hex(), perms),
        ]
        return t.build_pdf(objects, trailer_extra="/Encrypt 5 0 R /ID [<%s> <%s>]" % (file_id.hex(), file_id.hex()))

    epath = d.write("encrypted.pdf", encrypted_pdf(b"secret", b"owner"))
    job = [("p1.1", 1, 150, 542, 250, 642, "raster", 72)]
    rc, out, err = regions(epath, job)
    t.check(rc == OPEN_ERROR and out == b"" and err == b"pdfregions: cannot decrypt document\n", "encrypted without password: exit 1", repr((rc, err)))
    rc, out, err = regions(epath, job, "-upw", "wrong")
    t.check(rc == OPEN_ERROR and out == b"", "encrypted with a wrong password: exit 1", repr((rc, err)))
    for args in (("-upw", "secret"), ("-opw", "owner")):
        rc, out, err = regions(epath, job, *args)
        recs = parse_records(out)
        t.check(rc == OK and recs[0][0][1] == "ok" and mostly(recs[0][1], (0, 0, 0), 0.99), "encrypted with %s: decrypts and renders" % args[0], repr((rc, err)))

    # --- limits: timeout and memory --------------------------------------------------
    heavy = b"".join(b"%d %d m %d %d %d %d %d %d c S\n" % (i % 600, i % 780, i % 600 + 5, i % 780 + 9, i % 600 + 9, i % 780 + 2, i % 600 + 12, i % 780 + 6) for i in range(40000))
    hpath = d.write("heavy.pdf", t.simple_page_pdf(heavy))
    rc, out, err = regions(hpath, [("p1.%d" % i, 1, 0, 0, 100, 100, "svg", 0) for i in range(5000)], "-timeout", 1, timeout=60)
    recs = parse_records(out)
    t.check(rc == LIMIT and err == b"pdfregions: timeout\n", "timeout: exit 3 with a fixed diagnostic", repr((rc, err)))
    t.check(0 < len(recs) < 5000 and all(h[1] == "ok" and p.rstrip().endswith(b"</svg>") for h, p in recs), "timeout: records already written are complete", repr(len(recs)))

    # a full page at 600 dpi needs ~130 MB of bitmap: under a 64 MiB cap it fails cleanly
    rc, out, err = regions(path, [("p1.1", 1, 0, 0, 612, 792, "raster", 600), ("p1.2", 1, 100, 120, 244, 192, "raster", 72)], "-max-memory", 64, "-max-pixels", 50000000)
    t.check(rc in (OK, LIMIT) and rc >= 0, "-max-memory 64: exit 0 or 3, no crash", repr((rc, err)))
    if rc == OK:
        recs = parse_records(out)
        t.check([h[1] for h, _ in recs] == ["err:render", "ok"], "-max-memory 64: the oversized crop is err:render, the next job renders", repr([h for h, _ in recs]))
    else:
        t.check(err == b"pdfregions: out of memory\n", "-max-memory 64: fixed diagnostic", repr(err))
    rc, out, _ = regions(path, [("p1.2", 1, 100, 120, 244, 192, "raster", 72)], "-max-memory", 0)
    t.check(rc == OK, "-max-memory 0: no limit")

    # --- output failure ------------------------------------------------------------------
    with open("/dev/full", "wb") as full:
        rc = subprocess.run([os.path.join(utils.bindir, "pdfregions"), path], input=good.encode(), stdout=full, stderr=subprocess.PIPE, timeout=60).returncode
    t.check(rc == OPEN_ERROR, "stdout write failure: exit 1", repr(rc))

t.finish()
