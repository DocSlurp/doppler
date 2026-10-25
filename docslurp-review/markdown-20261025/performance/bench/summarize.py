#!/usr/bin/env python3
"""summarize.py IN.jsonl -> TSV: median/min CPU (user+sys), median wall over ok runs. Failed labels are listed, never averaged."""
import sys, json, statistics as st, collections

rows = collections.defaultdict(list)
for l in open(sys.argv[1]):
    r = json.loads(l)
    rows[r["label"]].append(r)
print("label\truns_ok/total\tcpu_med_s\tcpu_min_s\twall_med_s\tmaxrss_mb\tstdout_bytes\tload1_max")
for k, rs in rows.items():
    ok = [r for r in rs if r["ok"]]
    if not ok:
        print(f"{k}\t0/{len(rs)}\tFAILED rc={rs[0]['rc']} {rs[0]['stderr_head']!r}")
        continue
    cpu = [r["user"] + r["sys"] for r in ok]
    print("%s\t%d/%d\t%.3f\t%.3f\t%.3f\t%.0f\t%d\t%s" % (
        k, len(ok), len(rs), st.median(cpu), min(cpu), st.median(r["wall"] for r in ok),
        max(r["maxrss_mb"] for r in ok), ok[0]["stdout_bytes"], max(r["load1"] for r in rs)))
