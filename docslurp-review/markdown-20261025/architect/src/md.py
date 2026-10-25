#!/usr/bin/env python3 -I
"""Evidence JSONL -> Markdown projection (experiment). Knows nothing about poppler; consumes only the IR.
usage: md.py evidence.jsonl [--profile gfm|rich] [--assets-dir DIR]
profile gfm : CommonMark+GFM only; spans/shapes degrade to plain text/comments (lossy, portable)
profile rich: adds HTML <table> with colspan/rowspan, <sup>/<span style>, inline <svg> fragments (non-standard, optional)"""
import sys, json, re, collections, html
args=sys.argv[1:]; prof='gfm'
if '--profile' in args: prof=args[args.index('--profile')+1]
rich = prof=='rich'
D=collections.defaultdict(list); meta={}; struct={}
for l in open(args[0]):
    d=json.loads(l); t=d['t']
    if t=='meta': meta[d['k']]=d['v'].rstrip('\x00')
    elif t=='struct': struct[(d['page'],d['mcid'])]=d
    elif t=='page': cur=d['n']; D[cur]=[]; PG=D[cur]; D[('page',cur)]=d
    elif t in('block','line','path','image','link'): D[cur].append(d)
BOLD=re.compile(r'bold|black|heavy|semibold|demi',re.I); ITAL=re.compile(r'italic|oblique',re.I)
def sty(w): return (bool(w['b']) or bool(BOLD.search(w['font'])), bool(w['i']) or bool(ITAL.search(w['font'])), bool(w['m']) or 'courier' in w['font'].lower() or 'mono' in w['font'].lower())
def wrap(s,b,i,m):
    if not s.strip(): return s
    if m: s=f'`{s}`'
    if b and i: return f'***{s}***'
    if b: return f'**{s}**'
    if i: return f'*{s}*'
    return s
def inline(words, body, base_ref=None):
    out=[]; 
    for w in words:
        s=w['s']; b,i,m=sty(w)
        sup = w['fs']<0.8*body and base_ref is not None and w['base']<base_ref-1.5
        if rich and sup: s=f'<sup>{html.escape(s)}</sup>'
        elif sup: s=f'^{s}'
        else: s=wrap(s,b,i,m)
        if w['uri']: s=f'[{s}]({w["uri"]})'
        out.append(s)
    # merge adjacent runs of identical emphasis would be a refinement; keep simple
    return ' '.join(out).replace('** **',' ').replace('* *',' ')
def role_of(w,pg):
    s=struct.get((pg,w['mc'])); return s
def grid_from_rules(paths):
    H=[p for p in paths if p['d']['k']=='stroke' and abs(p['d']['y1']-p['d']['y0'])<1.5 and p['d']['x1']-p['d']['x0']>20]
    V=[p for p in paths if p['d']['k']=='stroke' and abs(p['d']['x1']-p['d']['x0'])<1.5 and p['d']['y1']-p['d']['y0']>8]
    if len(H)<2 or len(V)<2: return None
    ys=sorted({round(p['d']['y0'],0) for p in H}); xs=sorted({round(p['d']['x0'],0) for p in V})
    return xs,ys,{id(p) for p in H+V}
def project(pg, nth):
    items=D[pg]; words_all=[]; lines=[]
    for it in items:
        if it['t']=='line': lines.append(it)
    allw=[w for l in lines for w in l['words']]
    body=collections.Counter()
    for w in allw: body[round(w['fs'],1)]+=len(w['s'])
    bodyfs=body.most_common(1)[0][0] if body else 10
    out=[f'<!-- ds:page={pg} -->'] if True else []
    paths=[it for it in items if it['t']=='path']
    g=grid_from_rules([{'d':p['d']} for p in paths]) if paths else None
    used=set(); table_md=None; tb=None
    if g:
        xs,ys,_=g; rows=len(ys)-1; cols=len(xs)-1
        cell=[[[] for _ in range(cols)] for _ in range(rows)]
        for l in lines:
            for w in l['words']:
                cx=(w['x0']+w['x1'])/2; cy=(w['y0']+w['y1'])/2
                for r in range(rows):
                    if ys[r]<=cy<=ys[r+1]:
                        for c in range(cols):
                            if xs[c]<=cx<=xs[c+1]: cell[r][c].append(w); used.add(id(w))
        tb=(min(ys),max(ys))
        txt=[[inline(c,bodyfs) for c in row] for row in cell]
        if rich:
            h=['<table data-ds-source="ruling-grid" data-ds-confidence="0.9">']
            for r,row in enumerate(txt):
                tag='th' if r==0 else 'td'
                h.append('<tr>'+''.join(f'<{tag}>{html.escape(t)}</{tag}>' for t in row)+'</tr>')
            h.append('</table>'); table_md='\n'.join(h)
        else:
            table_md='| '+' | '.join(txt[0])+' |\n|'+'---|'*cols+'\n'+'\n'.join('| '+' | '.join(r)+' |' for r in txt[1:])
    # shapes (non-rule fills)
    shapes=[p['d'] for p in paths if p['d']['k']=='fill' and not p['d']['curve']]
    imgs=[it['d'] for it in items if it['t']=='image']
    events=[]
    for l in lines:
        ws=[w for w in l['words'] if id(w) not in used]
        if ws: events.append((l['y0'],'line',l,ws))
    if table_md: events.append((tb[0],'table',table_md,None))
    for im in imgs: events.append((im['y0'],'img',im,None))
    for s in shapes: events.append((s['y0'],'shape',s,None))
    events.sort(key=lambda e:e[0])
    para=[]
    def flush():
        if para: out.append(' '.join(para)); para.clear()
    prev_y=None
    for y,k,o,ws in events:
        if k=='table': flush(); out.append(o)
        elif k=='img':
            flush(); n=f'assets/p{pg}-img{o["ref"]}.png'
            out.append(f'![image {o["w"]}x{o["h"]}]({n})' + (f'\n<!-- ds:bbox={o["x0"]:.0f},{o["y0"]:.0f},{o["x1"]:.0f},{o["y1"]:.0f} -->'))
        elif k=='shape':
            flush()
            if rich: out.append(f'<svg viewBox="{o["x0"]:.0f} {o["y0"]:.0f} {o["x1"]-o["x0"]:.0f} {o["y1"]-o["y0"]:.0f}" width="{o["x1"]-o["x0"]:.0f}" height="{o["y1"]-o["y0"]:.0f}"><rect width="100%" height="100%" fill="#{o["color"]}"/></svg>')
            else: out.append(f'<!-- ds:shape rect fill=#{o["color"]} bbox={o["x0"]:.0f},{o["y0"]:.0f},{o["x1"]:.0f},{o["y1"]:.0f} (dropped in gfm profile) -->')
        else:
            fs=max(w['fs'] for w in ws); base=max(w['base'] for w in ws if w['fs']>=0.9*fs)
            main=[w for w in ws if w['fs']>=0.8*bodyfs or fs<bodyfs]
            struct_role=None
            for w in ws:
                s=role_of(w,pg)
                if s: struct_role=s['path'].split('/')[-1]; break
            text=inline(ws,bodyfs,base)
            m=re.match(r'^(\*\*|\*)?(-|•|•|\d+[.)])\s+',text)
            lvl = {'H1':1,'H2':2,'H3':3,'H4':4,'H5':5,'H6':6}.get(struct_role or '',0)
            if not lvl and fs>=1.25*bodyfs: lvl=1 if fs>=1.7*bodyfs else 2
            if lvl: flush(); out.append('#'*lvl+' '+re.sub(r'\*\*','',text))
            elif m or struct_role=='LI':
                flush(); out.append('- '+re.sub(r'^(\*\*|\*)?(-|•|•|\d+[.)])\s+','',text))
            else:
                if prev_y is not None and y-prev_y>1.8*bodyfs: flush()
                para.append(text)
            prev_y=y
    flush(); return '\n\n'.join(o for o in out if o)
fm=['---']+[f'{k.lower()}: "{v}"' for k,v in meta.items() if k in('Title','Author','Subject','Keywords','CreationDate')]+[f'profile: {prof}','---']
pages=[p for p in D if isinstance(p,int)]
print('\n'.join(fm)+'\n'); print('\n\n'.join(project(p,0) for p in sorted(pages)))
