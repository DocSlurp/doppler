#!/usr/bin/env python3
import subprocess, sys, time, os, resource, json
S=os.path.expanduser('~/.cache/docslurp/pricing-samples-2026-10-21'); P='/home/dan/tmp/agents/evidence_probe'
files=[S+'/sec-10k/aapl-10k-fy2025-as-filed.pdf', S+'/uscode-title/USCODE-2024-title1.pdf']
HERE=os.path.dirname(os.path.abspath(__file__))+'/..'
def run(cmd, env=None, n=3):
    ts=[]
    for _ in range(n):
        t=time.perf_counter(); subprocess.run(cmd,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=env); ts.append(time.perf_counter()-t)
    return min(ts), sorted(ts)[len(ts)//2]
print('# system tools: pdftotext 26.10.0 (/usr/bin); probe = out-of-tree EvidenceDev vs libpoppler 26.10.0; best/median of 3 wall s')
for f in files:
    print('##', os.path.basename(f), 'bytes', os.path.getsize(f))
    rows=[('pdftotext -raw',['pdftotext','-raw',f,'-']),('pdftotext -layout',['pdftotext','-layout',f,'-']),('pdftotext -bbox-layout',['pdftotext','-bbox-layout',f,'-']),
      ('pdftohtml -xml',['pdftohtml','-xml','-stdout','-i','-q',f]),('probe text-only (needNonText=0)',[P,f]),('probe +images/paths (needNonText=1)',[P,f])]
    for name,cmd in rows:
        env=dict(os.environ); 
        if 'needNonText=0' in name: env['EV_NONTEXT']='0'
        b,m=run(cmd,env); print(f'{name:40s} best {b:6.3f}  median {m:6.3f}')
    out=HERE+'/results/'+os.path.basename(f)[:-4]
    subprocess.run([P,f],stdout=open(out+'.evidence.jsonl','w'),stderr=subprocess.DEVNULL)
    print('evidence bytes', os.path.getsize(out+'.evidence.jsonl'))
    for pr in('gfm','rich'):
        t=time.perf_counter(); subprocess.run(['python3','-I',HERE+'/src/md.py',out+'.evidence.jsonl','--profile',pr],stdout=open(out+'.'+pr+'.md','w')); print(f'md.py(python) {pr}: {time.perf_counter()-t:.3f}s, md bytes {os.path.getsize(out+"."+pr+".md")}')
