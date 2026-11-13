"""Score furniture-window x body-sample arms with the v3.6 judge on the 254 wild-v2 cases
(doppler-md over the saved golden XHTML). Usage: DOPPLER_GOLDEN=DIR score_windows.py DOPPLER_MD OUT.json
DOPPLER_GOLDEN is the golden set directory (it holds raw/*.raw), as for scripts/parity.sh."""
import importlib.util, json, multiprocessing, os, subprocess, sys
from pathlib import Path
D = Path(__file__).parent
spec = importlib.util.spec_from_file_location('judge36', D.parent / 'html-spans-20261030' / 'judge36.py'); judge = importlib.util.module_from_spec(spec); spec.loader.exec_module(judge)
judge.COMPONENTS.update(tol_tables='tableTolerantScore', subst='substitutionRate')
DM, OUT = sys.argv[1], sys.argv[2]
RAW = Path(os.environ['DOPPLER_GOLDEN']) / 'raw'
ARMS = {'whole': []}
for w in [2, 4, 6, 8, 16]:
    for b in [None, 16, 32]:
        ARMS[f'w{w}-b{b or "cur"}'] = ['--window', str(w)] + (['--body-pages', str(b)] if b else [])
cat = judge.catalog()
ids = sorted(p.stem for p in RAW.glob('*.raw'))
ids = [i for i in ids if i in cat and judge.gold_pages(cat[i])]
assert len(ids) == 254, len(ids)
def one(cid):
    case = cat[cid]; ref = judge.reference(case); rows, memo = {}, {}
    for arm, args in ARMS.items():
        md = subprocess.run([DM, *args, str(RAW / f'{cid}.raw')], capture_output=True, check=True).stdout.decode()
        sel = judge.select_pages(case, md); assert sel is not None, (cid, arm)
        if sel not in memo: memo[sel] = judge.score_case(ref, sel)
        rows[arm] = dict(memo[sel], case=cid, level=judge.level(cid), changed=sel != next(iter(memo)))
    return rows
if __name__ == '__main__':
    with multiprocessing.get_context('fork').Pool(3) as pool:
        out = list(pool.imap_unordered(one, ids))
    res = {arm: judge.summarize([r[arm] for r in out]) for arm in ARMS}
    keys = ['quality', 'content', 'tables', 'tol_tables', 'headings', 'halluc', 'subst']
    print('| arm | ' + ' | '.join(keys) + ' | cases changed vs whole |')
    print('|---|' + '---:|' * (len(keys) + 1))
    for arm in ARMS:
        print(f'| {arm} | ' + ' | '.join(str(res[arm][k]) for k in keys) + f" | {sum(r[arm]['changed'] for r in out)} |")
    json.dump({'summary': res, 'rows': out}, open(OUT, 'w'))
