import importlib.util,json,subprocess,concurrent.futures
from pathlib import Path
D=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('screen',D.parent/'screen.py');s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
s.BASE=__import__('os').environ['BASELINE_BINARY']
def one(cid):
 pdf=str(s.W/s.cat[cid]['asset']['path'])
 raw=subprocess.run([s.PT,'-q','-bbox-layout','-compact-layout','-wordmeta','-rules',pdf,'-'],capture_output=True,check=True).stdout
 results={}
 for arm,binary,flags in [('candidate-html',s.DM,['--html']),('baseline-html',s.BASE,['--html']),('candidate-gfm',s.DM,[]),('baseline-gfm',s.BASE,[])]:
  p=subprocess.run([binary,*flags,'--report'],input=raw,capture_output=True,check=True)
  dest=D/arm;dest.mkdir(exist_ok=True)
  (dest/(cid+'.md')).write_bytes(p.stdout)
  report=json.loads(p.stderr);(dest/(cid+'.json')).write_text(json.dumps(report))
  results[arm]=(p.stdout,report)
 return dict(case=cid,degraded=results['candidate-html'][1]['degraded'],html_changed=results['candidate-html'][0]!=results['baseline-html'][0],gfm_changed=results['candidate-gfm'][0]!=results['baseline-gfm'][0])
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool: rows=list(pool.map(one,s.ids))
(D/'comparison.json').write_text(json.dumps(rows,indent=2))
print('cases',len(rows),'degraded',sum(bool(r['degraded']) for r in rows),'html changed',sum(r['html_changed'] for r in rows),'gfm changed',sum(r['gfm_changed'] for r in rows),flush=True)
