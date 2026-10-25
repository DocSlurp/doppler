#!/usr/bin/env python3
"""Generate a large tagged PDF: gen_big_tagged.py OUT.pdf [PAGES] [MCIDS_PER_PAGE]

Each page has one Sect with one P per MCID, every MCR carrying /Pg
(the common case); with --no-pg the /Pg entries are omitted and the
integer MCID form is used (page found through the ParentTree).
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import tagged_pdf as tp  # noqa: E402


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    out = args[0]
    pages = int(args[1]) if len(args) > 1 else 20
    per_page = int(args[2]) if len(args) > 2 else 120
    no_pg = "--no-pg" in sys.argv
    contents, sects = [], []
    for p in range(pages):
        c = b""
        for i in range(per_page):
            y = 760 - 12 * (i % 60)
            x = 72 + 250 * (i // 60)
            c += b"/P <</MCID %d>> BDC BT /F1 9 Tf %d %d Td (Page %d item %d lorem ipsum dolor) Tj ET EMC\n" % (i, x, y, p, i)
        contents.append(c)
        if no_pg:
            sects.append(tp.Elem("Sect", [tp.Elem("P", [tp.MC(p, i, mcr=False)]) for i in range(per_page)]))
        else:
            sects.append(tp.Elem("Sect", [tp.Elem("P", [tp.MC(p, i)], pg=p) for i in range(per_page)]))
    open(out, "wb").write(tp.build_tagged(contents, tp.Elem("Document", sects)))


if __name__ == "__main__":
    main()
