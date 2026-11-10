#!/usr/bin/env python3
"""A stand-in for pdfregions (docslurp/extraction/PROTOCOL.md) for tests: answers every job
with a flat-colour rgb8 crop or a trivial SVG. FAKE_PDFREGIONS_MODE: ok (default), crash
(exit 1 after the first record), hang (sleep), mismatch (wrong sizes), script (SVG with a
<script>, refused by the allowlist), bigsvg (an SVG over --svg-max-bytes)."""
import os, sys, time

mode = os.environ.get("FAKE_PDFREGIONS_MODE", "ok")
out = sys.stdout.buffer
jobs = [l.rstrip("\n").split("\t") for l in sys.stdin if l.strip()]
if mode == "hang":
    time.sleep(60)
for k, (jid, page, x0, y0, x1, y1, kind, dpi) in enumerate(jobs):
    x0, y0, x1, y1, dpi = float(x0), float(y0), float(x1), float(y1), int(dpi)
    if kind == "raster":
        w = max(1, round((x1 - x0) * dpi / 72)); h = max(1, round((y1 - y0) * dpi / 72))
        if mode == "mismatch":
            w += 5
        colour = bytes([40, 120, 200])
        out.write(f"{jid}\tok\trgb8\t{w}\t{h}\t{w*h*3}\n".encode()); out.write(colour * (w * h))
    else:
        extra = ""
        if mode == "script":
            extra = "<script>alert(1)</script>"
        elif mode == "bigsvg":
            extra = "".join(f'<path d="M {x0+i%50} {y0} L {x1} {y1-i%40}"/>' for i in range(20000))
        svg = (f'<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" '
               f'width="{x1-x0}pt" height="{y1-y0}pt" viewBox="{x0} {y0} {x1-x0} {y1-y0}">\n'
               f'<g fill="rgb(0%, 0%, 100%)" fill-opacity="1">\n<path d="M {x0+1.234567} {y0+1.5} L {x1-1} {y1-1} Z "/>\n</g>\n<g>\n</g>\n{extra}</svg>\n').encode()
        out.write(f"{jid}\tok\tsvg\t0\t0\t{len(svg)}\n".encode()); out.write(svg)
    out.flush()
    if mode == "crash":
        sys.exit(1)
