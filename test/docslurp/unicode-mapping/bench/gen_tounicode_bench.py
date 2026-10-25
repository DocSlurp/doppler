#!/usr/bin/env python3
"""Generate a PDF whose Identity-H font has a large ToUnicode CMap, for timing mapToUnicode.

usage: gen_tounicode_bench.py OUT.pdf [--kind multi|single|nonbmp|unmapped] [--entries 20000] [--glyphs 40000]

  multi     every entry maps to 2 BMP code points (ligature-like: goes through sMap)
  single    every entry maps to 1 BMP code point (map[] only; the fast path)
  nonbmp    every entry maps to one supplementary-plane character (UTF-16 surrogate pair, sMap)
  unmapped  2-code-point entries on even codes only, text uses odd codes: every lookup misses with sMap non-empty
"""

import argparse
import os
import random
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")))
import docslurp_testlib as t  # noqa: E402


def tounicode(kind, entries):
    lines = [b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap", b"1 begincodespacerange <0000> <ffff> endcodespacerange"]
    covered = entries
    for start in range(0, covered, 100):
        chunk = range(start, min(start + 100, covered))
        if kind == "unmapped":
            chunk = [c for c in chunk if c % 2 == 0]
        lines.append(b"%d beginbfchar" % len(chunk))
        for code in chunk:
            if kind in ("multi", "unmapped"):
                dst = "%04X%04X" % (0x4E00 + code % 0x4000, 0x4E00 + (code * 7) % 0x4000)
            elif kind == "nonbmp":
                cp = 0x20000 + code - 0x10000
                dst = "%04X%04X" % (0xD800 + (cp >> 10), 0xDC00 + (cp & 0x3FF))
            else:
                dst = "%04X" % (0x4E00 + code % 0x4000)
            lines.append(("<%04X> <%s>" % (code, dst)).encode())
        lines.append(b"endbfchar")
    lines.append(b"endcmap end end")
    return b"\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--kind", default="multi", choices=["multi", "single", "nonbmp", "unmapped"])
    ap.add_argument("--entries", type=int, default=20000)
    ap.add_argument("--glyphs", type=int, default=40000)
    args = ap.parse_args()

    rnd = random.Random(1)
    per_line, lines_per_page = 40, 50
    per_page = per_line * lines_per_page
    npages = (args.glyphs + per_page - 1) // per_page
    codes = [rnd.randrange(args.entries) for _ in range(args.glyphs)]
    if args.kind == "unmapped":
        codes = [c | 1 for c in codes]

    objects = [None, None]  # catalog, pages: filled in below
    kids = []
    font_obj = 3 + 2 * npages
    for p in range(npages):
        page_codes = codes[p * per_page:(p + 1) * per_page]
        content = [b"BT /F1 10 Tf 12 TL 20 770 Td"]
        for i in range(0, len(page_codes), per_line):
            content.append(b"<" + b"".join(b"%04X" % c for c in page_codes[i:i + per_line]) + b"> Tj T*")
        content.append(b"ET")
        page_num = 3 + 2 * p
        kids.append(b"%d 0 R" % page_num)
        objects.append(b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 %d 0 R >> >> /Contents %d 0 R >>" % (font_obj, page_num + 1))
        objects.append(t.stream("", b"\n".join(content), compress=True))
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objects[1] = b"<< /Type /Pages /Kids [%s] /Count %d >>" % (b" ".join(kids), npages)
    # font_obj, then descendant, descriptor, ToUnicode
    objects.append(b"<< /Type /Font /Subtype /Type0 /BaseFont /BenchFont /Encoding /Identity-H /DescendantFonts [%d 0 R] /ToUnicode %d 0 R >>" % (font_obj + 1, font_obj + 3))
    objects.append(b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /BenchFont /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor %d 0 R /DW 500 >>" % (font_obj + 2))
    objects.append(b"<< /Type /FontDescriptor /FontName /BenchFont /Flags 4 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 700 /StemV 80 >>")
    objects.append(t.stream("", tounicode(args.kind, args.entries), compress=True))
    with open(args.out, "wb") as f:
        f.write(t.build_pdf(objects))


if __name__ == "__main__":
    main()
