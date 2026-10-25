#!/usr/bin/env python3
"""Interleaved user-CPU / peak-RSS comparison of pdftotext builds.

usage: cpu_compare.py [-n 7] [--arg ARG ...] LABEL=BINDIR [LABEL=BINDIR ...] -- file.pdf [file.pdf ...]

Each round runs every build once (in order), so machine load affects all builds alike.
Reports the median user CPU seconds (plus min/max) and the median peak RSS in MB.
"""

import argparse
import os
import resource
import statistics
import subprocess
import sys


def run_once(binary, args, pdf):
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    pid = os.fork()
    if pid == 0:
        fd = os.open(os.devnull, os.O_WRONLY)
        os.dup2(fd, 1)
        os.dup2(fd, 2)
        os.execv(binary, [binary] + args + [pdf, "-"])
    _, status, ru = os.wait4(pid, 0)
    if os.waitstatus_to_exitcode(status) != 0:
        raise SystemExit("%s failed on %s" % (binary, pdf))
    return ru.ru_utime, ru.ru_maxrss / 1024.0


def main():
    argv = sys.argv[1:]
    sep = argv.index("--")
    ap = argparse.ArgumentParser()
    ap.add_argument("-n", type=int, default=7)
    ap.add_argument("--arg", action="append", default=[])
    ap.add_argument("builds", nargs="+")
    ns = ap.parse_args(argv[:sep])
    pdfs = argv[sep + 1:]
    builds = [b.split("=", 1) for b in ns.builds]
    print("%-28s %-10s %8s %8s %8s %9s" % ("file", "build", "median_s", "min_s", "max_s", "rss_MB"))
    for pdf in pdfs:
        res = {label: ([], []) for label, _ in builds}
        for _ in range(ns.n):
            for label, bindir in builds:
                u, rss = run_once(os.path.join(bindir, "pdftotext"), ns.arg, pdf)
                res[label][0].append(u)
                res[label][1].append(rss)
        for label, _ in builds:
            us, rs = res[label]
            print("%-28s %-10s %8.3f %8.3f %8.3f %9.1f" % (os.path.basename(pdf)[:28], label, statistics.median(us), min(us), max(us), statistics.median(rs)))


if __name__ == "__main__":
    main()
