#!/usr/bin/env python3 -I
"""Editable inline-SVG fragments per figure region (paths from pdftocairo -svg + <text> labels from pdftohtml runs).
Usage: svgfrag.py in.pdf outdir x0 y0 x1 y1 [name]   (bbox in pt, top-down; take it from figures.py output)"""
import sys, os, re, importlib.util
sp=importlib.util.spec_from_file_location("mp",os.path.join(os.path.dirname(__file__),"md_proto.py")); mp=importlib.util.module_from_spec(sp); sp.loader.exec_module(mp)
pdf,out=sys.argv[1],sys.argv[2]; x0,y0,x1,y1=map(float,sys.argv[3:7]); name=sys.argv[7] if len(sys.argv)>7 else "frag"
os.makedirs(out,exist_ok=True)
body=mp.sh("pdftocairo","-svg",pdf,"-").split("</defs>")[-1]
els=[]
for p in re.findall(r'<path[^>]*?/>',body):
    mt=re.search(r'transform="matrix\(1, 0, 0, -1, 0, ([\d.]+)\)"',p); d=re.search(r' d="([^"]*)"',p).group(1)
    toks=re.findall(r'[MLCZ]|-?[\d.]+',d); res=[];c=0
    for t in toks:
        if t in 'MLCZ': res.append(t);c=0
        else: v=float(t); res.append(f"{(float(mt.group(1))-v) if (mt and c%2==1) else v:g}"); c+=1
    nums=[float(v) for v in re.findall(r'-?[\d.]+',' '.join(res))]; xs=nums[0::2]; ys=nums[1::2]
    if not xs or not (x0-1<=min(xs) and max(xs)<=x1+1 and y0-1<=min(ys) and max(ys)<=y1+1): continue
    a=dict(re.findall(r'\b(fill|stroke|stroke-width)="([^"]*)"',p))
    a.setdefault('fill','none')
    sa=' '.join(f'{k}="{v}"' for k,v in a.items() if not (k=='stroke-width' and 'stroke' not in a))
    els.append(f'<path d="{" ".join(res)}" {sa}/>')
for r in mp.load_runs(pdf):
    if x0-1<=r['x0'] and r['x1']<=x1+30 and y0-2<=r['y0'] and r['y1']<=y1+14:
        els.append(f'<text x="{r["x0"]}" y="{r["y1"]-2}" font-family="Helvetica,Arial,sans-serif" font-size="{r["size"]:g}">{r["text"]}</text>')
w,h=x1-x0+30,y1-y0+16
frag=f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x0-4:g} {y0-4:g} {w:g} {h:g}" width="{w:g}" height="{h:g}" role="img" aria-label="{name}">\n'+"\n".join(els)+"\n</svg>\n"
open(f"{out}/{name}.svg","w").write(frag); print(name,len(frag),"bytes svg,",len(els),"elements")
