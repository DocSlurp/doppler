#!/usr/bin/env python3 -I
"""Word-level gutter probe on one page of a PDF: uses `pdftotext -bbox-layout -f P -l P` words (system poppler),
clusters word right-edges/left-edges of numeric words into columns; prints columns + rows with >=3 columns hit.
Usage: gutter_probe.py file.pdf page"""
import re,subprocess,sys
pdf,pg=sys.argv[1],sys.argv[2]
x=subprocess.run(["pdftotext","-bbox-layout","-f",pg,"-l",pg,pdf,"-"],capture_output=True,text=True).stdout
W=[(float(a),float(b),float(c),float(d),t) for a,b,c,d,t in re.findall(r'<word xMin="([\d.]+)" yMin="([\d.]+)" xMax="([\d.]+)" yMax="([\d.]+)">([^<]*)</word>',x)]
num=[w for w in W if re.fullmatch(r'[\$\(]?[\d,.]+\)?%?',w[4])]
# right-aligned numeric columns: cluster x1 within 3pt
ed=sorted(w[2] for w in num); cols=[]
for e in ed:
    if cols and e-cols[-1][-1]<=3: cols[-1].append(e)
    else: cols.append([e])
cols=[(round(sum(c)/len(c),1),len(c)) for c in cols if len(c)>=5]
print("words",len(W),"numeric",len(num),"right-edge column candidates (x1,count):",cols)
rows={}
for w in W: rows.setdefault(round((w[1]+w[3])/2/3),[]).append(w)
hit=0
for k,ws in sorted(rows.items()):
    ws.sort(); ncol=sum(1 for c,_ in cols if any(abs(w[2]-c)<=3 for w in ws if w in num))
    if ncol>=2: hit+=1
print("rows with >=2 numeric column hits:",hit,"of",len(rows))
