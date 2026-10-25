#!/usr/bin/env python3
"""Best-of-3 wall time of existing poppler CLI modes. usage: bench.py BIN_DIR OUT.tsv  (BIN_DIR '' = PATH)"""
import subprocess, sys, time, os, tempfile, shutil
bindir, out = sys.argv[1], sys.argv[2]
D = os.path.expanduser('~/.cache/docslurp/pricing-samples-2026-10-21')
pdfs = [f'{D}/sec-10k/aapl-10k-fy2025-as-filed.pdf', f'{D}/uscode-title/USCODE-2024-title1.pdf', f'{D}/scanned-document/inauguralofourfi00newy.pdf', os.path.join(os.path.dirname(os.path.abspath(__file__)),'fixtures/synth.pdf')]
b = lambda n: os.path.join(bindir, n) if bindir else n
modes = {
 'pdftotext-plain': lambda f: [b('pdftotext'), f, '-'],
 'pdftotext-layout': lambda f: [b('pdftotext'), '-layout', f, '-'],
 'pdftotext-bbox-layout': lambda f: [b('pdftotext'), '-bbox-layout', f, '-'],
 'pdftotext-tsv': lambda f: [b('pdftotext'), '-tsv', f, '-'],
 'pdftohtml-xml-noimg': lambda f: [b('pdftohtml'), '-xml', '-stdout', '-i', f],
 'pdftohtml-xml-img': lambda f: [b('pdftohtml'), '-xml', '-stdout', f],
 'pdfimages-list': lambda f: [b('pdfimages'), '-list', f],
}
w = tempfile.mkdtemp(dir=os.environ.get('TMPDIR','/home/dan/tmp/agents'))
with open(out,'w') as o:
    o.write('pdf\tmode\tbest_wall_s\tstdout_bytes\n')
    for f in pdfs:
        for m, c in modes.items():
            best, n = 1e9, 0
            for _ in range(3):
                s = time.perf_counter(); r = subprocess.run(c(f), cwd=w, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL); e = time.perf_counter()
                best = min(best, e-s); n = len(r.stdout)
            o.write(f'{os.path.basename(f)}\t{m}\t{best:.3f}\t{n}\n'); o.flush()
shutil.rmtree(w)
