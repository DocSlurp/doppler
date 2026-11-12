#!/usr/bin/env python3
"""A stand-in for pdfregions (docslurp/extraction/PROTOCOL.md) for tests: answers every job
with a flat-colour rgb8 crop or a trivial SVG.

FAKE_PDFREGIONS_MODE: ok (default), crash (exit 1 after the first record), hang (sleep),
mismatch (wrong sizes), script (SVG with a <script>, refused by the allowlist), bigsvg (an SVG
over --svg-max-bytes).

Knobs, for extraction batches (one process per batch):
  FAKE_PDFREGIONS_FAIL_PAGES=3,5    on reaching the first job of a listed page, exit 3 (a
                                    limit: the records already written stay valid)
  FAKE_PDFREGIONS_KILL_PAGES=3      likewise, but die by SIGKILL
  FAKE_PDFREGIONS_SLEEP=S           sleep S seconds before answering
  FAKE_PDFREGIONS_LOG=FILE          append one line per run: the first and last job pages
"""
import os, signal, sys, time

mode = os.environ.get("FAKE_PDFREGIONS_MODE", "ok")
pages_of = lambda name: {int(p) for p in os.environ.get(name, "").split(",") if p.strip()}
fail_pages, kill_pages = pages_of("FAKE_PDFREGIONS_FAIL_PAGES"), pages_of("FAKE_PDFREGIONS_KILL_PAGES")
sys.stderr.write("pdfregions: confined=1 landlock=3 seccomp=1\n")
out = sys.stdout.buffer
jobs = [l.rstrip("\n").split("\t") for l in sys.stdin if l.strip()]
log = os.environ.get("FAKE_PDFREGIONS_LOG")
if log and jobs:
    with open(log, "a") as f:
        f.write(f"{jobs[0][1]} {jobs[-1][1]}\n")
if os.environ.get("FAKE_PDFREGIONS_SLEEP"):
    time.sleep(float(os.environ["FAKE_PDFREGIONS_SLEEP"]))
if mode == "hang":
    time.sleep(60)
for k, (jid, page, x0, y0, x1, y1, kind, dpi) in enumerate(jobs):
    if int(page) in fail_pages:
        out.flush()
        sys.exit(3)
    if int(page) in kill_pages:
        out.flush()
        os.kill(os.getpid(), signal.SIGKILL)
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
