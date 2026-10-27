"""Reproduce local tolerant quality comparison using the frozen authored-reference judge."""
import importlib.util
import json
import multiprocessing
import subprocess
from pathlib import Path

ROOT = Path('/home/dan/code/oss/poppler')
OUT = ROOT / 'docslurp-review/architecture-20261027'
SCORE = Path('/home/dan/code/17th-street-labs/docslurp-app/performance/doppler-2026-10-26/score.py')
spec = importlib.util.spec_from_file_location('frozen_score', SCORE)
judge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(judge)
judge.COMPONENTS.update(tolerant_tables='tableTolerantScore', tolerant_table_content='tableTolerantContentF1')
cat = judge.catalog()
raws = sorted(Path('/home/dan/data/docslurp-evals/doppler-md-golden/raw').glob('*.raw'))
excluded = [{'id': p.stem, 'reason': 'real PDF without a generator-authored Markdown reference'} for p in raws if p.stem not in cat or not judge.gold_pages(cat[p.stem])]
raws = [p for p in raws if p.stem in cat and judge.gold_pages(cat[p.stem])]
assert len(raws) == 254, len(raws)
arms = {'baseline': '/home/dan/worktrees/poppler/doppler-md/docslurp/doppler-md/target/release/doppler-md',
        'candidate': str(ROOT / 'docslurp/doppler-md/target/release/doppler-md')}
result = {'judge_path': str(SCORE), 'reference': 'wild-v2 generator-authored liveMarkdown, frozen judge v3.5', 'cases':len(raws), 'excluded':excluded, 'arms': {}}
def score_pair(raw):
    case=cat[raw.stem]
    ref=judge.reference(case)
    cache={}
    pair={}
    for label,binary in arms.items():
        md=subprocess.check_output([binary,str(raw)],text=True)
        selected=judge.select_pages(case,md)
        assert selected is not None, raw
        # Exact equal predictions can reuse the deterministic score; changed predictions
        # are scored independently. Equality is never a quality acceptance requirement.
        if selected not in cache: cache[selected]=judge.score_case(ref,selected)
        row=dict(cache[selected])
        row.update(case=raw.stem, level=judge.level(raw.stem), cls=case['documentClass'], missing=False, unpaged=False)
        pair[label]=row
    return pair

pairs=[]
with multiprocessing.get_context('fork').Pool(6) as pool:
    for pair in pool.imap_unordered(score_pair,raws):
        pairs.append(pair)
        if len(pairs)%10==0: print('scored',len(pairs),flush=True)
for label in arms:
    rows=sorted((pair[label] for pair in pairs),key=lambda r:r['case'])
    groups={}
    for row in rows: groups.setdefault(row['level'],[]).append(row)
    result['arms'][label]={'all':judge.summarize(rows),'groups':{k:judge.summarize(v) for k,v in groups.items()},'rows':rows}
    print(label,json.dumps(result['arms'][label]['all']),flush=True)
# A skipped-work sentinel must lose content/table quality.
ref=next(judge.reference(cat[p.stem]) for p in raws if judge.reference(cat[p.stem])['tables'])
empty=judge.score_case(ref,'')
assert empty['content']<0.01 and empty['tables']<0.01, empty
result['empty_sentinel']=empty
base,new=result['arms']['baseline'],result['arms']['candidate']
result['all_component_rows_equal']=base['rows']==new['rows']
for key in ['all']:
    assert new[key]['quality'] >= base[key]['quality'] - 0.005
for key in base['groups']:
    assert new['groups'][key]['quality'] >= base['groups'][key]['quality'] - 0.005
(OUT/'quality.json').write_text(json.dumps(result,indent=2)+'\n')
print('quality gates passed; every component row equal:',result['all_component_rows_equal'],flush=True)
