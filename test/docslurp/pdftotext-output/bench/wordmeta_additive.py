#!/usr/bin/env python3
"""Check on a corpus that `-wordmeta` only adds fields.

usage: wordmeta_additive.py BINDIR LISTFILE [--jobs N]

For every PDF and each of -bbox, -bbox-layout and -tsv: removing the extra
attributes/columns from the -wordmeta output must give exactly the output
without the flag. Prints counts only. Also counts words with invisible=1."""
import concurrent.futures
import os
import re
import subprocess
import sys

ATTR = re.compile(rb' (fontName|fontSize|rotation|spaceAfter|invisible|color|baseline)="[^"]*"')


def pt(bindir, pdf, *flags):
    p = subprocess.run([os.path.join(bindir, "pdftotext")] + list(flags) + [pdf, "-"], capture_output=True, timeout=300)
    return p.returncode, p.stdout


def strip_tsv(data):
    out = []
    for line in data.split(b"\n"):
        cols = line.split(b"\t")
        out.append(b"\t".join(cols[:12]) if len(cols) > 12 else line)
    return b"\n".join(out)


def check(args):
    bindir, pdf = args
    res = {}
    for mode in ("-bbox", "-bbox-layout", "-tsv"):
        rc0, plain = pt(bindir, pdf, mode)
        rc1, meta = pt(bindir, pdf, mode, "-wordmeta")
        stripped = strip_tsv(meta) if mode == "-tsv" else ATTR.sub(b"", meta)
        invisible = meta.count(b'invisible="1"') if mode != "-tsv" else sum(1 for l in meta.split(b"\n") if l.startswith(b"5\t") and l.split(b"\t")[16:17] == [b"1"])
        res[mode] = (rc0 == rc1 and plain == stripped, invisible)
    return res


def main():
    argv = sys.argv[1:]
    jobs = 3
    if "--jobs" in argv:
        i = argv.index("--jobs")
        jobs = int(argv[i + 1])
        del argv[i:i + 2]
    bindir, listfile = argv
    pdfs = [l.strip() for l in open(listfile) if l.strip()]
    ok = {m: 0 for m in ("-bbox", "-bbox-layout", "-tsv")}
    inv_files = 0
    with concurrent.futures.ThreadPoolExecutor(jobs) as ex:
        for res in ex.map(check, [(bindir, p) for p in pdfs]):
            for m, (same, inv) in res.items():
                ok[m] += same
            inv_files += res["-bbox"][1] > 0
    for m, n in ok.items():
        print("%-13s additive on %d of %d PDFs" % (m, n, len(pdfs)))
    print("PDFs with at least one invisible=1 word: %d" % inv_files)


main()
