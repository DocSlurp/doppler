#!/usr/bin/env python3
"""Interleaved user-CPU benchmark of several pdftotext binaries on one PDF.

usage: bench_cpu.py [-n RUNS] PDF label=binary [label=binary ...] -- pdftotext-args...

Runs are interleaved (a b c a b c ...) so machine load affects all binaries
alike, and the median user CPU time (not wall time) is reported."""
import os
import resource
import statistics
import subprocess
import sys

args = sys.argv[1:]
runs = 7
if args[0] == "-n":
    runs, args = int(args[1]), args[2:]
pdf, rest = args[0], args[1:]
sep = rest.index("--") if "--" in rest else len(rest)
bins = [a.split("=", 1) for a in rest[:sep]]
extra = rest[sep + 1:]
times = {label: [] for label, _ in bins}
for i in range(runs + 1):  # first round is a warm-up
    for label, binary in bins:
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        subprocess.run([binary] + extra + [pdf, "-"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        if i:
            times[label].append(after.ru_utime - before.ru_utime)
base = None
for label, _ in bins:
    med = statistics.median(times[label])
    base = base or med
    print("%-12s median user %.3fs  min %.3fs  max %.3fs  (%.1f%% of first)  runs=%d" % (label, med, min(times[label]), max(times[label]), 100 * med / base, runs))
