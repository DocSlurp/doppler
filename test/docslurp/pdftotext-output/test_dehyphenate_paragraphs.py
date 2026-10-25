"""End-of-line dehyphenation must join words inside a block only, never glue a
paragraph that ends in a hyphen to the next block of the same flow."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t  # noqa: E402

utils = t.Utils.from_args()
GAP = 26


def pdf(last_end):
    """Three paragraphs; #1 and #2 have a hyphenated word break inside the block, #2 and #3 end in `last_end`."""
    lines = [
        (700, b"The quick brown fox jumps over the lazy dog and the exam-"),
        (686, b"ple continues on the second line of paragraph one."),
        (686 - GAP, b"A second paragraph begins with a fresh sentence and ends in a hyphen-"),
        (672 - GAP, b"ated word on its second line and then ends with a dash" + last_end),
        (672 - 2 * GAP, b"Third paragraph starts here."),
    ]
    content = b"BT /F1 12 Tf " + b" ".join(b"1 0 0 1 72 %d Tm (%s) Tj" % (y, x) for y, x in lines) + b" ET"
    return t.simple_page_pdf(content)


def run(last_end, *args):
    with t.TempDir() as d:
        path = d.write("h.pdf", pdf(last_end))
        return utils.run("pdftotext", *args, path, "-")


for label, end, args in (("hyphen", b"-", ()), ("hyphen -remove-hyphens all", b"-", ("-remove-hyphens", "all")), ("soft hyphen", b"\\255", ("-remove-hyphens", "soft"))):
    out = run(end, *args)
    lines = out.split("\n")
    if "soft" not in label:  # -remove-hyphens soft only joins U+00AD, not '-'
        t.check("the example continues" in out, label + ": hyphen inside a block is still joined (exam-/ple)", out)
        t.check("hyphenated word on its second line" in out, label + ": second hyphen inside a block is still joined (hyphen-/ated)", out)
    t.check(any(l.startswith("Third paragraph") for l in lines), label + ": last line of a block does not swallow the next paragraph", out)
    t.check("dashThird" not in out and "dash­Third" not in out, label + ": no glued 'dashThird'", out)

out = run(b"-", "-remove-hyphens", "none")
t.check("exam-\nple" in out and "dash-\nThird" in out, "-remove-hyphens none keeps every hyphen and line break", out)
t.finish()
