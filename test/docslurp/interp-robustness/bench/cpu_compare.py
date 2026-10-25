#!/usr/bin/env python3
"""Interleaved before/after CPU and peak-RSS comparison of a poppler util.

usage: cpu_compare.py [-n RUNS] [--util pdftotext] BASE_BINDIR NEW_BINDIR FILE...

Runs BASE and NEW alternately RUNS times per file (default 7) and prints the
median user CPU (user+sys is shown too) and the median peak RSS.  Output is
sent to /dev/null; stderr is discarded.
"""

import argparse
import os
import statistics
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import ir_helpers as h  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("-n", "--runs", type=int, default=7)
    ap.add_argument("--util", default="pdftotext")
    ap.add_argument("--args", default="", help="extra util arguments (space separated)")
    ap.add_argument("base")
    ap.add_argument("new")
    ap.add_argument("files", nargs="+")
    a = ap.parse_args()
    extra = a.args.split()
    print("%-40s %9s %9s %8s | %8s %8s" % ("file", "base user", "new user", "change", "base RSS", "new RSS"))
    for f in a.files:
        res = {"base": [], "new": []}
        for _ in range(a.runs):
            for tag, d in (("base", a.base), ("new", a.new)):
                out = os.devnull if a.util == "pdftotext" else None
                cmd = [os.path.join(d, a.util)] + extra + [f] + ([out] if a.util == "pdftotext" else [])
                res[tag].append(h.measure(cmd, timeout=600))
        bu = statistics.median(r.user for r in res["base"])
        nu = statistics.median(r.user for r in res["new"])
        br = statistics.median(r.maxrss_kb for r in res["base"]) / 1024
        nr = statistics.median(r.maxrss_kb for r in res["new"]) / 1024
        print("%-40s %8.3fs %8.3fs %+7.1f%% | %6.1fMB %6.1fMB" % (os.path.basename(f)[:40], bu, nu, 100.0 * (nu - bu) / bu if bu else 0, br, nr))


main()
