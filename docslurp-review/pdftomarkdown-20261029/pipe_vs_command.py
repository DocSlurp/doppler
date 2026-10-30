import resource, subprocess, statistics, sys, os
pt = "/home/dan/code/oss/poppler/build/utils/pdftotext"
bin = "/home/dan/worktrees/poppler/pdftomarkdown/docslurp/doppler-md/target/release/"
pdf = sys.argv[1]
env = dict(os.environ, PDFTOTEXT=pt)
def cpu(cmd, shell=False):
    b = resource.getrusage(resource.RUSAGE_CHILDREN)
    subprocess.run(cmd, shell=shell, env=env, stdout=subprocess.DEVNULL, check=True)
    a = resource.getrusage(resource.RUSAGE_CHILDREN)
    return a.ru_utime - b.ru_utime + a.ru_stime - b.ru_stime
pipe = f"{pt} -q -bbox-layout -wordmeta -rules '{pdf}' - | {bin}doppler-md"
P, O, H = [], [], []
for _ in range(9):
    P.append(cpu(pipe, shell=True)); O.append(cpu([bin + "pdftomarkdown", pdf])); H.append(cpu([bin + "pdftomarkdown", "--html", pdf]))
m = statistics.median
print(f"pipe {m(P):.3f}s  pdftomarkdown {m(O):.3f}s  --html {m(H):.3f}s  (user+sys CPU, median of 9)")
