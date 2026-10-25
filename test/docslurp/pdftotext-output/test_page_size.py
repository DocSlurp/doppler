"""-bbox, -bbox-layout and -tsv must report the page size in the same
coordinate space as the word boxes (rotated, scaled, media/crop box)."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t, xml_pages, tsv_rows  # noqa: E402

utils = t.Utils.from_args()
CONTENT = b"BT /F1 24 Tf 100 320 Td (Top left) Tj ET BT /F1 24 Tf 250 80 Td (Bottom right) Tj ET"
TOL = 0.01


def sizes(path, mode, *extra):
    """Return (width, height, [(xMin, yMin, xMax, yMax), ...]) of page 1 for an output mode."""
    out = utils.run("pdftotext", *extra, mode, path, "-")
    if mode == "-tsv":
        rows = tsv_rows(out)
        page = [r for r in rows if r[0] == "1"][0]
        boxes = [(float(r[6]), float(r[7]), float(r[6]) + float(r[8]), float(r[7]) + float(r[9])) for r in rows if r[0] == "5"]
        return float(page[8]), float(page[9]), boxes
    w, h, words = xml_pages(out)[0]
    return w, h, [x[:4] for x in words]


def check_case(name, pdf, expect_w, expect_h, *extra):
    with t.TempDir() as d:
        path = d.write("p.pdf", pdf)
        for mode in ("-bbox", "-bbox-layout", "-tsv"):
            w, h, boxes = sizes(path, mode, *extra)
            label = "%s %s %s" % (name, mode, " ".join(extra))
            t.check(abs(w - expect_w) < TOL and abs(h - expect_h) < TOL, label + ": page size is %gx%g" % (expect_w, expect_h), "got %gx%g" % (w, h))
            t.check(len(boxes) >= 2, label + ": has words", repr(boxes))
            inside = all(b[0] >= -TOL and b[1] >= -TOL and b[2] <= w + TOL and b[3] <= h + TOL for b in boxes)
            t.check(inside, label + ": every word box lies inside the reported page", "page %gx%g boxes %r" % (w, h, boxes))


def pdf(rotate=0, page_extra=""):
    return t.simple_page_pdf(CONTENT, page_extra="/Rotate %d %s" % (rotate, page_extra))


check_case("rotate0", pdf(0), 612, 792)
check_case("rotate90", pdf(90), 792, 612)
check_case("rotate180", pdf(180), 612, 792)
check_case("rotate270", pdf(270), 792, 612)
check_case("rotate90 -r 144", pdf(90), 1584, 1224, "-r", "144")

# crop box: sized differently from the media box, with and without rotation
crop = "/CropBox [50 50 450 350]"
check_case("cropbox", pdf(0, crop), 400, 300, "-cropbox")
check_case("cropbox rotate90", pdf(90, crop), 300, 400, "-cropbox")
# without -cropbox the media box is the reference even if a crop box exists
check_case("mediabox despite cropbox", pdf(90, crop), 792, 612)
t.finish()
