import resource, subprocess, statistics, sys
b, x, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
def cpu(args):
    s = resource.getrusage(resource.RUSAGE_CHILDREN)
    subprocess.run([b + "/doppler-md", *args, x], stdout=subprocess.DEVNULL, check=True)
    e = resource.getrusage(resource.RUSAGE_CHILDREN)
    return e.ru_utime - s.ru_utime + e.ru_stime - s.ru_stime
G, H = [], []
for _ in range(n):
    G.append(cpu([])); H.append(cpu(["--html"]))
m = statistics.median
print(f"gfm {m(G)*1000:.1f}ms [{min(G)*1000:.0f}-{max(G)*1000:.0f}]  html {m(H)*1000:.1f}ms [{min(H)*1000:.0f}-{max(H)*1000:.0f}]  n={n}")
