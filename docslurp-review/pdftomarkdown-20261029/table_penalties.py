import importlib.util, subprocess, collections, sys
from pathlib import Path
D = Path(__file__).parent
spec = importlib.util.spec_from_file_location('judge36', D / 'judge36.py'); judge = importlib.util.module_from_spec(spec); spec.loader.exec_module(judge)
import tolerant_table as tt
PT = '/home/dan/worktrees/poppler/pdftomarkdown/build/utils/pdftotext'; DM = '/home/dan/worktrees/poppler/pdftomarkdown/docslurp/doppler-md/target/release/doppler-md'
cat = judge.catalog()
ids = sorted(p.stem for p in Path('/home/dan/data/docslurp-evals/doppler-md-golden/raw').glob('*.raw'))
ids = [i for i in ids if i in cat and judge.gold_pages(cat[i]) and judge.reference(cat[i])['tables']]
tot = {'gfm': collections.Counter(), 'html': collections.Counter()}; sc = {'gfm': [], 'html': []}; worst = []
for cid in ids:
    case = cat[cid]; ref = judge.reference(case)
    x = subprocess.run([PT, '-q', '-bbox-layout', '-wordmeta', '-rules', str(judge.DATASET / case['asset']['path']), '-'], capture_output=True, check=True).stdout
    r = {}
    for arm, fl in (('gfm', []), ('html', ['--html'])):
        md = judge.select_pages(case, subprocess.run([DM, *fl], input=x, capture_output=True, check=True).stdout.decode())
        s = tt.summary(tt.score_markdown(ref, md)); r[arm] = s
        sc[arm].append(s['score'])
        for k, v in (s.get('structureComponents') or {}).items(): tot[arm][k] += v
    worst.append((r['html']['score'] - r['gfm']['score'], cid, r['gfm'].get('contentF1'), r['html'].get('contentF1')))
print('cases', len(ids), 'mean gfm', sum(sc['gfm'])/len(ids), 'html', sum(sc['html'])/len(ids))
keys = sorted(set(tot['gfm']) | set(tot['html']))
for k in keys:
    if tot['gfm'][k] != tot['html'][k]: print(f'  {k}: gfm {tot["gfm"][k]:.3f} html {tot["html"][k]:.3f}')
for w in sorted(worst)[:6]: print('  worst', w)
