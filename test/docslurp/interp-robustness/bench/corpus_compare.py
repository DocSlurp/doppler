#!/usr/bin/env python3
"""Compare baseline and candidate poppler utils over a list of PDFs.

usage: corpus_compare.py MODE BASE_BINDIR NEW_BINDIR LISTFILE [--jobs 3]

MODE is `text` (pdftotext to stdout) or `render` (pdftoppm -r 50, pages 1-3,
PPM bytes compared).  LISTFILE has one PDF path per line.  Prints one line per
differing file plus a summary; never copies any PDF or output anywhere.
"""

import argparse
import concurrent.futures
import hashlib
import os
import subprocess
import tempfile


def run(bindir, mode, path):
    try:
        if mode == "text":
            p = subprocess.run([os.path.join(bindir, "pdftotext"), path, "-"], capture_output=True, timeout=600)
            return p.returncode, hashlib.sha256(p.stdout).hexdigest(), len(p.stdout)
        with tempfile.TemporaryDirectory() as d:
            p = subprocess.run([os.path.join(bindir, "pdftoppm"), "-r", "50", "-f", "1", "-l", "3", path, os.path.join(d, "p")], capture_output=True, timeout=600)
            h = hashlib.sha256()
            n = 0
            for name in sorted(os.listdir(d)):
                with open(os.path.join(d, name), "rb") as f:
                    h.update(f.read())
                n += 1
            return p.returncode, h.hexdigest(), n
    except subprocess.TimeoutExpired:
        return -999, "timeout", 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["text", "render"])
    ap.add_argument("base")
    ap.add_argument("new")
    ap.add_argument("list")
    ap.add_argument("--jobs", type=int, default=3)
    a = ap.parse_args()
    files = [l.strip() for l in open(a.list) if l.strip()]

    def both(f):
        return f, run(a.base, a.mode, f), run(a.new, a.mode, f)

    same = diff = 0
    with concurrent.futures.ThreadPoolExecutor(a.jobs) as ex:
        for f, b, n in ex.map(both, files):
            if b == n:
                same += 1
            else:
                diff += 1
                print("DIFF %s base=%s new=%s" % (f, b, n), flush=True)
    print("%s: %d files, %d identical, %d different" % (a.mode, len(files), same, diff))


main()
