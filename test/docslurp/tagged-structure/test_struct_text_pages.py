"""Fix 3: struct text is taken from the right page, with each page interpreted once."""

import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tagged_pdf as tp  # noqa: E402

t = tp.t
utils = t.Utils.from_args()


def mc(text, mcid, tag="P"):
    return b"/%s <</MCID %d>> BDC BT /F1 12 Tf 72 %d Td (%s) Tj ET EMC\n" % (tag.encode(), mcid, 750 - 20 * (mcid % 35), text.encode())


def struct_lines(pdf):
    with t.TempDir() as d:
        path = d.write("s.pdf", pdf)
        out = utils.run("pdfinfo", "-struct-text", path)
    return [line.strip() for line in out.splitlines() if line.strip().startswith('"')]


pages = [mc("Alpha one", 0) + mc("Alpha two", 1), mc("Beta one", 0) + mc("Beta two", 1), mc("Gamma one", 0)]

# 1. /Pg given on each element: unchanged behaviour.
root = tp.Elem("Document", [tp.Elem("P", [tp.MC(0, 0)], pg=0), tp.Elem("P", [tp.MC(1, 0)], pg=1), tp.Elem("P", [tp.MC(1, 1)], pg=1), tp.Elem("P", [tp.MC(2, 0)], pg=2)])
got = struct_lines(tp.build_tagged(pages, root))
t.check(got == ['"Alpha one"', '"Beta one"', '"Beta two"', '"Gamma one"'], "explicit /Pg", got)

# 2. /Pg only on an ancestor, MCIDs as plain integers: inherited.
root = tp.Elem("Document", [tp.Elem("Sect", [tp.Elem("P", [tp.MC(1, 0, mcr=False)]), tp.Elem("P", [tp.MC(1, 1, mcr=False)])], pg=1), tp.Elem("Sect", [tp.Elem("P", [tp.MC(2, 0, mcr=False)])], pg=2)])
got = struct_lines(tp.build_tagged(pages, root))
t.check(got == ['"Beta one"', '"Beta two"', '"Gamma one"'], "/Pg inherited from an ancestor", got)

# 3. No /Pg anywhere: the page comes from the ParentTree, and the other
#    pages' MCID 0 text must not leak in.
root = tp.Elem("Document", [tp.Elem("P", [tp.MC(0, 0, mcr=False)]), tp.Elem("P", [tp.MC(1, 0, mcr=False)]), tp.Elem("P", [tp.MC(1, 1, mcr=False)]), tp.Elem("P", [tp.MC(2, 0, mcr=False)])])
got = struct_lines(tp.build_tagged(pages, root))
t.check(got == ['"Alpha one"', '"Beta one"', '"Beta two"', '"Gamma one"'], "no /Pg: page found via the ParentTree, no leaking", got)

# 4. Text of the same MCID on other pages must not be added to an element.
root = tp.Elem("Document", [tp.Elem("P", [tp.MC(2, 0, mcr=False)])])
got = struct_lines(tp.build_tagged(pages, root))
t.check(got == ['"Gamma one"'], "single element without /Pg gets only its own page", got)

# 5. Content in a form XObject (/Stm) is distinct from page content with the same MCID.
form = mc("In form", 0, tag="Span")
page_with_form = [mc("On page", 0) + b"/Fm0 Do\n"]
root = tp.Elem("Document", [tp.Elem("P", [tp.MC(0, 0)], pg=0), tp.Elem("Span", [tp.MC(0, 0, stm=0)], pg=0)])
got = struct_lines(tp.build_tagged(page_with_form, root, forms=[form]))
t.check(got == ['"On page"', '"In form"'], "/Stm form content is separate from page content", got)

# 6. Pages without /StructParents and no /Pg: an MCID that exists on exactly one
#    page is still resolved; an ambiguous one yields no text instead of leaking.
uniq_pages = [mc("Only on first", 0), mc("Only on second", 5) , mc("Shared A", 7) + mc("", 9)]
root = tp.Elem("Document", [tp.Elem("P", [tp.MC(0, 0, mcr=False)]), tp.Elem("P", [tp.MC(1, 5, mcr=False)])])
got = struct_lines(tp.build_tagged(uniq_pages, root, structparents=False))
t.check(got == ['"Only on first"', '"Only on second"'], "no /Pg and no /StructParents: unique MCID resolved", got)
ambiguous = [mc("First page", 0), mc("Second page", 0)]
root = tp.Elem("Document", [tp.Elem("P", [tp.MC(0, 0, mcr=False)])])
got = struct_lines(tp.build_tagged(ambiguous, root, structparents=False))
t.check("First pageSecond page" not in "".join(got) and "Second page" not in "".join(got), "ambiguous MCID without page info does not leak other pages", got)

# 7. Performance: a document with thousands of MCIDs must not re-interpret
#    every page for every MCID.
npages, per_page = 20, 120
big_pages = []
kids = []
for p in range(npages):
    big_pages.append(b"".join(mc("w%d_%d" % (p, i), i) for i in range(per_page)))
    kids.append(tp.Elem("Sect", [tp.Elem("P", [tp.MC(p, i)], pg=p) for i in range(per_page)]))
pdf = tp.build_tagged(big_pages, tp.Elem("Document", kids))
start = time.monotonic()
got = struct_lines(pdf)
elapsed = time.monotonic() - start
t.check(len(got) == npages * per_page and got[0] == '"w0_0"' and got[-1] == '"w19_119"', "large document struct text is complete", (len(got), got[:2]))
t.check(elapsed < 2.0, "2400 MCIDs on 20 pages extract in under 2 s (took %.2f s)" % elapsed, elapsed)

t.finish()
