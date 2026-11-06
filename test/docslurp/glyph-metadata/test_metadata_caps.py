"""Synthetic multi-page font churn and oversized metadata fail closed."""
import os
import sys
import xml.etree.ElementTree as ET
sys.path.insert(0, os.environ['DOCSLURP_TESTLIB_DIR'])
import docslurp_testlib as t

utils = t.Utils.from_args()

def document(count, cmap):
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'']
    kids = []
    for _ in range(count):
        page = len(objects) + 1
        kids.append(f'{page} 0 R')
        objects.extend([
            f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 {page+1} 0 R >> >> /Contents {page+2} 0 R >>',
            f'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode {page+3} 0 R >>',
            t.stream('', b'BT /F1 12 Tf 20 100 Td (x) Tj ET'),
            t.stream('', cmap, compress=True),
        ])
    objects[1] = f'<< /Type /Pages /Kids [{" ".join(kids)}] /Count {count} >>'
    return t.build_pdf(objects)

cmap = b'1 beginbfchar <78> <0078> endbfchar'
with t.TempDir() as d:
    normal = d.write('fonts.pdf', document(70,cmap))
    out = utils.run('pdftotext','-bbox-layout','-wordmeta','-glyphs',normal,'-')
    root = ET.fromstring(out)
    pages = [e for e in root.iter() if e.tag.split('}')[-1]=='page']
    fonts = [e for e in root.iter() if e.tag.split('}')[-1]=='font']
    t.check(len(pages)==70, 'all page slots retained')
    t.check(0 < len(fonts) <= 64, 'document font cap enforced before further metadata collection', str(len(fonts)))
    t.check(all(p.get('glyphsTruncated')=='1' for p in pages[64:]), 'pages after metadata exhaustion explicitly truncated')
    large = d.write('large.pdf', document(2,b' '*(256*1024)+cmap))
    out = utils.run('pdftotext','-bbox-layout','-wordmeta','-glyphs',large,'-')
    root=ET.fromstring(out)
    pages=[e for e in root.iter() if e.tag.split('}')[-1]=='page']
    t.check(all(p.get('glyphsTruncated')=='1' for p in pages), 'oversized decoded CMap stops collection and marks subsequent pages')
    t.check(not any(e.tag.split('}')[-1]=='font' for e in root.iter()), 'oversized CMap not retained')
    long_text=d.write('long-mapping.pdf',document(1,b'1 beginbfchar <78> <'+b'0041'*33+b'> endbfchar'))
    out=utils.run('pdftotext','-bbox-layout','-wordmeta','-glyphs',long_text,'-')
    root=ET.fromstring(out)
    t.check(any(e.get('glyphsTruncated')=='1' for e in root.iter() if e.tag.split('}')[-1]=='page'), 'per-glyph Unicode cap marks incomplete evidence before page output')
t.finish()
