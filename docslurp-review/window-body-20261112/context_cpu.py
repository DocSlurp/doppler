"""CPU (user+sys of pdftomarkdown and pdftotext) of converting a whole PDF as -f/-l ranges of
L pages with --context C, summed over the ranges; interleaved, median of REPS.
Usage: context_cpu.py PDFTOMARKDOWN REPS PDF..."""
import os, resource, statistics, subprocess, sys
PM, REPS, pdfs = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
def cpu():
    r = resource.getrusage(resource.RUSAGE_CHILDREN); return r.ru_utime + r.ru_stime
def pages(pdf):
    out = subprocess.run([os.environ['PDFINFO'], pdf], capture_output=True, text=True).stdout
    return int(next(l.split()[1] for l in out.splitlines() if l.startswith('Pages:')))
for pdf in pdfs:
    n = pages(pdf)
    arms = [(L, C) for L in (16, 64, 256) for C in (0, 4, 16)]
    res = {a: [] for a in arms}
    whole = []
    for rep in range(REPS):
        t = cpu(); subprocess.run([PM, '--stream', pdf], stdout=subprocess.DEVNULL, check=True); whole.append(cpu() - t)
        for L, C in (arms if rep % 2 == 0 else arms[::-1]):
            t = cpu()
            for f in range(1, n + 1, L):
                subprocess.run([PM, '--stream', '--body-size', '10', '--body-color', '#000000', '-f', str(f), '-l', str(min(n, f + L - 1)), '--context', str(C), pdf], stdout=subprocess.DEVNULL, check=True)
            res[(L, C)].append(cpu() - t)
    w = statistics.median(whole)
    print(f'{os.path.basename(pdf)[:20]} {n} pages: whole --stream {w:.2f}s')
    for L in (16, 64, 256):
        base = statistics.median(res[(L, 0)])
        print(f'  ranges of {L}: context 0 {base:.2f}s, 4 {statistics.median(res[(L, 4)]):.2f}s (+{100*(statistics.median(res[(L, 4)])/base-1):.0f}%), 16 {statistics.median(res[(L, 16)]):.2f}s (+{100*(statistics.median(res[(L, 16)])/base-1):.0f}%)', flush=True)
