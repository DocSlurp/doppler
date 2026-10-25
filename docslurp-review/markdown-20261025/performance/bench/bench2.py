#!/usr/bin/env python3
"""bench2: N runs per command; every run appended raw to a JSONL file.
usage: bench2.py OUT.jsonl N LABEL CWD -- cmd args...
- cmd runs with cwd=CWD (scratch dir; pdftoppm/pdftohtml write files there), stdout captured and counted
- records wall, user, sys (RUSAGE_CHILDREN delta), maxrss, rc, stdout bytes, 1-min loadavg at start
- rc!=0 rows are recorded with ok=false; summarize.py excludes them from statistics and lists them as FAILED
"""
import sys, subprocess, resource, time, json

out, n, label, cwd = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
cmd = sys.argv[6:]
with open(out, "a") as f:
    for i in range(n):
        load = open("/proc/loadavg").read().split()[0]
        r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
        t = time.perf_counter()
        p = subprocess.run(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        w = time.perf_counter() - t
        r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
        f.write(json.dumps(dict(label=label, run=i, wall=w, user=r1.ru_utime - r0.ru_utime,
                                sys=r1.ru_stime - r0.ru_stime, maxrss_mb=r1.ru_maxrss / 1024,
                                rc=p.returncode, ok=p.returncode == 0, stdout_bytes=len(p.stdout),
                                stderr_head=p.stderr[:200].decode("utf8", "replace"),
                                load1=load, cmd=" ".join(cmd))) + "\n")
