#!/usr/bin/env python3
"""Pixel-identity and listing checks between a baseline and a candidate build.

  identity.py --base DIR --new DIR [--pages 1,2,3] [--dpi 150] FILE.pdf...

For each PDF and page it renders with both builds and compares:
  * PPM bytes (pdftoppm default output),
  * PNG (baseline default level 9 vs candidate default) with `magick compare -metric AE`,
  * -gray PNG (baseline RGB-expanded vs candidate grayscale) with AE,
  * -mono PNG likewise,
and finally compares the `pdfimages -list` text of both builds.
Prints one line per check and exits non-zero on any difference.
Uses $TMPDIR for scratch.  Never commit the corpus PDFs or any output.
"""
import argparse, os, shutil, subprocess, sys, tempfile


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, **kw)


def ae(a, b):
    p = run(["magick", "compare", "-metric", "AE", a, b, "null:"])
    txt = p.stderr.decode().strip().split()
    return txt[0] if txt else "ERR"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--new", required=True)
    ap.add_argument("--pages", default="1")
    ap.add_argument("--dpi", default="150")
    ap.add_argument("--no-list", action="store_true")
    ap.add_argument("pdfs", nargs="+")
    a = ap.parse_args()
    d = tempfile.mkdtemp(prefix="identity.", dir=os.environ.get("TMPDIR"))
    bad = 0
    try:
        for pdf in a.pdfs:
            name = os.path.basename(pdf)
            for pg in a.pages.split(","):
                def render(tag, bindir, *flags):
                    out = os.path.join(d, tag)
                    r = run([os.path.join(bindir, "pdftoppm"), "-r", a.dpi, "-f", pg, "-l", pg, "-singlefile", *flags, pdf, out])
                    return out, r.returncode
                # PPM
                o, _ = render("b", a.base); n, _ = render("n", a.new)
                same = open(o + ".ppm", "rb").read() == open(n + ".ppm", "rb").read()
                print("%-40s p%-3s PPM bytes identical: %s" % (name[:40], pg, same)); bad += not same
                for label, flags, ext in (("PNG", ["-png"], "png"), ("gray PNG", ["-png", "-gray"], "png"), ("mono PNG", ["-png", "-mono"], "png")):
                    o, _ = render("b", a.base, *flags); n, _ = render("n", a.new, *flags)
                    v = ae(o + "." + ext, n + "." + ext)
                    print("%-40s p%-3s %-9s AE=%s  (base %d B, new %d B)" % (name[:40], pg, label, v, os.path.getsize(o + ".png"), os.path.getsize(n + ".png"))); bad += v != "0"
            if not a.no_list:
                lb = run([os.path.join(a.base, "pdfimages"), "-list", pdf]).stdout
                ln = run([os.path.join(a.new, "pdfimages"), "-list", pdf]).stdout
                print("%-40s pdfimages -list identical: %s" % (name[:40], lb == ln)); bad += lb != ln
    finally:
        shutil.rmtree(d, ignore_errors=True)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
