#!/usr/bin/env python3 -I
"""Fidelity prototype: PDF -> Markdown using ONLY system poppler outputs as inputs
(pdftohtml -xml -zoom 1 for styled text runs, pdftocairo -svg for rulings/fills/images).
It exists to validate the *algorithms* proposed for an in-process C++ MarkdownOutputDev,
not as the shipping implementation (it is orders of magnitude slower than the proposal).
Usage: md_proto.py file.pdf [--tables auto|gfm|html] """
import re, subprocess, sys, html, collections, math

def sh(*a): return subprocess.run(a, capture_output=True, text=True, check=True).stdout

# ---------------- inputs ----------------
def load_runs(pdf):
    x = sh("pdftohtml","-xml","-zoom","1","-stdout","-i","-noframes",pdf)
    specs = {m.group(1):(float(m.group(2)),m.group(3)) for m in re.finditer(r'<fontspec id="(\d+)" size="([\d.]+)" family="([^"]*)"',x)}
    runs=[]
    for m in re.finditer(r'<text top="(\d+)" left="(\d+)" width="(\d+)" height="(\d+)" font="(\d+)">(.*?)</text>',x):
        t,l,w,h,f,body=m.groups(); size,fam=specs[f]
        txt=html.unescape(re.sub(r'<[^>]+>','',body))
        runs.append(dict(x0=int(l),y0=int(t),x1=int(l)+int(w),y1=int(t)+int(h),size=size,fam=fam,
            bold='<b>' in body, ital='<i>' in body, text=txt, md=md_inline(body), html=html_inline(body)))
    seen=set();u=[]
    for r in runs:
        k=(r['text'],r['x0'],r['y0'])
        if k in seen: r['dup']=True; continue
        seen.add(k);u.append(r)
    return u
def _segments(body):
    """[(text,bold,ital)] from pdftohtml run body (<b>/<i> nest)"""
    out=[];b=i=0
    for tok in re.split(r'(</?[bi]>)',body):
        if tok=='<b>': b+=1
        elif tok=='</b>': b-=1
        elif tok=='<i>': i+=1
        elif tok=='</i>': i-=1
        elif tok: out.append((html.unescape(re.sub(r'<[^>]+>','',tok)),b>0,i>0))
    return out
def _mdesc(t): return re.sub(r'([\\`*_|\[\]<>])',r'\\\1',t)
def md_inline(body):
    """CommonMark-safe: whitespace stays OUTSIDE emphasis delimiters (left/right-flanking rule)."""
    s=''
    for t,b,i in _segments(body):
        core=t.strip()
        if not core: s+=t; continue
        lead=t[:len(t)-len(t.lstrip())]; trail=t[len(t.rstrip()):]
        d=('**' if b else '')+('*' if i else '')
        s+=lead+d+_mdesc(core)+d[::-1]+trail
    return s.strip()
def html_inline(body):
    """raw-HTML variant for use INSIDE <td>: <strong>/<em>, html-escaped (Markdown is not parsed in HTML blocks)"""
    s=''
    for t,b,i in _segments(body):
        e=html.escape(t,quote=False)
        if b: e=f'<strong>{e}</strong>' if e.strip() else e
        if i: e=f'<em>{e}</em>' if e.strip() else e
        s+=e
    return s.strip()

def load_svg(pdf):
    s=sh("pdftocairo","-svg",pdf,"-")
    body=s.split("</defs>")[-1]
    segs=[];rects=[];imgs=[]
    for p in re.findall(r'<path[^>]*?/>',body):
        d=re.search(r' d="([^"]*)"',p).group(1)
        nums=[float(v) for v in re.findall(r'-?[\d.]+',d)]
        mt=re.search(r'transform="matrix\(1, 0, 0, -1, 0, ([\d.]+)\)"',p)
        flip=(lambda y:float(mt.group(1))-y) if mt else (lambda y:y)
        stroke='stroke="rgb' in p; fill='fill="rgb' in p
        pts=list(zip(nums[0::2],[flip(v) for v in nums[1::2]]))
        if stroke and not fill and len(pts)==2 and (abs(pts[0][0]-pts[1][0])<0.01 or abs(pts[0][1]-pts[1][1])<0.01):
            (xa,ya),(xb,yb)=pts; segs.append(('h' if abs(ya-yb)<0.01 else 'v',min(xa,xb),min(ya,yb),max(xa,xb),max(ya,yb)))
        elif fill:
            xs=[a for a,b in pts];ys=[b for a,b in pts]
            col=re.search(r'fill="rgb\(([^)]*)\)"',p).group(1)
            rects.append(dict(x0=min(xs),y0=min(ys),x1=max(xs),y1=max(ys),col=col,stroked=stroke,npts=len(pts),poly=len(pts)>5 or len(pts)==3))
    for m in re.finditer(r'<image[^>]*?width="(\d+)" height="(\d+)"[^>]*?(?:transform="([^"]*)")?',body):
        imgs.append(m.group(0)[:80])
    return segs,rects,imgs

# ---------------- ruled tables (mechanism A) ----------------
def snap(vals,tol=1.5):
    out=[];
    for v in sorted(vals):
        if out and v-out[-1][-1]<=tol: out[-1].append(v)
        else: out.append([v])
    return [sum(g)/len(g) for g in out]
def covered(segs,kind,pos,a,b,tol=1.5):
    """is the edge on line `pos` between a..b covered (>=90%) by segments?"""
    iv=sorted((s[1],s[3]) if kind=='h' else (s[2],s[4]) for s in segs if s[0]==kind and abs((s[2] if kind=='h' else s[1])-pos)<=tol)
    cov=0;cur=a
    for lo,hi in iv:
        lo=max(lo,cur);hi=min(hi,b)
        if hi>lo: cov+=hi-lo;cur=hi
    return cov>=0.9*(b-a)
def ruled_tables(segs,rects):
    # whiteout fills (painter's algorithm): drop rule parts hidden by a later opaque white rect
    white=[r for r in rects if r['col'].startswith('100%, 100%, 100%')]
    def occluded(s):
        for w in white:
            if s[0]=='v' and w['x0']-1<=s[1]<=w['x1']+1 and w['y0']<=s[2]+1 and w['y1']>=s[4]-1: return True
        return False
    # split v-rules at white rects instead of dropping (clip)
    clipped=[]
    for s in segs:
        parts=[(s[2],s[4])] if s[0]=='v' else None
        if s[0]=='v':
            for w in white:
                if w['x0']<=s[1]-0.4 and s[1]+0.4<=w['x1']:  # fill must cover the full stroke width (0.8pt assumed)
                    parts=[q for (a,b) in parts for q in ([(a,min(b,w['y0']))] if a<w['y0'] else [])+([(max(a,w['y1']),b)] if b>w['y1'] else []) if not(a>=w['y0'] and b<=w['y1'])]+[]
                    if not parts: break
            for a,b in parts: clipped.append(('v',s[1],a,s[3],b))
        else: clipped.append(s)
    segs=clipped
    if len([s for s in segs if s[0]=='h'])<2 or len([s for s in segs if s[0]=='v'])<2: return []
    xs=snap([s[1] for s in segs if s[0]=='v']);ys=snap([s[2] for s in segs if s[0]=='h'])
    R,C=len(ys)-1,len(xs)-1
    if R<1 or C<1: return []
    parent=list(range(R*C))
    def find(a):
        while parent[a]!=a: parent[a]=parent[parent[a]];a=parent[a]
        return a
    def union(a,b): parent[find(a)]=find(b)
    for i in range(R):
        for j in range(C):
            if j+1<C and not covered(segs,'v',xs[j+1],ys[i],ys[i+1]): union(i*C+j,i*C+j+1)
            if i+1<R and not covered(segs,'h',ys[i+1],xs[j],xs[j+1]): union(i*C+j,(i+1)*C+j)
    cells={}
    for i in range(R):
        for j in range(C): cells.setdefault(find(i*C+j),[]).append((i,j))
    out=[]
    for root,m in cells.items():
        r0=min(i for i,j in m);r1=max(i for i,j in m);c0=min(j for i,j in m);c1=max(j for i,j in m)
        out.append(dict(r=r0,c=c0,rs=r1-r0+1,cs=c1-c0+1,x0=xs[c0],x1=xs[c1+1],y0=ys[r0],y1=ys[r1+1],
                        rectangular=len(m)==(r1-r0+1)*(c1-c0+1)))
    return dict(xs=xs,ys=ys,R=R,C=C,cells=out,bbox=(xs[0],ys[0],xs[-1],ys[-1]))

def cell_text(runs,cell):
    ins=[r for r in runs if cell['x0']-1<=(r['x0']+r['x1'])/2<=cell['x1']+1 and cell['y0']-1<=(r['y0']+r['y1'])/2<=cell['y1']+1]
    ins.sort(key=lambda r:(round(r['y0']/4),r['x0']))
    return ' '.join(r['md'] for r in ins), ins
def cell_html(ins): return ' '.join(r['html'] for r in ins)
def render_table(t,runs,mode):
    flags=[]
    grid=[[None]*t['C'] for _ in range(t['R'])]
    info=[]
    for c in t['cells']:
        txt,ins=cell_text(runs,c); info.append((c,txt,ins))
        for i in range(c['r'],c['r']+c['rs']):
            for j in range(c['c'],c['c']+c['cs']): grid[i][j]=(c,txt)
    # repair: split spanned cell into per-cluster cells (cluster extends to column before next cluster)
    repaired=[]
    for c,txt,ins in info:
        if REPAIR and c['r']>0 and c['cs']>1 and len(ins)>1 and max(b['x0']-a['x1'] for a,b in zip(ins,ins[1:]))>12:
            xs=t['xs']; colof=lambda r:max(j for j in range(c['c'],c['c']+c['cs']) if xs[j]<=r['x0']+1)
            groups=collections.OrderedDict()
            for r in ins: groups.setdefault(colof(r),[]).append(r)
            ks=list(groups); 
            for n,k in enumerate(ks):
                end=(ks[n+1] if n+1<len(ks) else c['c']+c['cs'])
                repaired.append((dict(c,c=k,cs=end-k,x0=xs[k],x1=xs[end]),' '.join(r['md'] for r in groups[k]),groups[k]))
            flags.append(f"REPAIRED r{c['r']}c{c['c']}: split by text clusters")
        else: repaired.append((c,txt,ins))
    info=repaired
    spans=any(c['rs']>1 or c['cs']>1 for c in t['cells'])
    nonrect=any(not c['rectangular'] for c in t['cells'])
    # deficiency probe: a spanned cell holding >1 horizontally separated text clusters => missing interior rule
    for c,txt,ins in info:
        if c['cs']>1 and len(ins)>1 and not any(True for _ in [0] if c['r']==0):
            gaps=[b['x0']-a['x1'] for a,b in zip(ins,ins[1:])]
            if gaps and max(gaps)>12: flags.append(f"cell r{c['r']}c{c['c']} spans {c['cs']} cols but holds {len(ins)} separate runs -> rules deficient, verify visually")
    if mode=='html' or (mode=='auto' and spans):
        h=["<table>"]
        for i in range(t['R']):
            row=[]
            for c,txt,ins in sorted([x for x in info if x[0]['r']==i],key=lambda x:x[0]['c']):
                a=(f' rowspan="{c["rs"]}"' if c['rs']>1 else '')+(f' colspan="{c["cs"]}"' if c['cs']>1 else '')
                tag='th' if i==0 else 'td'; row.append(f"<{tag}{a}>{cell_html(ins) if True else ''}</{tag}>")
            h.append("  <tr>"+"".join(row)+"</tr>")
        h.append("</table>"); return "\n".join(h),flags,spans
    # GFM has no spans: repeat each (repaired) cell's text in every slot it
    # covers, so every column keeps its header labels and every value stays
    # associated with each column it applies to (table-acceptance.md).
    flat=[['']*t['C'] for _ in range(t['R'])]
    for c,txt,ins in info:
        for i in range(c['r'],c['r']+c['rs']):
            for j in range(c['c'],c['c']+c['cs']): flat[i][j]=txt
    lines=[]
    for i in range(t['R']):
        lines.append("| "+" | ".join(flat[i])+" |")
        if i==0: lines.append("|"+"|".join(["---"]*t['C'])+"|")
    return "\n".join(lines),flags,spans

# ---------------- borderless tables (mechanism B) ----------------
def group_lines(runs):
    L=[]
    for r in sorted(runs,key=lambda r:((r['y0']+r['y1'])/2,r['x0'])):
        cy=(r['y0']+r['y1'])/2
        if L and abs(L[-1]['cy']-cy)<=max(2,0.35*(r['y1']-r['y0'])): L[-1]['runs'].append(r); L[-1]['cy']=(L[-1]['cy']+cy)/2
        else: L.append(dict(cy=cy,runs=[r]))
    for l in L: l['runs'].sort(key=lambda r:r['x0'])
    return L
def borderless_table(lines,body):
    """find gutters via vertical projection of word intervals over a block of consecutive lines.
    column = maximal x-interval set not crossed by gutters; require >=3 rows & >=2 occupied columns in >=60% rows."""
    ncells=lambda l:len(l['runs'])
    isbody=lambda r:r['size']>=0.85*body
    cand=[l for l in lines if sum(1 for r in l['runs'] if isbody(r))>=2]
    # need >=3 consecutive candidate lines (allowing 1 continuation line between)
    idx=[i for i,l in enumerate(lines) if l in cand]
    best=0;run=1
    for a,b in zip(idx,idx[1:]):
        run=run+1 if b-a<=2 else 1; best=max(best,run)
    if len(cand)<3 or best<3: return None
    iv=sorted((r['x0'],r['x1']) for l in cand for r in l['runs'])
    cols=[];cur=list(iv[0])
    for a,b in iv[1:]:
        if a-cur[1]>=8: cols.append(tuple(cur));cur=[a,b]
        else: cur[1]=max(cur[1],b)
    cols.append(tuple(cur))
    if len(cols)<2: return None
    return cols
def borderless_render(lines,cols,runs_in):
    rows=[]
    for l in lines:
        cells=['']*len(cols)
        for r in l['runs']:
            k=min(range(len(cols)),key=lambda k:0 if cols[k][0]-1<=r['x0']<=cols[k][1]+1 or cols[k][0]-1<=r['x1']<=cols[k][1]+1 else min(abs(r['x0']-cols[k][1]),abs(r['x1']-cols[k][0])))
            cells[k]=(cells[k]+' '+r['md']).strip()
        rows.append((l,cells))
    # wrapped-cell continuation: a line occupying only col0 whose gap to the previous line < 1.4*row pitch and previous row has other columns filled
    out=[]
    pitch=sorted(b[0]['cy']-a[0]['cy'] for a,b in zip(rows,rows[1:]))[len(rows)//2] if len(rows)>2 else 0
    for l,cells in rows:
        if out and sum(1 for c in cells if c)==1 and cells[0] and out[-1][1][0]!='' and any(out[-1][1][1:]) and (l['cy']-out[-1][0]['cy'])<=1.2*pitch and cells[0][0].islower():
            out[-1][1][0]+=' '+cells[0]; out[-1][0]=l
        else: out.append([l,cells])
    md=["| "+" | ".join(c) +" |" for l,c in out]
    md.insert(1,"|"+"|".join(["---"]*len(cols))+"|")
    return "\n".join(md)

# ---------------- headings + inline + math (mechanism C/D) ----------------
def sub_sup(prev,r,body):
    """script direction of run r relative to preceding base run prev (screen coords, y down)."""
    if r['size']>0.85*prev['size']: return None
    cy_r=(r['y0']+r['y1'])/2; cy_p=(prev['y0']+prev['y1'])/2
    return '^' if cy_r<cy_p-0.5 else '_'
def math_block(runs,segs,rules_used=None):
    """Greedy 2-D -> LaTeX: big Symbol op with limits, scripts, stacked fraction over a rule, trailing eq number.
    Returns (latex, tag, confidence)."""
    ops={'\u2211':'\\sum','\u220f':'\\prod','\u222b':'\\int'}
    runs=sorted(runs,key=lambda r:r['x0'])
    tag=''
    if runs and runs[-1]['x0']-runs[-2]['x1']>100 and re.fullmatch(r'\(\d+\)',runs[-1]['text'].strip()): tag=runs[-1]['text'].strip();runs=runs[:-1]
    out=[];used=set();conf=0.9
    for i,r in enumerate(runs):
        if i in used: continue
        if r['text'].strip() in ops:
            ocy=(r['y0']+r['y1'])/2; hh=r['y1']-r['y0']
            lim_up=[q for q in runs if q is not r and (q['y0']+q['y1'])/2<ocy-0.2*hh and q['x0']<r['x1'] and q['x1']>r['x0']]
            lim_dn=[q for q in runs if q is not r and (q['y0']+q['y1'])/2>ocy+0.2*hh and q['x0']<r['x1'] and q['x1']>r['x0']]
            tex=ops[r['text'].strip()]
            if lim_dn: tex+='_{'+lim_dn[0]['text'].strip()+'}'; used.add(runs.index(lim_dn[0]))
            if lim_up: tex+='^{'+lim_up[0]['text'].strip()+'}'; used.add(runs.index(lim_up[0]))
            out.append(tex); continue
        # fraction: another run stacked in same x-range with an h-rule between
        mate=[(j,q) for j,q in enumerate(runs) if j!=i and j not in used and q['y0']>r['y1']-6 and abs(q['x0']-r['x0'])<8 and q is not r]
        hr=[sg for sg in segs if sg[0]=='h' and r['x0']-4<=sg[1]<=r['x1']+4 and r['y1']-2<=sg[2]<=r['y1']+10]
        if mate and hr:
            j,q=mate[0]; used.add(j); out.append('\\frac{'+r['text'].strip()+'}{'+q['text'].strip()+'}'); continue
        # script attached to previous base
        if out and i>0 and (sub_sup(runs[i-1],r,0)) and -14<=r['x0']-runs[i-1]['x1']<=4:
            out[-1]+= sub_sup(runs[i-1],r,0)+'{'+r['text'].strip()+'}'; continue
        out.append(r['text'].strip())
    return ' '.join(out),tag,conf
def inline_scripts(line,body):
    """Within a prose line: attach script runs to the preceding base run. Returns (text, had_math)."""
    out=[];math=False
    runs=line['runs']
    for k,r in enumerate(runs):
        if k and r['size']<0.85*body and -14<=r['x0']-runs[k-1]['x1']<=4:
            d=sub_sup(runs[k-1] if runs[k-1]['size']>=0.85*body else runs[k-1],r,body)
            base=runs[k-1]['text'].rstrip()
            if d and re.search(r'[A-Za-z0-9)]$',base) and r['text'].strip().isalnum():
                out.append(('S',d,r['text'].strip())); math=math or ('Times' in r['fam'] or 'Symbol' in r['fam']); continue
        out.append(('T',(' ' if k and ((r['x0']-runs[k-1]['x1']>1 and not runs[k-1]['text'].endswith(' ')) or r['text'].startswith(' ')) and r['size']>=0.85*body else '')+r['md']))
    return out,math
def render_inline(items,is_math):
    s=''
    for it in items:
        if it[0]=='T': s+=it[1]
        else:
            s+= (it[1]+'{'+it[2]+'}') if is_math else ('<sup>' if it[1]=='^' else '<sub>')+it[2]+('</sup>' if it[1]=='^' else '</sub>')
    return s

REPAIR='--no-repair' not in sys.argv
def main():
    pdf=sys.argv[1]; mode=sys.argv[sys.argv.index('--tables')+1] if '--tables' in sys.argv else 'auto'
    runs=load_runs(pdf); segs,rects,imgs=load_svg(pdf)
    body_size=collections.Counter(round(r['size']) for r in runs).most_common(1)[0][0]
    out=[];used=set()
    t=ruled_tables(segs,rects)
    flags_all=[]
    if t:
        md,flags,spans=render_table(t,runs,mode); flags_all+=flags
        b=t['bbox']
        for i,r in enumerate(runs):
            if b[0]-2<=(r['x0']+r['x1'])/2<=b[2]+2 and b[1]-2<=(r['y0']+r['y1'])/2<=b[3]+2: used.add(i)
        tbl=(b[1],md)
    else: tbl=None
    rest=[r for i,r in enumerate(runs) if i not in used]
    lines=group_lines(rest)
    sizes=sorted({round(r['size']) for r in rest if round(r['size'])>body_size and r['bold'] and len(r['text'])<80},reverse=True)
    cols=borderless_table(lines,body_size)
    emitted=[]
    if tbl: emitted.append((tbl[0],tbl[1]))
    if cols:
        tl=[l for l in lines if len(l['runs'])>=2 or (len(l['runs'])==1 and l['runs'][0]['x0']<cols[0][1]+1)]
        tl=[l for l in lines if l['cy']>=min(l2['cy'] for l2 in lines if len(l2['runs'])>=2)-1]
        emitted.append((tl[0]['cy'],borderless_render(tl,cols,rest)))
        tlset={id(l) for l in tl}; lines=[l for l in lines if id(l) not in tlset]
    # display math: clusters around Symbol-font big operators
    mathy=[r for r in rest if r['fam']=='Symbol' and r['text'].strip() in ('\u2211','\u220f','\u222b')]
    consumed=set()
    for op in mathy:
        cl=[q for q in rest if q['y0']>=op['y0']-12 and q['y1']<=op['y1']+12 and q['x0']>=op['x0']-30 and q['x0']<=op['x1']+300]
        tex,tag,conf=math_block(cl,segs)
        emitted.append(((op['y0']+op['y1'])/2,'$$\n'+tex+(' \\tag{'+tag.strip('()')+'}' if tag else '')+'\n$$\n\n<!-- math: geometric heuristic, uncalibrated, evidence = one synthetic fixture; fallback = crop + alt text -->'))
        consumed|={id(q) for q in cl}
    lines=[dict(cy=l['cy'],runs=[r for r in l['runs'] if id(r) not in consumed]) for l in lines]
    lines=[l for l in lines if l['runs']]
    # scripts bound to display math belong to cluster; remaining small lines (sub/sup baselines) merge into neighbouring line
    merged=[]
    small=lambda l:all(r['size']<0.85*body_size for r in l['runs'])
    for k,l in enumerate(lines):
        if small(l):
            # attach to nearest non-small neighbour line that has a base run within 4pt in x
            cands=[m for m in (lines[k-1] if k else None, lines[k+1] if k+1<len(lines) else None) if m and not small(m) and abs(m['cy']-l['cy'])<=body_size*0.9
                   and any(-14<=l['runs'][0]['x0']-b['x1']<=4 for b in m['runs'])]
            if cands:
                t=cands[0]; t['runs']+=l['runs']; t['runs'].sort(key=lambda r:r['x0'])
                if t in merged: continue
                continue
        merged.append(l)
    lines=merged
    for l in lines:
        r=l['runs'][0]; items,ism=inline_scripts(l,body_size); txt=render_inline(items,ism) if any(i[0]=='S' for i in items) else ' '.join(x['md'] for x in l['runs'])
        txt=re.sub(r' {2,}',' ',txt)
        if ism:
            txt=re.sub(r'([\^_]\{[^}]*\})(?=\w)',r'\1 ',txt); txt=re.sub(r'(\S*[\^_]\{[^}]*\}\S*)',lambda m:'$'+m.group(1)+'$',txt)
        if False: txt=re.sub(r'(\S[^$]*?[A-Za-z0-9]\^\{[^}]*\}.*)',lambda m:m.group(1),txt)
        sz=round(max(x['size'] for x in l['runs']))
        if sz>body_size and all(x['bold'] for x in l['runs']):
            lvl=min(6,sizes.index(sz)+1); emitted.append((l['cy'],'#'*lvl+' '+re.sub(r'\*\*','',txt)))
        else: emitted.append((l['cy'],txt))
    emitted.sort(key=lambda e:e[0])
    print("\n\n".join(e[1] for e in emitted))
    for f in flags_all: print("<!-- fidelity-flag: "+f+" -->")
    if imgs: print(f"<!-- {len(imgs)} embedded image(s) detected -->")
if __name__=="__main__": main()
