#!/usr/bin/env python3
"""Prototype region-adaptive Markdown renderer over recorder JSONL events (experiment, not production).
usage: render_md.py events.jsonl [strict|rich]
strict = CommonMark/GFM only (pipe tables, no spans, shapes listed as comments).
rich   = may embed HTML <table> with colspan, <sup>, and inline <svg> fragments for shapes."""
import sys, json, statistics, html
ev=[json.loads(l) for l in open(sys.argv[1])]; prof=sys.argv[2] if len(sys.argv)>2 else "strict"
pages=sorted({e["p"] for e in ev}); out=[]
for e in ev:
    if e["t"]=="txt":
        n=e["f"].lower(); e["b"]=e["b"] or ("bold" in n or "black" in n or "heavy" in n); e["i"]=e["i"] or ("italic" in n or "oblique" in n)  # flags from FontDescriptor are 0 for base-14 (measured)
_all=[round(e["fs"]) for e in ev if e["t"]=="txt" and e["s"].strip()]
DOCBODY=statistics.mode(_all) if _all else 10
def esc(s): return s.replace("\\","\\\\").replace("*","\\*").replace("_","\\_").replace("|","\\|") if prof=="strict" else html.escape(s,quote=False) if False else s.replace("|","\\|")
for pg in pages:
    E=[e for e in ev if e["p"]==pg]
    txt=[e for e in E if e["t"]=="txt" and e["s"].strip()]
    strokes=[e for e in E if e["t"]=="stroke" and e["cls"] in("line","rule")]+[e for e in E if e["t"]=="fill" and e["cls"]=="rule"]
    hl=[e for e in strokes if abs(e["y1"]-e["y0"])<=1.5 and e["x1"]-e["x0"]>20]; vl=[e for e in strokes if abs(e["x1"]-e["x0"])<=1.5 and e["y1"]-e["y0"]>8]
    links=[e for e in E if e["t"]=="link"]; imgs=[e for e in E if e["t"]=="img"]
    rects=[e for e in E if e["t"]=="fill" and e["cls"]=="rect"]
    body=DOCBODY
    # ---- ruled tables: bbox of connected h/v lines
    tables=[]
    if len(hl)>=2 and len(vl)>=2:
        xs=sorted({round(v["x0"]) for v in vl}); ys=sorted({round(h["y0"]) for h in hl})
        tables.append(dict(x0=xs[0],x1=xs[-1],y0=ys[0],y1=ys[-1],xs=xs,ys=ys,kind="ruled"))
    used=set()
    blocks=[]  # (y, md)
    for tb in tables:
        grid=[[[] for _ in tb["xs"][:-1]] for _ in tb["ys"][:-1]]
        for i,t in enumerate(txt):
            if tb["x0"]-2<=t["x0"]<=tb["x1"] and tb["y0"]-2<=t["y0"]<=tb["y1"]+2:
                c=max(k for k,x in enumerate(tb["xs"][:-1]) if t["x0"]>=x-2); r=max(k for k,y in enumerate(tb["ys"][:-1]) if t["y0"]>=y-2) if t["y0"]>tb["ys"][0] else 0
                r=min(r,len(grid)-1); grid[r][c].append(t["s"].strip()); used.add(i)
        # colspan detection: vertical line missing inside this row band
        def spans(r):
            y0,y1=tb["ys"][r],tb["ys"][r+1]; res=[]
            for k,x in enumerate(tb["xs"]): res.append(any(abs(v["x0"]-x)<2 and v["y0"]<=y0+1 and v["y1"]>=y1-1 for v in vl))
            return res
        anyspan=any(not all(spans(r)) for r in range(len(grid)))
        if prof=="rich" and anyspan:
            md="<table>\n"
            for r,row in enumerate(grid):
                md+="<tr>"; sp=spans(r); k=0
                while k<len(row):
                    n=1
                    while k+n<len(row) and not sp[k+n]: n+=1
                    tag="th" if r==0 else "td"; md+=f"<{tag}"+(f" colspan={n}" if n>1 else "")+">"+" ".join(sum(row[k:k+n],[]))+f"</{tag}>"; k+=n
                md+="</tr>\n"
            md+="</table>"
        else:
            # GFM has no spans: repeat a spanning cell's text in every column it
            # covers, so each column keeps its header (table-acceptance.md) and
            # each value stays associated with every column it applies to.
            def flat_row(r,row):
                sp=spans(r); res=[]; k=0
                while k<len(row):
                    n=1
                    while k+n<len(row) and not sp[k+n]: n+=1
                    res+=[" ".join(sum(row[k:k+n],[])).replace("|","\\|")]*n; k+=n
                return res
            md="\n".join(("| "+" | ".join(flat_row(r,row))+" |") + ("\n|"+"---|"*len(row) if r==0 else "") for r,row in enumerate(grid))
            if anyspan: md+="\n<!-- strict: spans flattened, spanning text repeated per column -->"
        blocks.append((tb["y0"],md))
    # ---- lines
    rest=sorted([(i,t) for i,t in enumerate(txt) if i not in used],key=lambda it:(-round(it[1]["fs"]),round(it[1]["y0"]),it[1]["x0"]))
    lines=[]
    for i,t in rest:
        for L in lines:
            if abs(L[0]["y0"]-t["y0"])<0.5*max(L[0]["fs"],t["fs"]) or (t["fs"]<0.85*L[0]["fs"] and 0<L[0]["y0"]-t["y0"]<0.9*L[0]["fs"] and any(abs(t["x0"]-r["x1"])<0.5*L[0]["fs"] for r in L)):
                L.append(t); break
        else: lines.append([t])
    # borderless table: >=3 consecutive lines whose gap-splits give same >=2 columns aligned
    def cells(L):
        L=sorted(L,key=lambda t:t["x0"]); cs=[]
        for t in L:
            if cs and t["x0"]-cs[-1][-1]["x1"]<0.6*t["fs"]: cs[-1].append(t)
            else: cs.append([t])
        return cs
    def render_line(L):
        s=""; prev=None
        for t in sorted(L,key=lambda t:t["x0"]):
            w=esc(t["s"]).rstrip(); 
            sup = prev is not None and t["fs"]<0.85*prev["fs"] and t["y0"]<prev["y0"]-0.2*prev["fs"]
            if sup: w=f"<sup>{w}</sup>" if prof=="rich" else f"^{{{w}}}"
            elif t["b"] and t["i"]: w=f"***{w.strip()}***"
            elif t["b"]: w=f"**{w.strip()}**"
            elif t["i"]: w=f"*{w.strip()}*"
            for l in links:
                if l["x0"]-1<=t["x0"] and t["x1"]<=l["x1"]+1 and l["y0"]-2<=t["y0"]-t["fs"]*0.3<=l["y1"]+2: w=f"[{w}]({l['dst']})"
            s+=(" " if s and not sup and not s.endswith(" ") and not w.startswith(" ") else "")+w; prev=t
        return s.strip()
    i=0; heads=sorted({round(e["fs"]) for e in ev if e["t"]=="txt" and e["s"].strip() and e["fs"]>body*1.2 and len(e["s"])<80},reverse=True)
    lines.sort(key=lambda L:L[0]["y0"])
    while i<len(lines):
        L=lines[i]; cs=cells(L); y=L[0]["y0"]
        if len(cs)>=2:
            j=i; sig=[round(c[0]["x0"]/6) for c in cs]
            while j+1<len(lines) and [round(c[0]["x0"]/6) for c in cells(lines[j+1])]==sig: j+=1
            if j-i>=2:
                rows=[[ " ".join(esc(t["s"]).strip() for t in c) for c in cells(lines[k])] for k in range(i,j+1)]
                md="\n".join("| "+" | ".join(r)+" |"+("\n|"+"---|"*len(r) if n==0 else "") for n,r in enumerate(rows))
                md+="\n<!-- borderless table: header row inferred -->"; blocks.append((y,md)); i=j+1; continue
        fs=round(max(t["fs"] for t in L)); txtl=render_line(L)
        if fs in heads and len(L)<=2: blocks.append((y,"#"*(heads.index(fs)+1)+" "+txtl.replace("**","")))
        else: blocks.append((y,txtl))
        i+=1
    for r in rects:
        if prof=="rich": blocks.append((r["y0"],f'<svg xmlns="http://www.w3.org/2000/svg" width="{r["x1"]-r["x0"]:.0f}" height="{r["y1"]-r["y0"]:.0f}"><rect width="100%" height="100%" fill="#{r["c"]}"/></svg>'))
        else: blocks.append((r["y0"],f'<!-- shape rect {r["x0"]:.0f},{r["y0"]:.0f} {r["x1"]-r["x0"]:.0f}x{r["y1"]-r["y0"]:.0f} #{r["c"]} omitted in strict -->'))
    for im in imgs: blocks.append((im["y0"],f'![image obj {im["obj"]} p{pg} {im["pw"]}x{im["ph"]}](assets/p{pg}-obj{im["obj"]}.png)'))
    out.append(f"<!-- page {pg} -->"); out+= [b for _,b in sorted(blocks,key=lambda x:x[0])]
print("\n\n".join(out))
