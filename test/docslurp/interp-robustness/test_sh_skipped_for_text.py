"""The `sh` operator must not evaluate shadings for text-only devices.

pdftotext ignores everything but text, yet Gfx::opShFill() used to build the
whole shading (a Coons patch mesh is subdivided into up to 4^6 triangles per
patch) before handing it to a device that discards it.  Repeating `sh` on a
mesh makes that a cheap way to burn CPU.
"""

import os
import random
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(here)))
sys.path.insert(0, here)
import docslurp_testlib as t  # noqa: E402
import ir_helpers as h  # noqa: E402

utils = t.Utils.from_args()
pdftotext = os.path.join(utils.bindir, "pdftotext")


def coons_mesh(npatch):
    rnd = random.Random(1)
    data = bytearray()
    for _ in range(npatch):
        data.append(0)  # edge flag
        for _ in range(12):  # 12 control points, x and y
            data += bytes([rnd.randrange(256), rnd.randrange(256)])
        data += bytes([0, 255, 0, 255])  # one gray component per corner
    return bytes(data)


mesh = t.stream("/ShadingType 6 /ColorSpace /DeviceGray /BitsPerCoordinate 8 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [0 600 0 780 0 1]", coons_mesh(20))
resources = "<< /Font << /F1 4 0 R >> /Shading << /Sh0 6 0 R >> >>"

with t.TempDir() as d:
    content = b"BT /F1 12 Tf 72 700 Td (BEFORE) Tj ET\n" + b"/Sh0 sh\n" * 400 + b"BT /F1 12 Tf 72 650 Td (AFTER) Tj ET"
    path = d.write("mesh.pdf", t.simple_page_pdf(content, resources=resources, extra_objects=[mesh]))
    r = h.measure([pdftotext, path, "-"], timeout=120)
    t.check(r.returncode == 0 and not r.timed_out, "pdftotext finishes", r.stderr)
    t.check(r.text.split() == ["BEFORE", "AFTER"], "text around the shadings is extracted", repr(r.text))
    t.check(r.cpu < 0.5, "400 mesh shadings: CPU time %.2fs < 0.5s" % r.cpu)

    # Rendering must still paint shadings: an axial shading on a 100x100 page.
    shading = "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> /Extend [true true] >>"
    page = t.simple_page_pdf(b"/Sh0 sh", resources="<< /Shading << /Sh0 6 0 R >> >>", extra_objects=[shading], mediabox=(0, 0, 100, 100))
    path = d.write("axial.pdf", page)
    utils.run("pdftoppm", "-r", "72", "-singlefile", path, os.path.join(d.path, "out"))
    with open(os.path.join(d.path, "out.ppm"), "rb") as f:
        data = f.read()
    header = b"P6\n100 100\n255\n"
    t.check(data.startswith(header), "pdftoppm writes a 100x100 PPM", data[:20])
    left = tuple(data[len(header) + 3 * (50 * 100 + 2) :][:3])
    right = tuple(data[len(header) + 3 * (50 * 100 + 97) :][:3])
    t.check(left[0] > 220 and left[2] < 40, "left edge is red", left)
    t.check(right[2] > 220 and right[0] < 40, "right edge is blue", right)
t.finish()
