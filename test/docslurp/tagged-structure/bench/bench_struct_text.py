#!/usr/bin/env python3
"""Interleaved user-CPU benchmark of two sets of poppler utils.

usage: bench_struct_text.py BASE_BINDIR NEW_BINDIR [-n RUNS] [--cmd pdfinfo-struct-text|pdftotext] FILE...

Runs base and new alternately RUNS times per file (default 5) and prints the
median user+sys CPU seconds of each, measured with wait4() rusage so machine
load does not matter much.
"""

import os
import statistics
import subprocess
import sys

CMDS = {"pdfinfo-struct-text": ("pdfinfo", ["-struct-text"]), "pdftotext": ("pdftotext", ["-q"])}


def cpu(argv):
    pid = os.posix_spawn(argv[0], argv, os.environ, file_actions=[(os.POSIX_SPAWN_OPEN, 1, os.devnull, os.O_WRONLY, 0), (os.POSIX_SPAWN_OPEN, 2, os.devnull, os.O_WRONLY, 0)])
    _, status, ru = os.wait4(pid, 0)
    return ru.ru_utime, ru.ru_stime


def main():
    a = sys.argv[1:]
    runs = 5
    cmd = "pdfinfo-struct-text"
    files = []
    base, new = a[0], a[1]
    i = 2
    while i < len(a):
        if a[i] == "-n":
            runs = int(a[i + 1])
            i += 2
        elif a[i] == "--cmd":
            cmd = a[i + 1]
            i += 2
        else:
            files.append(a[i])
            i += 1
    tool, extra = CMDS[cmd]
    print("%-50s %10s %10s %8s" % ("file (%s, median of %d)" % (cmd, runs), "base user s", "new user s", "speedup"))
    for f in files:
        res = {"base": [], "new": []}
        for _ in range(runs):
            for name, d in (("base", base), ("new", new)):
                argv = [os.path.join(d, tool)] + extra + [f] + (["-"] if tool == "pdftotext" else [])
                u, s = cpu(argv)
                res[name].append(u + s)
        mb, mn = statistics.median(res["base"]), statistics.median(res["new"])
        print("%-50s %10.3f %10.3f %7.1fx" % (os.path.basename(f)[:50], mb, mn, mb / mn if mn else float("inf")))


if __name__ == "__main__":
    main()
