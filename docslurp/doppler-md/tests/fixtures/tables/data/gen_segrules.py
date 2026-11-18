# Word-style 3-rule table: each rule drawn as one filled rectangle per column (segments abut),
# Courier text, a two-line header. OUT [segmented=1|0]
import sys
import os; sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdfg import PDFG
out = sys.argv[1]; seg = len(sys.argv) < 3 or sys.argv[2] == '1'
cols = [(72, 261.36), (261.12, 328.8), (328.56, 391.68), (391.44, 517.68)]
rows = [('Total', '39695', '100.0', '100.0'), ('Not interviewed', '5701', '14.4', '18.2'),
        ('Interviewed, not examined', '2683', '6.8', '7.5'), ('MEC examined', '30818', '77.6', '73.4'),
        ('Home examined', '493', '1.2', '0.8')]
ops = []
def rule(ytop, h):
    if seg:
        for x0, x1 in cols: ops.append('%.2f %.2f %.2f %.2f re f' % (x0, ytop - h, x1 - x0, h))
    else: ops.append('%.2f %.2f %.2f %.2f re f' % (cols[0][0], ytop - h, cols[-1][1] - cols[0][0], h))
def text(x, ytop, s, right=None):
    if right is not None: x = right - 7.2 * len(s)
    ops.append('BT /F1 12 Tf %.2f %.2f Td (%s) Tj ET' % (x, 792 - ytop - 8.2, s))
ops.append('BT /F2 12 Tf 72 708 Td (Table A.3: Overall interview and examination response rates, 2025) Tj ET')
rule(792 - 113.8 + 1.08, 2.16)   # top rule (y down coords converted)
text(145.4, 116.05, 'Status'); text(0, 116.05, 'Sample', 327.12); text(0, 128.77, 'size', 327.12)
text(0, 116.05, 'Percent', 390.24); text(0, 116.05, 'Weighted', 490.56); text(0, 128.77, 'Percent', 490.56)
rule(792 - 139.8 + 1.08, 2.16)
y = 144.13
for r in rows:
    text(74.4, y, r[0]); text(0, y, r[1], 327.12); text(0, y, r[2], 390.24); text(0, y, r[3], 490.56); y += 15.36
rule(792 - 216.8 + 1.08, 2.16)
ops.append('BT /F1 12 Tf 72 %.2f Td (Source: survey data file, 2025) Tj ET' % (792 - 240))
p = PDFG(); cat = p.reserve(); pages = p.reserve()
f1 = p.add('<</Type/Font/Subtype/Type1/BaseFont/Courier>>'); f2 = p.add('<</Type/Font/Subtype/Type1/BaseFont/Courier-Bold>>')
cs = p.stream('\n'.join(ops))
pg = p.add('<</Type/Page/Parent %d 0 R/MediaBox[0 0 612 792]/Resources<</Font<</F1 %d 0 R/F2 %d 0 R>>>>/Contents %d 0 R>>' % (pages, f1, f2, cs))
p.set(pages, '<</Type/Pages/Kids[%d 0 R]/Count 1>>' % pg)
p.set(cat, '<</Type/Catalog/Pages %d 0 R>>' % pages)
p.write(out, cat)
