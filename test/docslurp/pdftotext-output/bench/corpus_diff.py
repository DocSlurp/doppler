#!/usr/bin/env python3
"""Compare two builds of pdftotext/pdftohtml over a PDF corpus, per output mode.

usage: corpus_diff.py BASE_BINDIR NEW_BINDIR LISTFILE [--jobs N] [--rotate]

LISTFILE has one PDF path per line. With --rotate every PDF is also run through
`qpdf --rotate=+90` first (into a scratch dir) and compared too.
Only counts, file indexes and generic categories are printed (no document text),
so the output is safe to paste into a PR. Exit status is 0 regardless."""
import collections
import concurrent.futures
import os
import re
import subprocess
import sys
import tempfile

MODES = {
    "text": ("pdftotext", ["-"]),
    "layout": ("pdftotext", ["-layout", "-"]),
    "bbox": ("pdftotext", ["-bbox", "-"]),
    "bbox-layout": ("pdftotext", ["-bbox-layout", "-"]),
    "tsv": ("pdftotext", ["-tsv", "-"]),
    "html-xml": ("pdftohtml", ["-xml", "-i", "-stdout", "-q"]),
}


def run(bindir, mode, pdf, extra=()):
    util, args = MODES[mode]
    if util == "pdftohtml":
        cmd = [os.path.join(bindir, util)] + args + list(extra) + [pdf]
    else:
        cmd = [os.path.join(bindir, util)] + list(extra) + args[:-1] + [pdf, "-"]
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=300)
        return p.returncode, p.stdout
    except subprocess.TimeoutExpired:
        return -999, b""


def classify(mode, a, b):
    """Generic category of a difference, from the differing lines only."""
    la = a.decode("utf-8", "replace").split("\n")
    lb = b.decode("utf-8", "replace").split("\n")
    if len(la) != len(lb):
        return "line count differs" if mode in ("text", "layout") else "row count differs"
    diff = [(x, y) for x, y in zip(la, lb) if x != y]
    if mode in ("bbox", "bbox-layout") and all(x.lstrip().startswith("<page ") and y.lstrip().startswith("<page ") for x, y in diff):
        return "page size only"
    if mode == "tsv" and all("###PAGE###" in x and "###PAGE###" in y for x, y in diff):
        return "page size only"
    if mode in ("bbox", "bbox-layout", "tsv"):
        if all(re.sub(r"[\x00-\x1f\x7f]", " ", x) == y or x.lstrip().startswith("<page ") or "###PAGE###" in x for x, y in diff):
            return "page size and/or control characters"
        return "other word-level change"
    return "text changed on %d line(s)" % len(diff)


def main():
    argv = sys.argv[1:]
    jobs = 3
    if "--jobs" in argv:
        i = argv.index("--jobs")
        jobs = int(argv[i + 1])
        del argv[i:i + 2]
    rotate = "--rotate" in argv
    if rotate:
        argv.remove("--rotate")
    base, new, listfile = argv
    pdfs = [l.strip() for l in open(listfile) if l.strip()]
    scratch = tempfile.mkdtemp(prefix="corpus-diff.", dir=os.environ.get("TMPDIR"))
    if rotate:
        rotated = []
        for i, p in enumerate(pdfs):
            out = os.path.join(scratch, "r%d.pdf" % i)
            r = subprocess.run(["qpdf", "--rotate=+90", p, out], capture_output=True)
            if r.returncode in (0, 3) and os.path.exists(out):
                rotated.append(out)
        pdfs = rotated
        print("rotated corpus: %d PDFs" % len(pdfs))
    tasks = [(m, i, p) for m in MODES for i, p in enumerate(pdfs)]

    def work(t):
        m, i, p = t
        ra, rb = run(base, m, p), run(new, m, p)
        return m, i, ra, rb

    stats = {m: collections.Counter() for m in MODES}
    cats = {m: collections.defaultdict(list) for m in MODES}
    with concurrent.futures.ThreadPoolExecutor(jobs) as ex:
        for m, i, ra, rb in ex.map(work, tasks):
            if ra == rb:
                stats[m]["identical"] += 1
            else:
                stats[m]["changed"] += 1
                cats[m][classify(m, ra[1], rb[1]) if ra[0] == rb[0] else "exit status %d -> %d" % (ra[0], rb[0])].append(i)
    for m in MODES:
        print("%-12s identical %3d  changed %3d" % (m, stats[m]["identical"], stats[m]["changed"]))
        for c, idx in sorted(cats[m].items()):
            print("    %-40s %3d file(s), files #%s" % (c, len(idx), ",".join(map(str, idx))))
    print("scratch:", scratch)


main()
