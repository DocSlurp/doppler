#!/usr/bin/env python3 -I
"""Figure region probe: cluster non-table vector paths + image placements into regions, snap in nearby text (labels),
crop-render each with pdftocairo. Usage: figures.py in.pdf outdir"""
import re, subprocess, sys, os, importlib.util
spec=importlib.util.spec_from_file_location("mp",os.path.join(os.path.dirname(__file__),"md_proto.py")); mp=importlib.util.module_from_spec(spec); spec.loader.exec_module(mp)
pdf,out=sys.argv[1],sys.argv[2]; os.makedirs(out,exist_ok=True)
segs,rects,_=mp.load_svg(pdf); runs=mp.load_runs(pdf)
svg=mp.sh("pdftocairo","-svg",pdf,"-")
items=[]  # (x0,y0,x1,y1,kind)
for r in rects:
    if r['col'].startswith('100%, 100%, 100%'): continue
    items.append((r['x0'],r['y0'],r['x1'],r['y1'],'fill'))
for s in segs: items.append((s[1],s[2],s[3],s[4],'rule'))
for m in re.finditer(r'<image[^>]*?width="(\d+)" height="(\d+)"[^>]*?(?:transform="matrix\(([^)]*)\)")?[^>]*?>',svg):
    pass
# image placements: pdftocairo wraps images in <use> with transform; take from the <image ... transform> attr
for m in re.finditer(r'<g[^>]*clip-path[^>]*>\s*<use[^>]*xlink:href="#image[^"]*"[^>]*transform="matrix\(([^)]*)\)"',svg): pass
dims={m.group(1):(float(m.group(2)),float(m.group(3))) for m in re.finditer(r'<image id="([^"]+)"[^>]*?width="(\d+)" height="(\d+)"',svg)}
for m in re.finditer(r'<use xlink:href="#(source-\d+)" transform="matrix\(([-\d.]+), ([-\d.]+), ([-\d.]+), ([-\d.]+), ([-\d.]+), ([-\d.]+)\)"',svg):
    w,h=dims[m.group(1)]; a,b,c,d,e,f=[float(v) for v in m.groups()[1:]]
    items.append((e,f,e+w*abs(a),f+h*abs(d),'image'))
def merge(items,gap=10):
    boxes=[list(i[:4])+[{i[4]}] for i in items]; ch=True
    while ch:
        ch=False
        for i in range(len(boxes)):
            for j in range(i+1,len(boxes)):
                a,b=boxes[i],boxes[j]
                if a[0]-gap<=b[2] and b[0]-gap<=a[2] and a[1]-gap<=b[3] and b[1]-gap<=a[3]:
                    boxes[i]=[min(a[0],b[0]),min(a[1],b[1]),max(a[2],b[2]),max(a[3],b[3]),a[4]|b[4]]; del boxes[j]; ch=True; break
            if ch: break
    return boxes
res=[]
for n,b in enumerate(merge(items),1):
    x0,y0,x1,y1,kinds=b
    labels=[r['text'] for r in runs if x0-2<=(r['x0']+r['x1'])/2<=x1+2 and y0-2<=(r['y0']+r['y1'])/2<=y1+14]
    # score: single hairline rules are decoration, not figures
    score=len([1 for i in items if x0<=i[0] and i[2]<=x1 and y0<=i[1] and i[3]<=y1 and i[4]!='rule' or (x0<=i[0] and i[2]<=x1 and y0<=i[1] and i[3]<=y1)])
    png=f"{out}/fig{n}"
    subprocess.run(["pdftocairo","-png","-singlefile","-r","110","-x",str(int(x0*110/72-4)),"-y",str(int(y0*110/72-4)),"-W",str(int((x1-x0)*110/72+8)),"-H",str(int((y1-y0)*110/72+8)),pdf,png],check=True)
    res.append((n,(round(x0),round(y0),round(x1),round(y1)),sorted(kinds),score,labels))
    print(f"![Figure {n}: labels={', '.join(labels)}]({png}.png)  <!-- bbox={res[-1][1]} kinds={sorted(kinds)} elements={score} -->")
