"""Text inside an ExtGState /SMask group must not reach text extraction.

A soft mask group is a content stream whose output is only ever used as an
alpha/luminosity mask.  Gfx used to interpret it for every device, so
pdftotext printed the mask's own text next to the visible text.
"""

import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(here)))
sys.path.insert(0, here)
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()

GROUP = "/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Group << /S /Transparency /CS /DeviceGray >> /Resources << /Font << /F1 4 0 R >> >>"
MASK_TEXT = b"1 g 0 0 306 792 re f 0 g BT /F1 12 Tf 72 500 Td (MASKONLY) Tj ET"

with t.TempDir() as d:
    # objects: 6 = ExtGState, 7 = mask group
    gs = "<< /Type /ExtGState /SMask << /Type /Mask /S /Luminosity /G 7 0 R >> >>"
    mask = t.stream(GROUP, MASK_TEXT)
    resources = "<< /Font << /F1 4 0 R >> /ExtGState << /GS1 6 0 R >> >>"
    content = b"BT /F1 12 Tf 72 700 Td (VISIBLE1) Tj ET /GS1 gs BT /F1 12 Tf 72 650 Td (VISIBLE2) Tj ET"
    path = d.write("smask.pdf", t.simple_page_pdf(content, resources=resources, extra_objects=[gs, mask]))
    for mode in ([], ["-raw"], ["-layout"]):
        out = utils.run("pdftotext", *mode, path, "-")
        what = "pdftotext %s" % " ".join(mode)
        t.check("MASKONLY" not in out, "%s: mask group text is not extracted" % what, repr(out))
        t.check("VISIBLE1" in out and "VISIBLE2" in out, "%s: visible text is extracted" % what, repr(out))

    # Mask applied from inside a form XObject.
    form = t.stream("/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /ExtGState << /GS1 6 0 R >> >>", b"/GS1 gs BT /F1 12 Tf 72 600 Td (INFORM) Tj ET")
    page = t.simple_page_pdf(b"/Fm0 Do", resources="<< /Font << /F1 4 0 R >> /XObject << /Fm0 8 0 R >> >>", extra_objects=[gs, mask, form])
    path = d.write("smask-form.pdf", page)
    out = utils.run("pdftotext", path, "-")
    t.check("MASKONLY" not in out and "INFORM" in out, "mask group text inside a form is not extracted", repr(out))

    # Rendering must still honour the soft mask: red square, left half visible
    # through a luminosity mask, right half hidden.
    mask = t.stream("/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Group << /S /Transparency /CS /DeviceGray >>", b"1 g 0 0 50 100 re f")
    page = t.simple_page_pdf(b"/GS1 gs 1 0 0 rg 0 0 100 100 re f", resources="<< /ExtGState << /GS1 6 0 R >> >>", extra_objects=[gs, mask], mediabox=(0, 0, 100, 100))
    path = d.write("render.pdf", page)
    utils.run("pdftoppm", "-r", "72", "-singlefile", path, os.path.join(d.path, "out"))
    with open(os.path.join(d.path, "out.ppm"), "rb") as f:
        data = f.read()
    header = b"P6\n100 100\n255\n"
    t.check(data.startswith(header), "pdftoppm writes a 100x100 PPM", data[:20])

    def px(x, y):
        o = len(header) + 3 * (y * 100 + x)
        return tuple(data[o : o + 3])

    t.check(px(20, 50) == (255, 0, 0), "unmasked half is red", px(20, 50))
    t.check(px(80, 50) == (255, 255, 255), "masked half stays white", px(80, 50))
t.finish()
