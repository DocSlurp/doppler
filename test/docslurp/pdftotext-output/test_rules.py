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



# --- hostile and edge cases: clipping, page bounds, huge coordinates, amplification, resolution ---
HUGE = b"1" + b"0" * 120
EDGE = t.simple_page_pdf(
    b"q 0 0 10 10 re W n 0 G 1 w 90 696 m 300 696 l S Q "  # fully clipped stroke: not a rule
    b"0 G 1 w -600 500 m -500 500 l S "  # off-page stroke: not a rule
    b"0 G 1 w 10 400 m " + HUGE + b" 400 l S "  # huge coordinate: clipped to the page
    b"0 g 100 300 200 1 re f"  # 1 pt filled rule
)


def nested_forms_pdf(pages=30, depth=5, fanout=10):
    # The leaf form strokes a horizontal and a vertical line; every level paints the next one
    # fanout times; all pages share one content stream scaled by a huge cm.
    objs = ["<< /Type /Catalog /Pages 2 0 R >>", None]
    content_num = 3
    objs.append(t.stream("", b"1" + b"0" * 120 + b" 0 0 1" + b"0" * 120 + b" 0 0 cm 1 w /X Do"))
    form_nums = []
    first_form = 4
    for level in range(depth):
        num = first_form + level
        form_nums.append(num)
    for level, num in enumerate(form_nums):
        if level == depth - 1:
            body = b"0 0 m 1 0 l S 0 0 m 0 1 l S"
            res = ""
        else:
            body = b" ".join([b"q /X Do Q"] * fanout)
            res = " /Resources << /XObject << /X %d 0 R >> >>" % form_nums[level + 1]
        objs.append(t.stream("/Type /XObject /Subtype /Form /BBox [0 0 1 1]" + res, body))
    page_first = first_form + depth
    kids = []
    for i in range(pages):
        kids.append("%d 0 R" % (page_first + i))
        objs.append("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /X %d 0 R >> >> /Contents %d 0 R >>" % (form_nums[0], content_num))
    objs[1] = "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join(kids), pages)
    return t.build_pdf(objs)


with t.TempDir() as d:
    path = d.write("edge.pdf", EDGE)
    out = utils.run("pdftotext", "-bbox-layout", "-rules", path, "-")
    rules, root = tags(out, "rule")
    page = [e for e in root.iter() if e.tag.endswith("}page")][0]
    width = float(page.attrib["width"])
    t.check(not any(box(r)[0] < -1 for r in rules), "off-page stroke is not reported", repr([r.attrib for r in rules]))
    t.check(not any(abs(box(r)[1] - 96) < 2 for r in rules), "fully clipped stroke is not reported", repr([r.attrib for r in rules]))
    t.check(all(len(v) < 16 for r in rules for v in r.attrib.values()), "rule coordinates are printed with bounded digits", repr([r.attrib for r in rules]))
    t.check(any(abs(box(r)[1] - 392) < 2 and box(r)[2] <= width + 0.01 for r in rules), "a huge-coordinate stroke is clipped to the page", repr([r.attrib for r in rules]))

    out = utils.run("pdftotext", "-r", "144", "-bbox-layout", "-rules", path, "-")
    rules, _ = tags(out, "rule")
    boxes, _ = tags(out, "box")
    t.check(any(abs(box(r)[0] - 200) < 2 for r in rules) and not boxes, "a 1 pt filled rule stays a rule at -r 144", repr(([r.attrib for r in rules], [b.attrib for b in boxes])))

    bomb = d.write("bomb.pdf", nested_forms_pdf())
    data = utils.run("pdftotext", "-bbox-layout", "-rules", bomb, "-", binary=True, timeout=300)
    t.check(len(data) < 40_000_000, "nested-form rule amplification stays bounded (%d bytes from %d)" % (len(data), len(nested_forms_pdf())), "")
    t.check(data.count(b"<rule") <= 200000, "a document keeps at most 200000 rules", str(data.count(b"<rule")))

t.finish()
