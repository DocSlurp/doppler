"""-pngopt compression=N is accepted by pdftoppm, pdftocairo and pdfimages.

Checks that the level changes the file size (0 stores, 9 is smallest), that the
decoded pixels are identical at every level, that bad values are rejected, and
that the default level matches PNGWriter::defaultCompressionLevel.
"""

import os
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(here)))
sys.path.insert(0, here)
import docslurp_testlib as t  # noqa: E402
import rastertools as r  # noqa: E402

DEFAULT_LEVEL = 6  # keep in sync with PNGWriter::defaultCompressionLevel

utils = t.Utils.from_args()


def page_pdf():
    # Text plus an RGB gradient image: compressible, but not trivially so.
    w, h = 96, 64
    pixels = bytearray()
    for y in range(h):
        for x in range(w):
            pixels += bytes([(x * 5) & 255, (y * 7) & 255, ((x ^ y) * 9) & 255])
    img = t.stream("/Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceRGB /BitsPerComponent 8" % (w, h), bytes(pixels), compress=True)
    content = b"q 240 0 0 160 20 20 cm /Im0 Do Q BT /F1 12 Tf 20 200 Td (compression level test) Tj ET"
    return t.simple_page_pdf(content, resources="<< /Font << /F1 4 0 R >> /XObject << /Im0 6 0 R >> >>", extra_objects=[img], mediabox=(0, 0, 300, 250))


def exit_code(util, *args):
    return subprocess.run([os.path.join(utils.bindir, util)] + [str(a) for a in args], capture_output=True).returncode


def png_of(path):
    with open(path, "rb") as f:
        return f.read()


with t.TempDir() as d:
    pdf = d.write("page.pdf", page_pdf())

    def out(name):
        return os.path.join(d.path, name)

    # pdftoppm
    sizes = {}
    pixels = {}
    for level in (0, 1, 6, 9):
        o = out("ppm%d" % level)
        utils.run("pdftoppm", "-r", 100, "-png", "-pngopt", "compression=%d" % level, "-singlefile", pdf, o)
        data = png_of(o + ".png")
        sizes[level] = len(data)
        pixels[level] = r.png_pixels(data)
    t.check(sizes[0] > sizes[9], "pdftoppm: level 0 output is larger than level 9", str(sizes))
    t.check(sizes[1] >= sizes[9], "pdftoppm: level 1 output is not smaller than level 9", str(sizes))
    t.check(len(set(sizes.values())) > 1, "pdftoppm: level changes file size", str(sizes))
    t.check(all(pixels[l] == pixels[9] for l in pixels), "pdftoppm: pixels identical at every level")

    o = out("ppmdef")
    utils.run("pdftoppm", "-r", 100, "-png", "-singlefile", pdf, o)
    o2 = out("ppmexp")
    utils.run("pdftoppm", "-r", 100, "-png", "-pngopt", "compression=%d" % DEFAULT_LEVEL, "-singlefile", pdf, o2)
    t.check(png_of(o + ".png") == png_of(o2 + ".png"), "pdftoppm: default output equals compression=%d" % DEFAULT_LEVEL)

    # pdftoppm to stdout takes the same code path
    stdout_png = utils.run("pdftoppm", "-r", 100, "-png", "-pngopt", "compression=0", "-singlefile", pdf, binary=True)
    t.check(len(stdout_png) == sizes[0], "pdftoppm: -pngopt honoured when writing to stdout", "%d vs %d" % (len(stdout_png), sizes[0]))

    # bad values
    for bad in ("compression=10", "compression=-1", "compression=x", "compression", "bogus=1"):
        rc = exit_code("pdftoppm", "-png", "-pngopt", bad, "-singlefile", pdf, out("bad"))
        t.check(rc != 0 and not os.path.exists(out("bad.png")), "pdftoppm: -pngopt %s is rejected" % bad, "rc=%d" % rc)

    # pdftocairo
    csizes = {}
    cpix = {}
    for level in (0, 9):
        o = out("cairo%d" % level)
        utils.run("pdftocairo", "-r", 100, "-png", "-pngopt", "compression=%d" % level, "-singlefile", pdf, o)
        data = png_of(o + ".png")
        csizes[level] = len(data)
        cpix[level] = r.png_pixels(data)
    t.check(csizes[0] > csizes[9], "pdftocairo: level 0 output is larger than level 9", str(csizes))
    t.check(cpix[0] == cpix[9], "pdftocairo: pixels identical at levels 0 and 9")

    # pdfimages
    isizes = {}
    ipix = {}
    for level in (0, 9):
        root = out("img%d" % level)
        utils.run("pdfimages", "-png", "-pngopt", "compression=%d" % level, pdf, root)
        data = png_of(root + "-000.png")
        isizes[level] = len(data)
        ipix[level] = r.png_pixels(data)
    t.check(isizes[0] > isizes[9], "pdfimages: level 0 output is larger than level 9", str(isizes))
    t.check(ipix[0] == ipix[9], "pdfimages: pixels identical at levels 0 and 9")

    rc = exit_code("pdfimages", "-png", "-pngopt", "compression=11", pdf, out("badimg"))
    t.check(rc != 0, "pdfimages: compression=11 is rejected", "rc=%d" % rc)
    rc = exit_code("pdftocairo", "-jpeg", "-pngopt", "compression=1", pdf, out("badcairo"))
    t.check(rc != 0, "pdftocairo: -pngopt without -png is rejected", "rc=%d" % rc)

t.finish()
