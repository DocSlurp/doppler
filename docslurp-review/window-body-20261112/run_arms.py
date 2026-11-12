"""Run doppler-md over saved pdftotext XHTML for every (furniture window, body sample) arm.
Usage: run_arms.py DM XHTML_DIR OUT_DIR [jobs]. Writes OUT/<doc>/<arm>.md, .rep and cpu.json."""
import json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
DM, XD, OUT = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
JOBS = int(sys.argv[4]) if len(sys.argv) > 4 else 3
WINDOWS = [2, 4, 6, 8, 16]
BODIES = [None, 16, 32, 64]
ARMS = {'whole': []}
for w in WINDOWS:
    for b in BODIES:
        ARMS[f'w{w}-b{b or "cur"}'] = ['--window', str(w)] + (['--body-pages', str(b)] if b else [])
def run(doc, arm, args):
    d = OUT / doc; d.mkdir(parents=True, exist_ok=True)
    md = d / f'{arm}.md'
    if md.exists() and (d / f'{arm}.cpu').exists():
        return
    with open(md, 'wb') as o, open(d / f'{arm}.rep', 'wb') as e:
        pr = subprocess.Popen([DM, '--report', *args, str(XD / f'{doc}.xhtml')], stdout=o, stderr=e)
        _, status, ru = os.wait4(pr.pid, 0)
    assert status == 0, (doc, arm)
    (d / f'{arm}.cpu').write_text(f'{ru.ru_utime} {ru.ru_stime} {ru.ru_maxrss}')
docs = sorted(p.name[:-6] for p in XD.glob('*.xhtml'))
with ThreadPoolExecutor(JOBS) as ex:
    list(ex.map(lambda a: run(*a), [(d, a, args) for d in docs for a, args in ARMS.items()]))
print('done', len(docs), 'docs', len(ARMS), 'arms')
