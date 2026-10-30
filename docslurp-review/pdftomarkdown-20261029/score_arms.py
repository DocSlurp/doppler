"""Score pdftotext layout precision and table format arms with the v3.6 judge (scratch)."""
import importlib.util, json, multiprocessing, resource, subprocess, sys, time
from pathlib import Path
D = Path(__file__).parent
spec = importlib.util.spec_from_file_location('judge36', D / 'judge36.py'); judge = importlib.util.module_from_spec(spec); spec.loader.exec_module(judge)
judge.COMPONENTS.update(tol_tables='tableTolerantScore', subst='substitutionRate')
W = judge.DATASET
PT = '/home/dan/worktrees/poppler/pdftomarkdown/build/utils/pdftotext'
DM = '/home/dan/worktrees/poppler/pdftomarkdown/docslurp/doppler-md/target/release/doppler-md'
ARMS = {'strict-gfm': ([], []), 'compact-gfm': (['-compact-layout'], []), 'strict-html': ([], ['--html']), 'compact-html': (['-compact-layout'], ['--html'])}
cat = judge.catalog()
ids = sorted(p.stem for p in Path('/home/dan/data/docslurp-evals/doppler-md-golden/raw').glob('*.raw'))
ids = [i for i in ids if i in cat and judge.gold_pages(cat[i])]
assert len(ids) == 254, len(ids)
def one(cid):
    case = cat[cid]; ref = judge.reference(case); pdf = str(W / case['asset']['path'])
    xh, cpu, rows, memo = {}, {}, {}, {}
    for prec in ([], ['-compact-layout']):
        s = resource.getrusage(resource.RUSAGE_CHILDREN)
        xh[tuple(prec)] = subprocess.run([PT, '-q', '-bbox-layout', '-wordmeta', '-rules', *prec, pdf, '-'], capture_output=True, check=True).stdout
        e = resource.getrusage(resource.RUSAGE_CHILDREN); cpu[tuple(prec)] = e.ru_utime - s.ru_utime + e.ru_stime - s.ru_stime
    for arm, (prec, fmt) in ARMS.items():
        md = subprocess.run([DM, *fmt], input=xh[tuple(prec)], capture_output=True, check=True).stdout.decode()
        sel = judge.select_pages(case, md); assert sel is not None, (cid, arm)
        if sel not in memo: memo[sel] = judge.score_case(ref, sel)
        rows[arm] = dict(memo[sel], case=cid, level=judge.level(cid))
    return rows, cpu
if __name__ == '__main__':
    t = time.time(); out = []
    with multiprocessing.get_context('fork').Pool(6) as pool:
        for r in pool.imap_unordered(one, ids): out.append(r)
    res = {arm: judge.summarize([r[0][arm] for r in out]) for arm in ARMS}
    lv = {arm: {} for arm in ARMS}
    for arm in ARMS:
        g = {}
        for r in out: g.setdefault(r[0][arm]['level'], []).append(r[0][arm])
        lv[arm] = {k: judge.summarize(v)['quality'] for k, v in sorted(g.items())}
    cpu = {k: sum(r[1][k] for r in out) for k in [(), ('-compact-layout',)]}
    keys = ['quality', 'content', 'tables', 'tol_tables', 'math', 'headings', 'halluc', 'subst']
    for arm in ARMS: print(f"{arm:13s}", ' '.join(f"{k}={res[arm][k]}" for k in keys), '| levels', lv[arm])
    print('changed cases vs strict-gfm:', {a: sum(r[0][a] != {**r[0]['strict-gfm']} for r in out) for a in ARMS})
    print(f"pdftotext CPU (parallel, screening): strict {cpu[()]:.2f}s compact {cpu[('-compact-layout',)]:.2f}s; {time.time()-t:.0f}s wall")
    json.dump({'summary': res, 'levels': lv, 'rows': [r[0] for r in out]}, open(D / 'eval36.json', 'w'))
