import resource, subprocess, statistics, sys, os
B = "/home/dan/worktrees/poppler/pdftomarkdown/docslurp/doppler-md/target/release/pdftomarkdown"
env = dict(os.environ, PDFTOTEXT="/home/dan/worktrees/poppler/pdftomarkdown/build/utils/pdftotext")
def cpu(args):
    s = resource.getrusage(resource.RUSAGE_CHILDREN)
    subprocess.run([B, *args], env=env, stdout=subprocess.DEVNULL, check=True)
    e = resource.getrusage(resource.RUSAGE_CHILDREN)
    return e.ru_utime - s.ru_utime + e.ru_stime - s.ru_stime
for pdf in sys.argv[1:]:
    S, C = [], []
    for i in range(7):
        if i % 2: S.append(cpu(["--strict", pdf])); C.append(cpu([pdf]))
        else: C.append(cpu([pdf])); S.append(cpu(["--strict", pdf]))
    m = statistics.median
    print(f"{os.path.basename(pdf)[:40]:40s} strict {m(S):.3f}s [{min(S):.3f}-{max(S):.3f}]  compact {m(C):.3f}s [{min(C):.3f}-{max(C):.3f}]  {(m(C)/m(S)-1)*100:+.1f}%")
