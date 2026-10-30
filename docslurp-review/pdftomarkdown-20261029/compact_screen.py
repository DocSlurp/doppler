import glob, os, resource, subprocess, sys, hashlib
pt = "/home/dan/worktrees/poppler/pdftomarkdown/build/utils/pdftotext"
dm = "/home/dan/worktrees/poppler/pdftomarkdown/docslurp/doppler-md/target/release/doppler-md"
pdfs = sorted(glob.glob("/home/dan/tmp/docslurp-*/docslurp-source-cache-*.pdf")) + sorted(glob.glob(os.path.expanduser("~/.cache/docslurp/pricing-samples-2026-10-21/*/*.pdf"))) + ["/usr/share/doc/ghostscript/Ghostscript.pdf"]
def run(pdf, extra):
    s = resource.getrusage(resource.RUSAGE_CHILDREN)
    x = subprocess.run([pt, "-q", "-bbox-layout", "-wordmeta", "-rules", *extra, pdf, "-"], capture_output=True, timeout=600)
    e = resource.getrusage(resource.RUSAGE_CHILDREN)
    t = e.ru_utime - s.ru_utime + e.ru_stime - s.ru_stime
    if x.returncode: return None, t, None
    out = {}
    for mode in ([], ["--html"]):
        m = subprocess.run([dm, *mode], input=x.stdout, capture_output=True)
        out[tuple(mode)] = (m.returncode, m.stdout)
    return out, t, len(x.stdout)
same = diff = fail = 0; tb = tc = 0.0; bb = bc = 0; diffs = []
for pdf in pdfs:
    a, ta, la = run(pdf, []); b, tb2, lb = run(pdf, ["-compact-layout"])
    if a is None or b is None: fail += 1; continue
    tb += ta; tc += tb2; bb += la; bc += lb
    if a == b: same += 1
    else: diff += 1; diffs.append(os.path.basename(pdf))
print(f"pdfs={len(pdfs)} identical={same} differ={diff} failed={fail}")
print(f"pdftotext CPU: ordinary {tb:.2f}s compact {tc:.2f}s ({(tc/tb-1)*100:+.1f}%); XHTML bytes {bb/1e6:.0f}MB -> {bc/1e6:.0f}MB")
print("differ:", diffs[:20])
