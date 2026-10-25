#!/usr/bin/env python3
"""Compare baseline and candidate poppler utils on a list of PDFs.

usage: compare_corpus.py LISTFILE BASE_BINDIR NEW_BINDIR OUTDIR [--tools pdftotext,struct-text,...] [--timeout SEC] [--strip-nul-base]

--strip-nul-base removes NUL bytes from the baseline output first, to separate the
TextStringToUtf8 trailing-NUL fix from other differences.

For each PDF listed in LISTFILE (one path per line) runs each tool with both
binaries, writes differing outputs to OUTDIR (scratch; never commit it) and
prints a summary. Timeouts and non-zero exits are reported separately.
"""

import difflib
import os
import subprocess
import sys

TOOLS = {
    "struct-text": ("pdfinfo", ["-struct-text"]),
    "struct": ("pdfinfo", ["-struct"]),
    "pdftotext": ("pdftotext", ["-q"]),
    "pdftotext-raw": ("pdftotext", ["-q", "-raw"]),
}


def run(bindir, tool, extra, pdf, timeout):
    cmd = [os.path.join(bindir, tool)] + extra + [pdf] + (["-"] if tool == "pdftotext" else [])
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        return p.returncode, p.stdout
    except subprocess.TimeoutExpired:
        return "timeout", b""


def main():
    listfile, base, new, outdir = sys.argv[1:5]
    tools = ["struct-text", "pdftotext"]
    if "--tools" in sys.argv:
        tools = sys.argv[sys.argv.index("--tools") + 1].split(",")
    timeout = 300
    if "--timeout" in sys.argv:
        timeout = int(sys.argv[sys.argv.index("--timeout") + 1])
    strip_nul = "--strip-nul-base" in sys.argv
    os.makedirs(outdir, exist_ok=True)
    pdfs = [line.strip() for line in open(listfile) if line.strip()]
    summary = {k: {"same": 0, "diff": 0, "error": 0} for k in tools}
    for i, pdf in enumerate(pdfs):
        for k in tools:
            tool, extra = TOOLS[k]
            rb, ob = run(base, tool, extra, pdf, timeout)
            rn, on = run(new, tool, extra, pdf, timeout)
            if strip_nul:
                ob = ob.replace(b"\0", b"")
            if rb != rn:
                summary[k]["error"] += 1
                print("RC-DIFF %s %s base=%s new=%s" % (k, pdf, rb, rn), flush=True)
                continue
            if ob == on:
                summary[k]["same"] += 1
                print("SAME %s %s" % (k, pdf), flush=True)
                continue
            summary[k]["diff"] += 1
            name = "%03d-%s-%s" % (i, k, os.path.basename(pdf))
            a = ob.decode("utf-8", "replace").splitlines()
            b = on.decode("utf-8", "replace").splitlines()
            d = list(difflib.unified_diff(a, b, "base", "new", lineterm="", n=1))
            with open(os.path.join(outdir, name + ".diff"), "w") as f:
                f.write("\n".join(d) + "\n")
            added = sum(1 for x in d if x.startswith("+") and not x.startswith("+++"))
            removed = sum(1 for x in d if x.startswith("-") and not x.startswith("---"))
            print("DIFF %s %s +%d -%d lines (bytes %d -> %d)" % (k, pdf, added, removed, len(ob), len(on)), flush=True)
    print("files: %d" % len(pdfs))
    for k, v in summary.items():
        print("%-14s same=%d diff=%d rc-differs=%d" % (k, v["same"], v["diff"], v["error"]))


if __name__ == "__main__":
    main()
