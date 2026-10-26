"""-rules (with -bbox-layout) reports axis-aligned vector rules and filled boxes per page.

Table borders, row separators and cell shading are drawn as stroked lines, stroked rectangles
and filled rectangles. -rules emits them in the word coordinate space: <rule> for line segments
and thin filled rectangles, <box> for other filled rectangles, each with its colour. Curves and
diagonal lines are ignored. Output without -rules is unchanged, and a page with more than
20000 rules is truncated and marked rulesTruncated="1".
"""

import os
import subprocess
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t  # noqa: E402

utils = t.Utils.from_args()

CONTENT = (
    b"BT /F1 10 Tf 100 700 Td (Cell) Tj ET "
    b"0 0 1 RG 1 w 90 696 m 300 696 l S "  # blue horizontal stroke just under the word (rule)
    b"0 G 0.5 w 90 690 m 90 720 l S "  # vertical stroke (rule)
    b"0 G 0.5 w 320 600 100 50 re S "  # stroked rectangle: four edge rules
    b"1 0 0 rg 90 560 200 0.8 re f "  # thin red filled rectangle: rule
    b"0.9 g 90 500 200 40 re f "  # grey filled rectangle: box
    b"0 G 50 400 m 150 450 l S "  # diagonal: ignored
    b"0 G 50 300 m 60 350 70 350 80 300 c S"  # curve: ignored
)
PDF = t.simple_page_pdf(CONTENT)
BIG = t.simple_page_pdf(b"0 G 0.1 w " + b" ".join(b"10 %d m 20 %d l S" % (i % 700 + 10, i % 700 + 10) for i in range(25000)))


def tags(out, name):
    root = ET.fromstring(out.encode("utf-8"))
    return [e for e in root.iter() if e.tag.endswith("}" + name)], root


def box(e):
    return tuple(float(e.attrib[k]) for k in ("xMin", "yMin", "xMax", "yMax"))


with t.TempDir() as d:
    path = d.write("rules.pdf", PDF)
    plain_layout = utils.run("pdftotext", "-bbox-layout", path, "-")
    t.check("<rule" not in plain_layout and "<box" not in plain_layout, "without -rules the output has no rules", plain_layout[-400:])

    out = utils.run("pdftotext", "-bbox-layout", "-rules", path, "-")
    rules, root = tags(out, "rule")
    boxes, _ = tags(out, "box")
    words = [e for e in root.iter() if e.tag.endswith("}word")]
    t.check(len(rules) == 7, "2 strokes + 4 rectangle edges + 1 thin fill = 7 rules", repr([(box(r), r.attrib.get("color")) for r in rules]))
    t.check(len(boxes) == 1 and boxes[0].attrib.get("color") == "#e5e5e5", "one grey filled box", repr([b.attrib for b in boxes]))
    blue = [r for r in rules if r.attrib.get("color") == "#0000ff"]
    t.check(len(blue) == 1, "stroke colour is reported", repr([r.attrib for r in rules]))
    if blue and words:
        _, wy0, _, wy1 = box(words[0])
        x0, y0, x1, y1 = box(blue[0])
        t.check(wy1 < y0 < wy1 + 6 and abs(x0 - 90) < 1 and abs(x1 - 300) < 1, "rule shares the word coordinate space (just below the word)", repr((box(words[0]), box(blue[0]))))
    red = [r for r in rules if r.attrib.get("color") == "#ff0000"]
    t.check(len(red) == 1 and box(red[0])[3] - box(red[0])[1] < 1, "thin filled rectangle becomes one rule", repr([r.attrib for r in red]))

    proc = subprocess.run([os.path.join(utils.bindir, "pdftotext"), "-rules", path, "-"], capture_output=True)
    t.check(proc.returncode == 99, "-rules without -bbox-layout is rejected", repr(proc))

    big = d.write("big.pdf", BIG)
    out = utils.run("pdftotext", "-bbox-layout", "-rules", big, "-")
    rules, root = tags(out, "rule")
    pages = [e for e in root.iter() if e.tag.endswith("}page")]
    t.check(len(rules) == 20000 and pages[0].attrib.get("rulesTruncated") == "1", "rules are capped at 20000 per page and the page is marked", "%d rules, %r" % (len(rules), pages[0].attrib))

t.finish()
