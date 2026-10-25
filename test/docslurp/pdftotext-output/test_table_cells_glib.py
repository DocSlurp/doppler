"""glib poppler_page_get_text() must separate the cells of a table row, and the
text layout / attributes must keep one entry per character of that text.

Needs the PyGObject bindings and the glib typelib built next to the utils;
exits 77 (skipped) when either is missing."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t  # noqa: E402

bindir = t.Utils.from_args().bindir
glibdir = os.path.join(os.path.dirname(os.path.abspath(bindir)), "glib")
if not os.path.exists(os.path.join(glibdir, "Poppler-0.18.typelib")):
    print("SKIP: no glib typelib in %s" % glibdir)
    sys.exit(77)
# The typelib names an unversioned-path library: make sure the one from this build tree is loaded
# (not a system libpoppler-glib) by re-executing with LD_LIBRARY_PATH set.
libdirs = [glibdir, os.path.dirname(glibdir)]
if os.environ.get("DOCSLURP_GLIB_REEXEC") != "1":
    env = dict(os.environ, GI_TYPELIB_PATH=glibdir, DOCSLURP_GLIB_REEXEC="1", LD_LIBRARY_PATH=os.pathsep.join(libdirs + [os.environ.get("LD_LIBRARY_PATH", "")]))
    os.execve(sys.executable, [sys.executable, "-I"] + sys.argv, env)
try:
    import gi

    gi.require_version("Poppler", "0.18")
    from gi.repository import Poppler
except (ImportError, ValueError) as e:
    print("SKIP: PyGObject / Poppler typelib unavailable: %s" % e)
    sys.exit(77)

ROWS = [("Item", "Qty", "Price"), ("Apple", "10", "1.20"), ("Banana", "25", "0.50"), ("Red Cherry", "7", "12.75"), ("Date", "3", "4.00")]
COLS = [72, 200, 330]


def page_text(content):
    with t.TempDir() as d:
        path = d.write("t.pdf", t.simple_page_pdf(content))
        doc = Poppler.Document.new_from_file("file://" + path, None)
        page = doc.get_page(0)
        text = page.get_text()
        ok, rects = page.get_text_layout()
        attrs = page.get_text_attributes()
        return text, (rects if ok else []), attrs


ops = []
for i, row in enumerate(ROWS):
    for x, cell in zip(COLS, row):
        ops.append(b"1 0 0 1 %d %d Tm (%s) Tj" % (x, 700 - 30 * i, cell.encode()))
text, rects, attrs = page_text(b"BT /F1 12 Tf " + b" ".join(ops) + b" ET")

expected = "\n".join("\t".join(r) for r in ROWS)
t.check(text == expected, "table cells are separated by tabs, rows by newlines", "got %r\nwant %r" % (text, expected))
t.check("Apple101.20" not in text, "cells are no longer glued ('Apple101.20')", repr(text))
t.check(len(rects) == len(text), "text layout has one rectangle per character (%d rects, %d chars)" % (len(rects), len(text)), "")
t.check(bool(attrs) and attrs[-1].end_index == len(text) - 1, "text attributes cover the whole text", "last end_index %s for %d chars" % (attrs[-1].end_index if attrs else None, len(text)))

# the rectangle of each separator sits between the two cells it separates
ok = True
for i, ch in enumerate(text):
    if ch == "\t" and i < len(rects) and not (rects[i].x2 > rects[i].x1 and rects[i - 1].x2 <= rects[i].x1 + 0.01 and rects[i + 1].x1 >= rects[i].x2 - 0.01):
        ok = False
t.check(ok, "each tab rectangle lies between the neighbouring cells", "")

# ordinary text: words still separated by a single space, unchanged
text, rects, attrs = page_text(b"BT /F1 12 Tf 72 700 Td (Hello brave new world) Tj ET")
t.check(text == "Hello brave new world", "non-table text is unchanged", repr(text))
t.check(len(rects) == len(text), "non-table layout has one rectangle per character", "%d vs %d" % (len(rects), len(text)))
t.finish()
