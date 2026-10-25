#!/usr/bin/env python3
"""Interleaved CPU-time benchmark for poppler utils.

Usage:
  bench.py --runs 5 'NAME=cmd with {out}' 'NAME2=cmd2 with {out}' ...

Each command runs in its own scratch directory under $TMPDIR; {out} expands to
that directory, so commands can write
files with e.g. `pdftoppm -png in.pdf {out}/p`.  Commands are run round-robin
(A B C A B C ...) so machine load drifts hit all of them equally.  Reports the
median user CPU seconds (children, user+sys is also shown), the output size
in bytes, and wall time.  Output directories are deleted after each run.
Never commit corpus PDFs or outputs.
"""
import argparse, os, resource, shlex, shutil, statistics, subprocess, tempfile, time


def run_once(cmd):
    d = tempfile.mkdtemp(prefix="bench.", dir=os.environ.get("TMPDIR"))
    try:
        argv = shlex.split(cmd.replace("{out}", d))
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        t0 = time.monotonic()
        with open(os.devnull, "wb") as dn:
            p = subprocess.run(argv, stdout=dn, stderr=dn)
        wall = time.monotonic() - t0
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        size = 0
        for root, _, files in os.walk(d):
            for f in files:
                size += os.path.getsize(os.path.join(root, f))
        return dict(user=after.ru_utime - before.ru_utime, sys=after.ru_stime - before.ru_stime, wall=wall, size=size, rc=p.returncode)
    finally:
        shutil.rmtree(d, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("items", nargs="+", help="alternating: --label handled below")
    args, rest = ap.parse_known_args()
    # simple pair parsing: LABEL=CMD
    specs = []
    for it in args.items + rest:
        label, _, cmd = it.partition("=")
        specs.append((label, cmd))
    res = {l: [] for l, _ in specs}
    for _ in range(args.runs):
        for label, cmd in specs:
            res[label].append(run_once(cmd))
    print("%-28s %9s %9s %9s %12s %s" % ("label", "user_med", "sys_med", "wall_med", "size_bytes", "rc"))
    for label, _ in specs:
        r = res[label]
        print("%-28s %9.2f %9.2f %9.2f %12d %s" % (label, statistics.median(x["user"] for x in r), statistics.median(x["sys"] for x in r),
              statistics.median(x["wall"] for x in r), r[-1]["size"], sorted(set(x["rc"] for x in r))))
        print("   user runs:", " ".join("%.2f" % x["user"] for x in r))


if __name__ == "__main__":
    main()
