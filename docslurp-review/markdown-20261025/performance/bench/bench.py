#!/usr/bin/env python3
"""Tiny benchmark harness: best-of-N wall/user/sys/maxrss per command, TSV output.
usage: bench.py N label -- cmd args...   (stdout -> /dev/null, bytes counted)"""
import sys, os, subprocess, resource, time, json
n=int(sys.argv[1]); label=sys.argv[2]; cmd=sys.argv[4:]
best=None; outbytes=0
for _ in range(n):
    r0=resource.getrusage(resource.RUSAGE_CHILDREN); t=time.perf_counter()
    p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
    w=time.perf_counter()-t; r1=resource.getrusage(resource.RUSAGE_CHILDREN)
    outbytes=len(p.stdout)
    rec=(w, r1.ru_utime-r0.ru_utime, r1.ru_stime-r0.ru_stime, r1.ru_maxrss/1024)
    if best is None or rec[0]<best[0]: best=rec
print("%s\twall=%.3f\tuser=%.3f\tsys=%.3f\tmaxrssMB=%.0f\tstdout=%d\trc=%d"%(label,*best,outbytes,p.returncode))
