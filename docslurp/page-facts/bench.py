#!/usr/bin/env python3
"""Page-facts benchmark: pages/s, CPU and peak RSS for pdfpagefacts (basic, full), the
--page-facts overhead on pdftomarkdown --stream (text only and with --markers), pdfinfo, the
pdftotext pass pdftomarkdown runs, and cold start.

    docslurp/page-facts/bench.py --bin build/utils \\
        --pdftomarkdown docslurp/doppler-md/target/release/pdftomarkdown \\
        [--runs 5] [--synthetic] [FILE.pdf ...]

CPU is user+sys of the child and everything it waited for (wait4, RUSAGE_BOTH on Linux),
peak RSS is the largest of them (so pdftomarkdown's includes its pdftotext). Runs go through
a small C harness compiled with cc into the temporary folder: a child forked from Python
keeps Python's peak RSS across exec, so os.wait4 from here would report ~26 MiB for every
command. Commands are interleaved and each figure is the median of --runs runs, as
DOCSLURP.md "Benchmarking" asks on a loaded machine. --synthetic adds generated inputs
(500 text pages, 300 scanned pages with an invisible OCR layer, 100 MRC pages); they are
written to a temporary folder under $TMPDIR and removed. Prints a Markdown report.
"""

import argparse
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))


HARNESS_C = r"""
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#include <fcntl.h>
int main(int argc, char **argv) {
    struct timespec a, b;
    clock_gettime(CLOCK_MONOTONIC, &a);
    pid_t pid = fork();
    if (pid == 0) {
        int fd = open("/dev/null", O_WRONLY);
        dup2(fd, 1); dup2(fd, 2); dup2(fd, 5); /* fd 5: --page-facts-fd 5 */
        execv(argv[1], argv + 1);
        _exit(127);
    }
    int status; struct rusage ru;
    wait4(pid, &status, 0, &ru);
    clock_gettime(CLOCK_MONOTONIC, &b);
    printf("%.6f %.6f %ld %d\n", (b.tv_sec - a.tv_sec) + (b.tv_nsec - a.tv_nsec) / 1e9,
           ru.ru_utime.tv_sec + ru.ru_utime.tv_usec / 1e6 + ru.ru_stime.tv_sec + ru.ru_stime.tv_usec / 1e6,
           ru.ru_maxrss, WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status));
    return 0;
}
"""
HARNESS = None


def build_harness(folder):
    global HARNESS
    src = os.path.join(folder, "harness.c")
    with open(src, "w") as f:
        f.write(HARNESS_C)
    exe = os.path.join(folder, "harness")
    if subprocess.run(["cc", "-O2", "-o", exe, src]).returncode == 0:
        HARNESS = exe


def run(cmd, env=None):
    """(wall s, cpu s, peak RSS KiB, exit status) of one run, output discarded."""
    cmd = [shutil.which(cmd[0]) or cmd[0]] + cmd[1:]
    if HARNESS:
        out = subprocess.run([HARNESS] + cmd, capture_output=True, text=True, env=env).stdout.split()
        return float(out[0]), float(out[1]), int(out[2]), int(out[3])
    devnull = os.open(os.devnull, os.O_WRONLY)
    start = time.perf_counter()
    p = subprocess.Popen(cmd, stdout=devnull, stderr=devnull, env=env)
    _, status, ru = os.wait4(p.pid, 0)
    wall = time.perf_counter() - start
    os.close(devnull)
    return wall, ru.ru_utime + ru.ru_stime, ru.ru_maxrss, os.waitstatus_to_exitcode(status)


def pages_of(bindir, path):
    out = subprocess.run([os.path.join(bindir, "pdfinfo"), path], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if line.startswith("Pages:"):
            return int(line.split()[1])
    return 0


def synthetic(folder):
    sys.path.insert(0, os.path.join(HERE, "..", "..", "test", "docslurp", "page-facts"))
    sys.path.insert(0, os.path.join(HERE, "..", "..", "test", "docslurp"))
    import pf_fixtures as fx

    made = []
    text_pages = [{"content": fx.words(400, y=760)} for _ in range(500)]
    made.append(("synthetic text, 500 pp", fx.doc(text_pages)))
    res = "<< /Font << /F1 3 0 R >> /XObject << /Im1 {0} 0 R /M1 {1} 0 R >> >>"
    scan_pages = [{"content": "q 612 0 0 792 0 0 cm /Im1 Do Q\n" + fx.words(300, y=760, invisible=True), "resources": res} for _ in range(300)]
    made.append(("synthetic scans + OCR layer, 300 pp", fx.doc(scan_pages, extras=[fx.image("/DCTDecode"), fx.image("/JBIG2Decode", 2550, 3300, None, 0, "/ImageMask true")])))
    mrc_pages = [{"content": "q 612 0 0 792 0 0 cm /Im1 Do Q\nq 612 0 0 792 0 0 cm /M1 Do Q\n", "resources": res} for _ in range(100)]
    made.append(("synthetic MRC scans, 100 pp", fx.doc(mrc_pages, extras=[fx.image("/DCTDecode"), fx.image("/JBIG2Decode", 2550, 3300, None, 0, "/ImageMask true")])))
    out = []
    for i, (label, data) in enumerate(made):
        path = os.path.join(folder, "synthetic-%d.pdf" % i)
        with open(path, "wb") as f:
            f.write(data)
        out.append((label, path))
    tiny = os.path.join(folder, "tiny.pdf")
    with open(tiny, "wb") as f:
        f.write(fx.doc([{"content": fx.words(10)}]))
    return out, tiny


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bin", required=True, help="folder with pdftotext, pdfpagefacts, pdfinfo")
    ap.add_argument("--pdftomarkdown", required=True)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--synthetic", action="store_true")
    ap.add_argument("--cold-runs", type=int, default=21)
    ap.add_argument("files", nargs="*")
    a = ap.parse_args()
    b = os.path.abspath(a.bin)
    md = os.path.abspath(a.pdftomarkdown)
    env = dict(os.environ, PDFTOTEXT=os.path.join(b, "pdftotext"))
    folder = tempfile.mkdtemp(prefix="page-facts-bench.")
    try:
        build_harness(folder)
        if not HARNESS:
            print("(no cc: peak RSS includes Python's)\n")
        inputs = [(os.path.basename(f), os.path.abspath(f)) for f in a.files]
        tiny = None
        if a.synthetic:
            syn, tiny = synthetic(folder)
            inputs += syn
        facts_out = os.path.join(folder, "facts.jsonl")

        def md_cmd(path, *extra):
            return [md, "--stream"] + list(extra) + [path]

        def with_facts(path, level, *extra):
            # --page-facts needs a new file each run: descriptor 5 (/dev/null, opened by the
            # harness) takes the same writes
            if HARNESS:
                return [md, "--stream", "--page-facts-fd", "5", "--page-facts-level", level] + list(extra) + [path]
            return ["sh", "-c", 'exec "$0" "$@" 5>/dev/null', md, "--stream", "--page-facts-fd", "5", "--page-facts-level", level] + list(extra) + [path]

        commands = [
            ("pdfinfo", lambda p: [os.path.join(b, "pdfinfo"), p]),
            ("pdftotext pass (as pdftomarkdown runs it)", lambda p: [os.path.join(b, "pdftotext"), "-q", "-bbox-layout", "-wordmeta", "-rules", "-compact-layout", p, "-"]),
            ("pdfpagefacts basic", lambda p: [os.path.join(b, "pdfpagefacts"), "-level", "basic", p]),
            ("pdfpagefacts full", lambda p: [os.path.join(b, "pdfpagefacts"), "-level", "full", p]),
            ("pdftomarkdown --stream", lambda p: md_cmd(p)),
            ("  + --page-facts basic", lambda p: with_facts(p, "basic")),
            ("  + --page-facts full", lambda p: with_facts(p, "full")),
            ("pdftomarkdown --stream --markers", lambda p: md_cmd(p, "--markers")),
            ("  + --page-facts basic ", lambda p: with_facts(p, "basic", "--markers")),
            ("  + --page-facts full ", lambda p: with_facts(p, "full", "--markers")),
        ]
        print("# Page facts benchmark\n")
        print("Median of %d interleaved runs. CPU = user+sys of the process and its children; RSS = peak of any of them.\n" % a.runs)
        for label, path in inputs:
            n = pages_of(b, path)
            results = {name: [] for name, _ in commands}
            for _ in range(a.runs):
                for name, make in commands:
                    results[name].append(run(make(path), env))
            print("## %s (%d pages)\n" % (label, n))
            print("| command | wall s | CPU s | pages / CPU s | peak RSS MiB | CPU vs reference |")
            print("|---|---:|---:|---:|---:|---:|")
            ref = {}
            for name, _ in commands:
                rs = results[name]
                if any(r[3] != 0 for r in rs):
                    print("| %s | failed (exit %s) | | | | |" % (name.strip(), sorted(set(r[3] for r in rs))))
                    continue
                wall = statistics.median(r[0] for r in rs)
                cpu = statistics.median(r[1] for r in rs)
                rss = statistics.median(r[2] for r in rs) / 1024
                if not name.startswith("  "):
                    ref["base"] = cpu
                if name.startswith("pdfpagefacts"):
                    rel = "%.2f x pdftotext pass" % (cpu / ref["pass"]) if ref.get("pass") else ""
                elif name.startswith("  "):
                    rel = "%+.1f %%" % (100 * (cpu / ref["base"] - 1)) if ref.get("base") else ""
                else:
                    rel = ""
                if name.startswith("pdftotext pass"):
                    ref["pass"] = cpu
                print("| %s | %.3f | %.3f | %.0f | %.1f | %s |" % (name.strip(), wall, cpu, n / cpu if cpu > 0 else 0, rss, rel))
            print()
        if tiny:
            cold = {}
            for _ in range(a.cold_runs):
                for name, cmd in (
                    ("pdfinfo", [os.path.join(b, "pdfinfo"), tiny]),
                    ("pdfpagefacts basic", [os.path.join(b, "pdfpagefacts"), tiny]),
                    ("pdfpagefacts full", [os.path.join(b, "pdfpagefacts"), "-level", "full", tiny]),
                    ("pdftomarkdown", [md, tiny]),
                ):
                    cold.setdefault(name, []).append(run(cmd, env))
            print("## Cold start (1-page PDF, median of %d)\n" % a.cold_runs)
            print("| command | wall ms | CPU ms | peak RSS MiB |")
            print("|---|---:|---:|---:|")
            for name, rs in cold.items():
                print("| %s | %.1f | %.1f | %.1f |" % (name, 1000 * statistics.median(r[0] for r in rs), 1000 * statistics.median(r[1] for r in rs), statistics.median(r[2] for r in rs) / 1024))
            print()
    finally:
        shutil.rmtree(folder, ignore_errors=True)


if __name__ == "__main__":
    main()
