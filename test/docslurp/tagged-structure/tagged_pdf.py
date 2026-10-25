"""Builders for small tagged PDFs used by the tagged-structure regression tests.

Import by path (tests run with ``python3 -I``)::

    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import tagged_pdf as tp
"""

import os
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402


class MC:
    """Marked-content reference kid. ``mcr`` selects the /MCR dictionary form (with /Pg)."""

    def __init__(self, page, mcid, mcr=True):
        self.page = page
        self.mcid = mcid
        self.mcr = mcr


class Elem:
    """Structure element. ``pg`` is a page index or None; ``extra`` is raw dictionary text."""

    def __init__(self, s, kids=(), pg=None, extra=""):
        self.s = s
        self.kids = list(kids)
        self.pg = pg
        self.extra = extra


def build_tagged(pages, root, role_map="", structparents=True, page_resources=""):
    """Build a tagged PDF.

    pages: list of content-stream byte strings (page i uses /StructParents i).
    root:  Elem used as the single child of /StructTreeRoot.
    """
    n = len(pages)
    objs = {}
    font_num = 3
    page_num = [4 + 2 * i for i in range(n)]
    cont_num = [5 + 2 * i for i in range(n)]
    next_num = [4 + 2 * n]
    root_num = next_num[0]
    next_num[0] += 1
    parent_tree_num = next_num[0]
    next_num[0] += 1

    entries = {}  # page -> {mcid: elem objnum}
    bodies = {}

    def walk(e, parent_num):
        num = next_num[0]
        next_num[0] += 1
        kid_items = []
        child_bodies = []
        for k in e.kids:
            if isinstance(k, MC):
                pg = k.page
                if k.mcr:
                    kid_items.append("<< /Type /MCR /Pg %d 0 R /MCID %d >>" % (page_num[pg], k.mcid))
                else:
                    kid_items.append("%d" % k.mcid)
                entries.setdefault(pg, {})[k.mcid] = num
            else:
                kid_items.append("%d 0 R" % walk(k, num))
        body = "<< /Type /StructElem /S /%s /P %d 0 R " % (e.s, parent_num)
        if e.pg is not None:
            body += "/Pg %d 0 R " % page_num[e.pg]
        body += e.extra
        if len(kid_items) == 1:
            body += " /K %s" % kid_items[0]
        elif kid_items:
            body += " /K [%s]" % " ".join(kid_items)
        body += " >>"
        bodies[num] = body
        return num

    top = walk(root, root_num)

    nums = []
    for pg in sorted(entries):
        m = entries[pg]
        arr = ["%d 0 R" % m[i] if i in m else "null" for i in range(max(m) + 1)]
        nums.append("%d [%s]" % (pg, " ".join(arr)))
    objs[1] = "<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot %d 0 R >>" % root_num
    objs[2] = "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % p for p in page_num), n)
    objs[font_num] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
    for i, c in enumerate(pages):
        objs[page_num[i]] = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> %s >> /Contents %d 0 R %s>>" % (
            page_resources, cont_num[i], ("/StructParents %d " % i) if structparents else "")
        objs[cont_num[i]] = t.stream("", c)
    rm = (" /RoleMap << %s >>" % role_map) if role_map else ""
    objs[root_num] = "<< /Type /StructTreeRoot /K %d 0 R /ParentTree %d 0 R%s >>" % (top, parent_tree_num, rm)
    objs[parent_tree_num] = "<< /Nums [%s] >>" % " ".join(nums)
    objs.update(bodies)
    total = max(objs)
    return t.build_pdf([objs[i] for i in range(1, total + 1)])


def tj(text, size=12, x=72, y=700):
    return b"BT /F1 %d Tf %d %d Td (%s) Tj ET" % (size, x, y, text.encode("latin-1"))
