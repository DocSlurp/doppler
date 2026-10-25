"""Skipping an inline image on a text-only device must be bounded by the real data.

An inline image declares its size in /W and /H, which a hostile file can set to
anything.  When the device ignores images (pdftotext), Gfx still has to consume
the image data to find 'EI', and it used to call getChar() W*H*bpc/8 times even
after the stream hit EOF, while EmbedStream kept appending EOF markers to its
replay buffer (16000x16000 gray: ~3 s CPU and ~260 MB RSS from a 626 byte file).
"""

import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(here)))
sys.path.insert(0, here)
import docslurp_testlib as t  # noqa: E402
import ir_helpers as h  # noqa: E402

utils = t.Utils.from_args()
pdftotext = os.path.join(utils.bindir, "pdftotext")
pdftoppm = os.path.join(utils.bindir, "pdftoppm")

AFTER = b" BT /F1 12 Tf 72 700 Td (AFTER) Tj ET"


def run_text(d, name, content):
    path = d.write(name + ".pdf", t.simple_page_pdf(content))
    return h.measure([pdftotext, path, "-"], timeout=120)


with t.TempDir() as d:
    # Hostile sizes with truncated data: must be cheap in CPU and memory.
    hostile = {
        "raw gray": b"BI /W 16000 /H 16000 /BPC 8 /CS /G ID xx EI",
        "raw image mask": b"BI /W 40000 /H 40000 /IM true ID xx EI",
    }
    for what, content in hostile.items():
        r = run_text(d, "hostile", content)
        t.check(r.returncode == 0 and not r.timed_out, "%s: pdftotext finishes" % what, r.stderr)
        t.check(r.cpu < 1.0, "%s: CPU time %.2fs < 1.0s" % (what, r.cpu))
        t.check(r.maxrss_kb < 100 * 1024, "%s: peak RSS %d MB < 100 MB" % (what, r.maxrss_kb // 1024))

    # Filtered data ends long before the declared size, and text follows 'EI':
    # the skip must stop at the filter's EOF and the parser must resume after EI.
    filtered = {
        "AHx": b"/F /AHx ID 0011> EI",
        "A85": b'/F /A85 ID 87cURD]i,"Ebo80~> EI',
        "Fl": b"/F /Fl ID \x78\x9c\x03\x00\x00\x00\x00\x01 EI",
        "RL": b"/F /RL ID \x80 EI",
        "LZW": b"/F /LZW ID \x80\x0b\x60\x50\x22\x0c\x0c\x85\x01 EI",
        "DCT": b"/F /DCT ID \xff\xd8\xff EI",
    }
    for what, tail in filtered.items():
        r = run_text(d, "filtered", b"BI /W 16000 /H 16000 /BPC 8 /CS /G " + tail + AFTER)
        t.check(r.text.strip() == "AFTER", "%s: text after the image is extracted" % what, repr(r.text))
        t.check(r.cpu < 0.4, "%s: CPU time %.2fs < 0.4s" % (what, r.cpu))

    # Ordinary inline images: the parser resumes right after EI.  The raw data
    # deliberately contains the bytes "EI " so a naive scan for EI would stop early.
    gray = b"BI /W 4 /H 4 /BPC 8 /CS /G ID " + b"\x10EI \x20\x30\x40\x50\x60\x70\x80\x90\xa0\xb0\xc0\xd0\xe0\xf0" + b"\nEI"
    mask = b"BI /W 16 /H 3 /IM true /D [1 0] ID \xaa\xaa\x55\x55\xee\xee\nEI"
    rgb_ahx = b"BI /W 2 /H 2 /BPC 8 /CS /RGB /F /AHx ID ff0000 00ff00 0000ff ffff00> EI"
    content = b"q 50 0 0 50 10 10 cm " + gray + b" Q q 20 0 0 20 100 10 cm " + mask + b" Q q 40 0 0 40 200 10 cm " + rgb_ahx + b" Q"
    r = run_text(d, "normal", content + AFTER)
    t.check(r.text.strip() == "AFTER", "ordinary inline images: text after them is extracted", repr(r.text))
    r = run_text(d, "twice", content + b" BT /F1 12 Tf 72 750 Td (MIDDLE) Tj ET " + content + AFTER)
    t.check(r.text.split() == ["MIDDLE", "AFTER"], "two rounds of inline images keep text order", repr(r.text))

    # Rendering still draws the image (the skip path is only for text devices).
    page = t.simple_page_pdf(b"q 100 0 0 100 0 0 cm " + b"BI /W 2 /H 2 /BPC 8 /CS /RGB ID \xff\x00\x00\x00\xff\x00\x00\x00\xff\xff\xff\x00\nEI Q", mediabox=(0, 0, 100, 100))
    path = d.write("render.pdf", page)
    utils.run("pdftoppm", "-r", "72", "-singlefile", path, os.path.join(d.path, "out"))
    with open(os.path.join(d.path, "out.ppm"), "rb") as f:
        data = f.read()
    # P6\n100 100\n255\n then RGB triples
    header_len = len(b"P6\n100 100\n255\n")
    t.check(data.startswith(b"P6\n100 100\n255\n"), "pdftoppm writes a 100x100 PPM", data[:20])

    def px(x, y):
        o = header_len + 3 * (y * 100 + x)
        return tuple(data[o : o + 3])

    t.check(px(10, 10) == (255, 0, 0), "top-left pixel is red", px(10, 10))
    t.check(px(90, 10) == (0, 255, 0), "top-right pixel is green", px(90, 10))
    t.check(px(10, 90) == (0, 0, 255), "bottom-left pixel is blue", px(10, 90))
    t.check(px(90, 90) == (255, 255, 0), "bottom-right pixel is yellow", px(90, 90))
t.finish()
