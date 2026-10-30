"""Check chunked output identity against the scored whole-document arms."""
import importlib.util,json,subprocess
from pathlib import Path
D=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('screen',D/'screen.py');s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
rows=[]
for cid in s.ids:
    pdf=str(s.W/s.cat[cid]['asset']['path'])
    whole_raw=subprocess.run([s.PT,'-bbox-layout','-compact-layout','-wordmeta','-rules','-enc','UTF-8','-eol','unix',pdf,'-'],capture_output=True,check=True).stdout
    count=whole_raw.count(b'<page ')
    for arm,(_,flags) in s.ARMS.items():
        binary=s.BASE if arm.startswith('baseline') else s.DM
        whole=subprocess.run([binary,*flags],input=whole_raw,capture_output=True,check=True).stdout
        chunks=[]
        for first in range(1,count+1,32):
            last=min(first+31,count)
            raw=subprocess.run([s.PT,'-bbox-layout','-compact-layout','-wordmeta','-rules','-f',str(first),'-l',str(last),'-enc','UTF-8','-eol','unix',pdf,'-'],capture_output=True,check=True).stdout
            out=subprocess.run([binary,*flags,'--params',json.dumps({'furniture_min_share':0.4*count/(last-first+1)})],input=raw,capture_output=True,check=True).stdout
            assert out.count(b'\f')==last-first
            chunks.append(out)
        merged=b'\f'.join(chunks)
        dest=D/'outputs'/arm;dest.mkdir(parents=True,exist_ok=True);(dest/(cid+'.md')).write_bytes(merged)
        rows.append(dict(case=cid,arm=arm,pages=count,identical=whole==merged))
(D/'chunk-identity.json').write_text(json.dumps(rows,indent=2))
print(len(rows),'arms;',sum(not r['identical'] for r in rows),'chunk/whole differences')
