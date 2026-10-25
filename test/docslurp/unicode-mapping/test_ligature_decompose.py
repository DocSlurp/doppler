"""Latin ligatures U+FB00..U+FB06 coming from ToUnicode are decomposed for text extraction.

Gfx8BitFont already NFKC-decomposes ligature glyph names, but a ToUnicode entry mapping to
U+FB01 (or a CID font) bypassed that and extracted "ﬁ".  TextPage::addChar now splits them
into letters, giving each letter its own character box so glib/Qt offsets and boxes stay aligned.
"""

import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import um_fixtures as f  # noqa: E402
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()
# code -> ToUnicode value: 1..7 are FB00..FB06, 8 is 'x', 9 is a non-ligature presentation form (FB13)
ENTRIES = [(1, "FB00"), (2, "FB01"), (3, "FB02"), (4, "FB03"), (5, "FB04"), (6, "FB05"), (7, "FB06"), (8, "0078"), (9, "FB13")]
TOUNICODE = (
    b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n"
    b"1 begincodespacerange <0000> <ffff> endcodespacerange\n"
    + ("%d beginbfchar\n" % len(ENTRIES)).encode()
    + "".join("<%04X> <%s>\n" % e for e in ENTRIES).encode()
    + b"endbfchar\nendcmap end end"
)
EXPECTED = "ff" "fi" "fl" "ffi" "ffl" "st" "st" "x" "ﬓ"


def make(d):
    content = b"BT /F1 12 Tf 72 700 Td <" + b"".join(b"%04X" % c for c, _ in ENTRIES) + b"> Tj ET"
    return d.write("lig.pdf", f.type0_page(content, "Identity-H", TOUNICODE, ordering="Identity"))


with t.TempDir() as d:
    path = make(d)
    out = utils.run("pdftotext", path, "-").strip()
    t.check(out == EXPECTED, "ligatures decompose, other presentation forms untouched", repr(out))

    bbox = utils.run("pdftotext", "-bbox", path, "-")
    m = re.search(r'<word xMin="([\d.-]+)" yMin="[\d.-]+" xMax="([\d.-]+)" yMax="[\d.-]+">([^<]*)</word>', bbox)
    width = float(m.group(2)) - float(m.group(1)) if m else -1
    t.check(m is not None and abs(width - 9 * 12.0) < 0.5, "word box still spans the nine glyph advances", repr(m and m.groups()))

    # character offsets vs character boxes through poppler-glib, when GObject introspection is usable
    glib_dir = os.path.join(os.path.dirname(utils.bindir.rstrip("/")), "glib")
    top = os.path.dirname(utils.bindir.rstrip("/"))
    script = (
        "import sys, gi\n"
        "gi.require_version('Poppler', '0.18')\n"
        "from gi.repository import Poppler\n"
        "doc = Poppler.Document.new_from_file('file://' + sys.argv[1], None)\n"
        "page = doc.get_page(0)\n"
        "text = page.get_text().strip()\n"
        "ok, rects = page.get_text_layout()\n"
        "print(repr(text), len(rects))\n"
        "xs = [r.x1 for r in rects]\n"
        "print('monotonic' if all(a < b for a, b in zip(xs, xs[1:])) else 'not-monotonic')\n"
    )
    env = dict(os.environ, GI_TYPELIB_PATH=glib_dir, LD_LIBRARY_PATH=top + os.pathsep + glib_dir)
    proc = subprocess.run([sys.executable, "-I", "-c", script, path], capture_output=True, env=env)
    if proc.returncode != 0:
        print("SKIP: poppler-glib introspection unavailable: " + proc.stderr.decode(errors="replace").strip().splitlines()[-1])
    else:
        lines = proc.stdout.decode().splitlines()
        t.check(lines[0] == "%r %d" % (EXPECTED, len(EXPECTED)), "glib: one character box per extracted character", lines[0])
        t.check(lines[1] == "monotonic", "glib: character boxes are in reading order", lines[1])
t.finish()
