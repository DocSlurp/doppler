"""--stream old vs new: CPU (process tree), peak RSS (largest child) and time to first output byte.
Usage: stream_cost.py OLD NEW REPS PDF..."""
import os, statistics, subprocess, sys, time
OLD, NEW, REPS, pdfs = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4:]
def one(exe, pdf):
    t = time.monotonic()
    p = subprocess.Popen([exe, '--stream', pdf], stdout=subprocess.PIPE)
    p.stdout.read(1); first = time.monotonic() - t
    p.stdout.read()
    _, _, ru = os.wait4(p.pid, 0)
    return ru.ru_utime + ru.ru_stime, ru.ru_maxrss / 1024, first
for pdf in pdfs:
    r = {OLD: [], NEW: []}
    for i in range(REPS):
        for exe in ((OLD, NEW) if i % 2 == 0 else (NEW, OLD)):
            r[exe].append(one(exe, pdf))
    med = {e: [statistics.median(x[k] for x in v) for k in range(3)] for e, v in r.items()}
    print(f"{os.path.basename(pdf)[:20]}: old CPU {med[OLD][0]:.2f}s RSS {med[OLD][1]:.0f}MiB first {med[OLD][2]:.2f}s | new CPU {med[NEW][0]:.2f}s RSS {med[NEW][1]:.0f}MiB first {med[NEW][2]:.2f}s", flush=True)
