#!/usr/bin/env python3
"""Hand-written synthetic fixtures (no deps). tagged.pdf: H1 (inline-dict MCID), P (named property-list MCID),
list, 3x3 ruled table with Table/TR/TH/TD tags, link annotation, XObject image, shape, math-ish text. untagged.pdf: same page minus tags."""
import sys, zlib
def build(tagged, named=True):
    objs = []
    def add(s): objs.append(s); return len(objs)
    # reserve ids
    cat=add(None); pages=add(None); page=add(None); content=add(None); f1=add(None); f2=add(None); f3=add(None); img=add(None); link=add(None)
    st=add(None) if tagged else 0; doc=add(None) if tagged else 0; pt=add(None) if tagged else 0
    tb=th1=tr1=tr2=tr3=0; elems=[]
    mc = 0
    ops=[]
    def bdc(tag, inline=True):
        nonlocal mc
        i=mc; mc+=1
        if tagged:
            ops.append(f"/{tag} <</MCID {i}>> BDC" if inline else f"/{tag} /MC{i} BDC")
        return i
    def emc():
        if tagged: ops.append("EMC")
    mc_list=[]  # (tag, mcid, kind)
    def text(x,y,font,size,s,tag=None,inline=True,render=None):
        if tag: mc_list.append((tag,bdc(tag,inline)))
        ops.append(f"BT /{font} {size} Tf {x} {y} Td ({s}) Tj ET")
        if tag: emc()
    text(72,720,"F2",24,"Quarterly Report","H1")
    text(72,690,"F1",11,"Revenue grew 12% year over year; see the","P",inline=not named)
    ops.append("BT /F3 11 Tf 328 690 Td (docs) Tj ET")   # link text (untagged continuation inside no MC: gap probe)
    text(72,672,"F1",11,"E = mc","P",inline=True)
    ops.append("BT /F1 7 Tf 108 677 Td (2) Tj ET")        # superscript
    text(72,650,"F1",11,"- first bullet","LI"); text(72,636,"F1",11,"- second bullet","LI")
    # table 3x3 with rulings at x=72,172,272,372 y=560,540,520,500
    xs=[72,172,272,372]; ys=[580,560,540,520]
    for y in ys: ops.append(f"0.5 w {xs[0]} {y} m {xs[-1]} {y} l S")
    for x in xs: ops.append(f"0.5 w {x} {ys[0]} m {x} {ys[-1]} l S")
    cells=[["Item","Qty","Price"],["Apple","10","1.20"],["Pear","4","2.05"]]
    for r,row in enumerate(cells):
        for c,v in enumerate(row):
            text(xs[c]+4,ys[r]-14,"F2" if r==0 else "F1",10,v,"TH" if r==0 else "TD")
    # shape: filled rect + image
    ops.append("0.9 0.2 0.2 rg 400 560 60 20 re f")
    ops.append("q 80 0 0 60 72 400 cm /Im1 Do q".replace(" q","") + " Q")
    ops.append("BT /F1 9 Tf 72 380 Td (Figure 1: red square) Tj ET")
    cs="\n".join(ops).encode()
    objs[content-1]=("stream",f"<< /Length {len(cs)} >>".encode(),cs)
    objs[cat-1]=(b"<< /Type /Catalog /Pages %d 0 R "%pages + (b"/MarkInfo << /Marked true >> /StructTreeRoot %d 0 R /Lang (en-US) "%st if tagged else b"")+b">>")
    objs[pages-1]=b"<< /Type /Pages /Kids [%d 0 R] /Count 1 >>"%page
    props="".join(f"/MC{i} << /MCID {i} >> " for i in range(mc))
    objs[page-1]=(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Contents %d 0 R /Annots [%d 0 R] "%(pages,content,link)
        +(b"/StructParents 0 " if tagged else b"")+b"/Resources << /Font << /F1 %d 0 R /F2 %d 0 R /F3 %d 0 R >> /XObject << /Im1 %d 0 R >> /Properties << %s >> >> >>"%(f1,f2,f3,img,props.encode()))
    objs[f1-1]=b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>"
    objs[f2-1]=b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>"
    objs[f3-1]=b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Oblique >>"
    raw=bytes([255,0,0]*4)
    objs[img-1]=("stream",b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length %d >>"%len(raw),raw)
    objs[link-1]=b"<< /Type /Annot /Subtype /Link /Rect [328 686 360 700] /Border [0 0 0] /A << /S /URI /URI (https://docslurp.io/) >> >>"
    if tagged:
        # struct tree: Document -> H1, P, L(LI,LI), Table(TR(TH..),TR(TD..),TR(TD..))
        # ids: st, doc, pt, then elements appended
        leaf={}
        def new(): objs.append(None); return len(objs)
        ids=[new() for _ in range(40)]; it=iter(ids)
        def el(typ,kids,pgref=True):
            i=next(it)
            ks=" ".join(k if isinstance(k,str) else str(k) for k in kids)
            objs[i-1]=f"<< /Type /StructElem /S /{typ} /P PARENT /Pg {page} 0 R /K [{ks}] >>".encode()
            return i
        # build bottom-up with manual parent fixups
        recs={}
        def mk(typ,kids):
            i=next(it); recs[i]=(typ,kids); return i
        mcs=iter(range(mc))
        def leafmc(tag):
            return next(mcs)
        h1=mk("H1",[-1]); p1=mk("P",[-2]); p2=mk("P",[-3]); l1=mk("LI",[-4]); l2=mk("LI",[-5])
        lst=mk("L",[l1,l2])
        rows=[]; k=6
        for r in range(3):
            cs_=[]
            for c in range(3):
                cs_.append(mk("TH" if r==0 else "TD",[-k])); k+=1
            rows.append(mk("TR",cs_))
        tbl=mk("Table",rows)
        d=mk("Document",[h1,p1,p2,lst,tbl])
        parent={}
        for i,(t,ks) in recs.items():
            for kk in ks:
                if kk in recs: parent[kk]=i
        parent[d]=st
        for i,(t,ks) in recs.items():
            kstr=" ".join(f"{x} 0 R" if x in recs else str(-x-1) for x in ks)
            objs[i-1]=f"<< /Type /StructElem /S /{t} /P {parent[i]} 0 R /Pg {page} 0 R /K [{kstr}] >>".encode()
        # ParentTree: mcid -> leaf struct elem
        mcid2el={}
        for i,(t,ks) in recs.items():
            for kk in ks:
                if kk not in recs: mcid2el[-kk-1]=i
        nums=" ".join(f"{m} {mcid2el[m]} 0 R" for m in sorted(mcid2el))
        objs[pt-1]=f"<< /Nums [0 [{ ' '.join(f'{mcid2el[m]} 0 R' for m in sorted(mcid2el)) }]] >>".encode()
        objs[st-1]=f"<< /Type /StructTreeRoot /K {d} 0 R /ParentTree {pt} 0 R >>".encode()
        objs[doc-1]=b"<< >>"
    out=bytearray(b"%PDF-1.7\n"); offs=[]
    for i,o in enumerate(objs,1):
        offs.append(len(out))
        out+=f"{i} 0 obj\n".encode()
        if isinstance(o,tuple):
            out+=o[1]+b"\nstream\n"+o[2]+b"\nendstream"
        else: out+=(o if o is not None else b"<< >>")
        out+=b"\nendobj\n"
    x=len(out)
    out+=f"xref\n0 {len(objs)+1}\n0000000000 65535 f \n".encode()
    for o in offs: out+=f"{o:010d} 00000 n \n".encode()
    out+=f"trailer\n<< /Size {len(objs)+1} /Root {cat} 0 R /Info << /Title (Fixture) /Author (Arch) >> >>\nstartxref\n{x}\n%%EOF\n".encode()
    return bytes(out)
if __name__=="__main__":
    d=sys.argv[1]
    open(d+"/tagged.pdf","wb").write(build(True)); open(d+"/untagged.pdf","wb").write(build(False)); open(d+"/tagged_inline_only.pdf","wb").write(build(True,named=False))
