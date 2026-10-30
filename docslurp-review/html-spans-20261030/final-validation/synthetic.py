"""Authored synthetic source; no customer input. Run from any directory."""
from pathlib import Path
import json, subprocess, resource, statistics, os
D=Path(__file__).resolve().parent
R=D.parents[2]
PT=R/'build/utils/pdftotext'
BASE=Path(__import__('os').environ['BASELINE_BINARY'])
CAND=R/'docslurp/doppler-md/target/release/doppler-md'
# Persist source topology before PDF generation.
edges=[(20,700,320,700),(20,640,320,640),(20,700,20,640),(320,700,320,640),(220,700,220,640),(120,660,120,640),(20,660,320,660),(220,680,320,680)]
cells=[(30,686,'Merged'),(230,686,'RightOne'),(230,666,'RightTwo'),(30,646,'BottomOne'),(130,646,'BottomTwo'),(230,646,'BottomThree')]
expected='<table><tbody><tr><td rowspan="2" colspan="2">Merged</td><td>RightOne</td></tr><tr><td>RightTwo</td></tr><tr><td>BottomOne</td><td>BottomTwo</td><td>BottomThree</td></tr></tbody></table>'
(D/'source.json').write_text(json.dumps(dict(edges=edges,cells=cells,pages=33,expected=expected),indent=2))
objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids ['+' '.join(f'{4+2*i} 0 R' for i in range(33))+'] /Count 33 >>','<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
for i in range(33):
    content='0.5 w\n'+''.join(f'{a} {b} m {c} {d} l S\n' for a,b,c,d in edges)+''.join(f'BT /F1 10 Tf {x} {y} Td ({t}) Tj ET\n' for x,y,t in cells)+f'BT /F1 10 Tf 20 500 Td (PageMarker{i+1}) Tj ET\n'
    objects += [f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {5+2*i} 0 R >>',f'<< /Length {len(content)} >>\nstream\n{content}endstream']
pdf=b'%PDF-1.4\n'; offsets=[]
for n,obj in enumerate(objects,1):
    offsets.append(len(pdf)); pdf+=f'{n} 0 obj\n{obj}\nendobj\n'.encode()
xref=len(pdf);pdf+=('xref\n0 '+str(len(objects)+1)+'\n0000000000 65535 f \n'+''.join(f'{o:010} 00000 n \n' for o in offsets)+f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n').encode()
(D/'synthetic.pdf').write_bytes(pdf)
arms={'baseline-gfm':(BASE,[]),'baseline-html':(BASE,['--html']),'candidate-html':(CAND,['--html'])}
def convert(binary,flags):
    outputs=[]
    for first,last in [(1,32),(33,33)]:
        raw=subprocess.run([str(PT),'-bbox-layout','-compact-layout','-wordmeta','-rules','-f',str(first),'-l',str(last),'-enc','UTF-8','-eol','unix',str(D/'synthetic.pdf'),'-'],capture_output=True,check=True).stdout
        params=json.dumps({'furniture_min_share':0.4*33/(last-first+1)})
        out=subprocess.run([str(binary),*flags,'--params',params],input=raw,capture_output=True,check=True).stdout
        assert out.count(b'\f')==last-first
        outputs.append(out)
    return b'\f'.join(outputs)
results={a:[] for a in arms}
for repeat in range(5):
    order=list(arms);order=order[repeat%3:]+order[:repeat%3]
    for arm in order:
        before=resource.getrusage(resource.RUSAGE_CHILDREN); out=convert(*arms[arm]); after=resource.getrusage(resource.RUSAGE_CHILDREN)
        results[arm].append(after.ru_utime+after.ru_stime-before.ru_utime-before.ru_stime)
        (D/f'synthetic-{arm}.md').write_bytes(out)
        pages=out.decode().split('\f');assert len(pages)==33
        for i,page in enumerate(pages,1):
            assert f'PageMarker{i}' in page
            for _,_,label in cells: assert page.count(label)==1,(arm,i,label)
        if arm=='candidate-html':assert out.count(b'rowspan="2" colspan="2"')==33
(D/'cpu.json').write_text(json.dumps({'load_average':os.getloadavg(),'runs':results,'median_user_sys_seconds':{a:statistics.median(v) for a,v in results.items()}},indent=2))
subprocess.run(['pdftoppm','-f','33','-l','33','-scale-to','1000','-png','-singlefile',str(D/'synthetic.pdf'),str(D/'page33')],check=True)
print('33-page topology, exact-once text, chunk transition and five interleaved CPU runs passed')
