"""pdftoppm -png writes grayscale/1-bit PNGs for -gray/-mono instead of RGB.

The IHDR colour type must be 0 (grayscale) for -gray (8-bit) and -mono (1-bit),
2 (RGB) otherwise, and the decoded pixels must equal the PGM/PBM output of the
same render.
"""

import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(here)))
sys.path.insert(0, here)
import docslurp_testlib as t  # noqa: E402
import rastertools as r  # noqa: E402

utils = t.Utils.from_args()

# Text, a gray ramp and a coloured box so that gray conversion is exercised.
content = b"""0.2 0.4 0.8 rg 20 20 100 60 re f
0.5 g 140 20 100 60 re f
0 g BT /F1 24 Tf 20 120 Td (Gray and mono PNG) Tj ET
1 0 0 RG 3 w 20 100 m 260 160 l S"""

with t.TempDir() as d:
    pdf = d.write("p.pdf", t.simple_page_pdf(content, mediabox=(0, 0, 300, 200)))

    def png(*flags):
        data = utils.run("pdftoppm", "-r", 72, "-png", *flags, "-singlefile", pdf, binary=True)
        return data

    def pnm(*flags):
        return r.pnm_read(utils.run("pdftoppm", "-r", 72, *flags, "-singlefile", pdf, binary=True))

    # colour: unchanged, RGB 8-bit
    data = png()
    w, h, depth, ctype = r.png_header(data)
    t.check((depth, ctype) == (8, 2), "plain -png is 8-bit RGB (color type 2)", str((depth, ctype)))
    (_, rows) = r.png_pixels(data)
    magic, pw, ph, payload = pnm()
    t.check(b"".join(rows) == payload and (w, h) == (pw, ph), "plain -png pixels equal the PPM")

    # gray
    data = png("-gray")
    w, h, depth, ctype = r.png_header(data)
    t.check((depth, ctype) == (8, 0), "-gray -png is 8-bit grayscale (color type 0)", str((depth, ctype)))
    (_, rows) = r.png_pixels(data)
    magic, pw, ph, payload = pnm("-gray")
    t.check(magic == b"P5", "-gray without -png is a PGM", str(magic))
    t.check(b"".join(rows) == payload and (w, h) == (pw, ph), "-gray -png pixels equal the PGM")
    t.check(len(set(payload)) > 3, "gray render has several gray levels (sanity)")

    # mono
    data = png("-mono")
    w, h, depth, ctype = r.png_header(data)
    t.check((depth, ctype) == (1, 0), "-mono -png is 1-bit grayscale (color type 0, depth 1)", str((depth, ctype)))
    (_, rows) = r.png_pixels(data)
    magic, pw, ph, payload = pnm("-mono")
    t.check(magic == b"P4", "-mono without -png is a PBM", str(magic))
    stride = (w + 7) // 8
    # PBM has 1 = black, PNG gray has 1 = white
    inv = b"".join(bytes((~b) & 255 for b in payload[i * stride:(i + 1) * stride]) for i in range(h))
    mask = (0xff << (8 * stride - w)) & 0xff
    same = all((a ^ b) & (0xff if (j % stride) != stride - 1 else mask) == 0 for j, (a, b) in enumerate(zip(b"".join(rows), inv)))
    t.check(same and (w, h) == (pw, ph), "-mono -png pixels equal the (inverted) PBM")

    # stdout path and compression option still work with the gray writer
    out = utils.run("pdftoppm", "-r", 72, "-png", "-gray", "-pngopt", "compression=1", "-singlefile", pdf, binary=True)
    t.check(r.png_header(out)[2:] == (8, 0), "-gray -png -pngopt works")

t.finish()
