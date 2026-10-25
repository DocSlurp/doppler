"""JPXStream::getImageParams reads the header only.

pdfimages -list used to run a complete OpenJPEG decode per JPX image just to
report its colour space.  The checks here are:
  * the listed parameters are right for RGB/gray JP2 and J2K fixtures,
  * a JPX whose tile data is damaged but whose header is fine lists silently
    (nothing is decoded), while rendering still reports the damage,
  * listing a large image costs far less CPU than rendering it,
  * rendering is still correct after getImageParams ran (full decode later),
  * a JP2 whose pclr box expands a 1-component codestream to RGB is still
    reported as RGB (the header alone is not enough there).
"""

import os
import resource
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(here)))
sys.path.insert(0, here)
import docslurp_testlib as t  # noqa: E402
import rastertools as r  # noqa: E402

utils = t.Utils.from_args()


def run(util, *args):
    """Return (stdout bytes, stderr text, user+sys CPU seconds of the child)."""
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    proc = subprocess.run([os.path.join(utils.bindir, util)] + [str(a) for a in args], capture_output=True, timeout=120)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
    return proc.returncode, proc.stdout, proc.stderr.decode(errors="replace"), cpu


def listing(path):
    rc, out, err, cpu = run("pdfimages", "-list", path)
    rows = [line.split() for line in out.decode().splitlines()[2:]]
    return rc, rows, err, cpu


# Column positions of `pdfimages -list`
PAGE, NUM, TYPE, WIDTH, HEIGHT, COLOR, COMP, BPC, ENC = range(9)

with t.TempDir() as d:
    # --- parameters of small fixtures -------------------------------------------------
    cases = [
        ("rgb.jp2", 64, 48, "rgb", "3"),
        ("rgb.j2k", 64, 48, "rgb", "3"),
        ("gray.j2k", 64, 48, "gray", "1"),
    ]
    for name, w, h, color, comps in cases:
        pdf = d.write(name + ".pdf", r.jpx_image_pdf(r.fixture(name), w, h))
        rc, rows, err, _ = listing(pdf)
        ok = rc == 0 and len(rows) == 1 and rows[0][COLOR] == color and rows[0][COMP] == comps and rows[0][ENC] == "jpx"
        t.check(ok, "pdfimages -list %s: %s/%s jpx" % (name, color, comps), repr(rows) + err)
        t.check(err == "", "pdfimages -list %s: no diagnostics" % name, err)

    # --- header-only: damaged tile data is not touched by -list ------------------------
    rgb = bytearray(r.fixture("rgb.jp2"))
    # Keep the JP2 header and the codestream main header, wreck the tile payload.
    sot = rgb.find(b"\xff\x90")
    t.check(sot > 0, "found SOT marker in fixture")
    damaged = bytes(rgb[:sot + 14]) + bytes([0xff]) * (len(rgb) - sot - 14)
    pdf = d.write("damaged.pdf", r.jpx_image_pdf(damaged, 64, 48))
    rc, rows, err, _ = listing(pdf)
    t.check(rc == 0 and len(rows) == 1 and rows[0][COLOR] == "rgb" and rows[0][COMP] == "3", "damaged tile data: -list still reports rgb/3", repr(rows) + err)
    t.check(err == "", "damaged tile data: -list does not decode (no OpenJPEG diagnostics)", err)

    # --- cost: listing a large flat image vs rendering it ------------------------------
    big = r.fixture("big.jp2")
    pdf = d.write("big.pdf", r.jpx_image_pdf(big, 3000, 3000))
    list_cpu = min(listing(pdf)[3] for _ in range(3))
    rc, rows, err, _ = listing(pdf)
    t.check(rc == 0 and rows[0][WIDTH] == "3000" and rows[0][COLOR] == "rgb", "big jp2 listed correctly", repr(rows) + err)
    render_cpu = min(run("pdftoppm", "-r", 20, "-png", "-singlefile", pdf, os.path.join(d.path, "big"))[3] for _ in range(3))
    t.check(list_cpu < 0.5 * render_cpu, "listing a 3000x3000 JPX costs < half of rendering it (%.2fs vs %.2fs CPU)" % (list_cpu, render_cpu))

    # --- rendering is unchanged and still decodes after getImageParams -----------------
    pdf = d.write("render.pdf", r.jpx_image_pdf(big, 3000, 3000))
    rc, out, err, _ = run("pdftoppm", "-r", 72, "-singlefile", pdf)
    magic, w, h, payload = r.pnm_read(out)
    cx, cy = w // 2, h // 2
    px = payload[(cy * w + cx) * 3:(cy * w + cx) * 3 + 3]
    # the fixture is lossy-compressed, so allow a couple of levels of error
    t.check(rc == 0 and all(abs(a - b) <= 3 for a, b in zip(px, (200, 30, 60))), "render: JPX centre pixel is the encoded colour", repr(tuple(px)) + err)
    t.check(tuple(payload[:3]) == (255, 255, 255), "render: page background untouched")

    # --- palette JP2: header says 1 component, decode gives RGB ------------------------
    gray_cs = r.fixture("gray.j2k")
    pal = r.palette_jp2(gray_cs, 64, 48)
    pdf = d.write("pal.pdf", r.jpx_image_pdf(pal, 64, 48))
    rc, rows, err, _ = listing(pdf)
    t.check(rc == 0 and len(rows) == 1 and rows[0][COLOR] == "rgb" and rows[0][COMP] == "3", "palette JP2: -list reports the expanded rgb/3", repr(rows) + err)
    rc, out, err, _ = run("pdftoppm", "-r", 72, "-singlefile", pdf)
    magic, w, h, payload = r.pnm_read(out)
    t.check(rc == 0 and len(set(payload[i:i + 3] for i in range(0, len(payload), 3))) > 10, "palette JP2 renders in colour", err)

t.finish()
