"""Compact geometry keeps words and metadata while bounding coordinate rounding."""
import os
import subprocess
import sys
import xml.etree.ElementTree as ET
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t
utils = t.Utils.from_args()
content = (b'BT /F1 12 Tf 72.123456 700.654321 Td (Same style words & <tags>) Tj '
           b'/F1 9 Tf 1 0 0 rg (red) Tj ET '
           b'BT /F1 12 Tf 3 Tr 72 600 Td (hidden words) Tj ET '
           b'BT /F1 12 Tf 0 Tr 0 1 -1 0 300 300 Tm (Rotated) Tj ET')
coords = {'xMin', 'xMax', 'yMin', 'yMax'}
with t.TempDir() as d:
    path = d.write('geometry.pdf', t.simple_page_pdf(content))
    for meta in ([], ['-wordmeta']):
        args = ['-bbox-layout', *meta, '-rules']
        ordinary = utils.run('pdftotext', *args, path, '-')
        compact = utils.run('pdftotext', *args, '-compact-layout', path, '-')
        a, b = list(ET.fromstring(ordinary).iter()), list(ET.fromstring(compact).iter())
        t.check(len(a) == len(b), 'same XML structure')
        for left, right in zip(a, b):
            t.check(left.tag == right.tag and left.text == right.text, 'same text and element')
            isword = left.tag.endswith('}word')
            for key, value in left.attrib.items():
                if isword and key in coords:
                    t.check(abs(float(value)-float(right.attrib[key])) <= 0.000501, 'bounded word geometry rounding')
                else:
                    t.check(value == right.attrib[key], 'unchanged non-coordinate metadata')
        t.check(len(compact) < len(ordinary), 'compact output saves bytes')
    invalid = subprocess.run([os.path.join(utils.bindir, 'pdftotext'), '-compact-layout', path, '-'], capture_output=True)
    t.check(invalid.returncode != 0 and not invalid.stdout, 'flag without layout refuses')
    bad = d.write('malformed.pdf', b'%PDF-1.7\nnot a document')
    invalid = subprocess.run([os.path.join(utils.bindir, 'pdftotext'), '-bbox-layout', '-compact-layout', bad, '-'], capture_output=True)
    t.check(invalid.returncode != 0, 'malformed input fails')
t.finish()
