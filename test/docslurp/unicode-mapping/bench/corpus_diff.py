#!/usr/bin/env python3
"""Compare pdftotext output of a baseline and a candidate build over a PDF corpus.

usage: corpus_diff.py --base DIR --new DIR --out SCRATCHDIR [--glob '/path/**/*.pdf'] [-j 3] [pdftotext args...]

Writes <out>/<n>.base.txt and <n>.new.txt only for PDFs that differ, and prints a summary
(identical / changed / failed) plus the list of changed files with a unified-diff line count.
Corpus PDFs and their output are private: keep --out in scratch space and never commit it.
"""

import argparse
import collections
import concurrent.futures
import glob
import os
import subprocess
import sys


def run(binary, pdf, extra):
    try:
        p = subprocess.run([binary, *extra, pdf, "-"], capture_output=True, timeout=300)
        return p.returncode, p.stdout
    except subprocess.TimeoutExpired:
        return -999, b""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--new", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--glob", default="/home/dan/tmp/docslurp-*/**/*.pdf", help="recursive glob of PDFs")
    ap.add_argument("-j", type=int, default=3)
    args, extra = ap.parse_known_args()
    pdfs = sorted(glob.glob(args.glob, recursive=True))
    base_bin = os.path.join(args.base, "pdftotext")
    new_bin = os.path.join(args.new, "pdftotext")
    os.makedirs(args.out, exist_ok=True)

    def one(item):
        i, pdf = item
        rb, ob = run(base_bin, pdf, extra)
        rn, on = run(new_bin, pdf, extra)
        return i, pdf, rb, ob, rn, on

    same = changed = failed = 0
    changed_list = []
    with concurrent.futures.ThreadPoolExecutor(args.j) as ex:
        for i, pdf, rb, ob, rn, on in ex.map(one, enumerate(pdfs)):
            if rb != 0 and rn != 0:
                failed += 1
                continue
            if rb == rn and ob == on:
                same += 1
                continue
            changed += 1
            with open(os.path.join(args.out, "%03d.base.txt" % i), "wb") as f:
                f.write(ob)
            with open(os.path.join(args.out, "%03d.new.txt" % i), "wb") as f:
                f.write(on)
            a = ob.decode("utf-8", "replace").splitlines()
            b = on.decode("utf-8", "replace").splitlines()
            # line-multiset difference: cheap even for huge outputs (difflib is quadratic)
            ca, cb = collections.Counter(a), collections.Counter(b)
            nd = sum(((ca - cb) + (cb - ca)).values())
            changed_list.append((i, pdf, rb, rn, nd))
    print("PDFs: %d  identical: %d  changed: %d  failed-in-both: %d" % (len(pdfs), same, changed, failed))
    for i, pdf, rb, rn, nd in changed_list:
        print("%03d rc=%d/%d difflines=%d %s" % (i, rb, rn, nd, pdf))


if __name__ == "__main__":
    sys.exit(main())
