#!/usr/bin/env python3
"""Hand-written synthetic PDFs (no deps). Usage: mkfixtures.py OUTDIR
Fixtures: ruled_merged.pdf (grid w/ rowspan+colspan), borderless.pdf (whitespace-aligned table),
heading_math.pdf (3 heading sizes, bold/italic runs, sup/sub, fraction, sqrt, sum), shapes.pdf (vector figure + filled bar chart + image)."""
import sys, zlib, os
out = sys.argv[1]; os.makedirs(out, exist_ok=True)

def pdf(content, fonts, extra_res="", xobjs=b"", imgs=None, path="x.pdf"):
    objs = []
    objs.append(b"<< /Type /Catalog /Pages 2 0 R >>")
    objs.append(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
    fres = " ".join(f"/{k} {5+i} 0 R" for i,k in enumerate(fonts))
    nf = len(fonts)
    xr = ""
    if imgs:
        xr = "/XObject << " + " ".join(f"/Im{i} {5+nf+i} 0 R" for i in range(len(imgs))) + " >>"
    objs.append(f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << {fres} >> {xr} >> >>".encode())
    c = content.encode()
    objs.append(b"<< /Length %d >>\nstream\n" % len(c) + c + b"\nendstream")
    for k,v in fonts.items():
        objs.append(f"<< /Type /Font /Subtype /Type1 /BaseFont /{v} >>".encode())
    for (w,h,data) in (imgs or []):
        objs.append(b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length %d >>\nstream\n" % (w,h,len(data)) + data + b"\nendstream")
    b = bytearray(b"%PDF-1.7\n"); offs=[]
    for i,o in enumerate(objs,1):
        offs.append(len(b)); b += f"{i} 0 obj\n".encode()+o+b"\nendobj\n"
    x = len(b); b += f"xref\n0 {len(objs)+1}\n0000000000 65535 f \n".encode()
    for o in offs: b += f"{o:010d} 00000 n \n".encode()
    b += f"trailer\n<< /Size {len(objs)+1} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n".encode()
    open(os.path.join(out,path),"wb").write(b)

def T(x,y,s,f="F1",sz=10): return f"BT /{f} {sz} Tf {x} {y} Td ({s}) Tj ET\n"
def L(x0,y0,x1,y1,w=0.8): return f"{w} w {x0} {y0} m {x1} {y1} l S\n"
F = {"F1":"Helvetica","F2":"Helvetica-Bold","F3":"Helvetica-Oblique","F4":"Times-Roman","F5":"Symbol","F6":"Courier"}

# 1. ruled table with merged cells. columns x: 72,172,272,372,472 ; rows y: 700,670,640,610,580,550
cx=[72,172,272,372,472]; ry=[700,670,640,610,580,550]
c="0 0 0 RG\n"
c+=T(72,740,"Table 1. Quarterly revenue","F2",14)
# horizontal rules (skip interior segments for merged regions)
for i,y in enumerate(ry):
    if i==2:   # row line 2: skip col0-col1 span? (rowspan of cell A in rows1-2 -> skip x 72..172)
        c+=L(172,y,472,y)
    elif i==4: # colspan footer: rows full
        c+=L(72,y,472,y)
    else: c+=L(72,y,472,y)
for j,x in enumerate(cx):
    if j==2:   # header colspan across col1-2 in row 0 -> vertical x=272 only below row0
        c+=L(x,ry[1],x,ry[-1])
    elif j==3: c+=L(x,ry[0],x,ry[3])  # footer row 4 colspan 3 cols: no vline below ry[3]... 
    else: c+=L(x,ry[0],x,ry[-1])
c+=T(78,682,"Region","F2")+T(178,682,"2025 revenue (USD m)","F2")+T(378,682,"Growth","F2")
c+=T(78,652,"EMEA")+T(178,652,"Q1")+T(278,652,"1,204.5")+T(378,652,"+4.1%")
# A rowspan rows1-2 (suppressed rule at ry[2] col0) -> text only once
c+=T(178,622,"Q2")+T(278,622,"1,311.0")+T(378,622,"+8.9%")
c+=T(78,592,"APAC")+T(178,592,"Q1")+T(278,592,"980.2")+T(378,592,"-1.2%")
c+=T(78,562,"Total (colspan 3)","F2")+T(378,562,"+3.9%","F2")
# fix: col0 rule between row1 and row2 skipped; footer row spans col0..col2 (skip x=172,272 verticals in last row) - approximated by white-out
c+="1 1 1 rg 173 551 98 28 re f 273 551 98 28 re f 0 g\n"
c+=T(78,562,"Total (colspan 3)","F2")
pdf(c,F,path="ruled_merged.pdf")

# 2. borderless table, zebra shading, right-aligned numerics, wrapped cell
c=T(72,740,"Borderless table (no rules)","F2",14)
c+="0.93 g 72 676 400 18 re f 72 640 400 18 re f 0 g\n"
rows=[("Item","Units","Unit price","Total"),("Widget alpha","12","3.50","42.00"),("Widget beta with a long","7","10.25","71.75"),("description wrapped","","",""),("Gadget","100","0.99","99.00")]
y=680
for i,r in enumerate(rows):
    f="F2" if i==0 else "F1"
    c+=T(76,y,r[0],f)
    for x,s in zip((260,330,430),r[1:]): 
        # right-align at x+40 using ~5pt/char
        c+=T(x+40-5*len(s),y,s,f)
    y-=18
pdf(c,F,path="borderless.pdf")

# 3. headings, emphasis, math
c=T(72,740,"1 Introduction","F2",20)+T(72,700,"1.1 Background of the study","F2",14)
c+=T(72,675,"Body text 10pt regular followed by ","F1",10)+T(238,675,"bold","F2",10)+T(262,675," and ","F1",10)+T(285,675,"italic","F3",10)+T(312,675," runs; footnote marker","F1",10)+T(425,679,"1","F1",6)
c+=T(72,650,"Inline math: E = mc","F4",11)+T(150,655,"2","F4",7)+T(160,650," and x","F4",11)+T(185,646,"i","F4",7)+T(192,650," + y","F4",11)
# display math: sum with limits
c+=T(250,580,"n","F4",8)+T(248,560,"\\345","F5",24)+T(248,548,"i=1","F4",8)+T(280,560,"x","F4",12)+T(288,556,"i","F4",8)+T(300,560,"=","F4",12)+T(316,566,"a","F4",12)+L(314,563,328,563,0.6)+T(316,553,"b","F4",12)
c+=T(72,500,"Equation (1)","F1",9)+T(520,560,"(1)","F4",11)
c+=T(72,470,"2 Methods","F2",20)
pdf(c,F,path="heading_math.pdf")

# 4. shapes: boxed flow diagram w/ arrows, bar chart (filled rects), embedded image
c=T(72,740,"Figure 1. Pipeline diagram","F2",12)
c+="0.8 0.9 1 rg 72 640 90 40 re f 0 G 72 640 90 40 re S 0 g\n"+T(88,655,"Parse")
c+="0.8 0.9 1 rg 232 640 90 40 re f 0 G 232 640 90 40 re S 0 g\n"+T(244,655,"Classify")
c+=L(162,660,232,660,1.5)+"232 660 m 224 664 l 224 656 l f\n"
c+="0.2 0.4 0.8 rg 100 450 30 80 re f 150 450 30 120 re f 200 450 30 40 re f 0 g\n"+L(90,450,260,450)+L(90,450,90,590)
c+=T(100,438,"A","F1",8)+T(150,438,"B","F1",8)+T(200,438,"C","F1",8)
img=bytes(v for y in range(64) for x in range(64) for v in (x*4%256, y*4%256, 128))
c+="q 120 0 0 120 340 430 cm /Im0 Do Q\n"
pdf(c,F,imgs=[(64,64,img)],path="shapes.pdf")
print("ok")
