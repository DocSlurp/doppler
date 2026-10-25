#!/usr/bin/env python3
"""Write the hostile/regression PDFs used by the benchmarks into OUTDIR.

usage: make_repros.py OUTDIR
"""

import os
import random
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(here))))
import docslurp_testlib as t  # noqa: E402

AFTER = b" BT /F1 12 Tf 72 700 Td (AFTER) Tj ET"


def coons_mesh(npatch):
    rnd = random.Random(1)
    data = bytearray()
    for _ in range(npatch):
        data.append(0)
        for _ in range(12):
            data += bytes([rnd.randrange(256), rnd.randrange(256)])
        data += bytes([0, 255, 0, 255])
    return bytes(data)


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)

    def w(name, data):
        with open(os.path.join(out, name), "wb") as f:
            f.write(data)

    w("inline-raw-16000.pdf", t.simple_page_pdf(b"BI /W 16000 /H 16000 /BPC 8 /CS /G ID xx EI"))
    w("inline-mask-40000.pdf", t.simple_page_pdf(b"BI /W 40000 /H 40000 /IM true ID xx EI"))
    w("inline-dct-16000.pdf", t.simple_page_pdf(b"BI /W 16000 /H 16000 /BPC 8 /CS /RGB /F /DCT ID \xff\xd8\xff\xc0\x00\x11\x08\x00\x10\x00\x10\x03\x01\x22\x00\x02\x11\x01\x03\x11\x01 EI" + AFTER))
    mesh = t.stream("/ShadingType 6 /ColorSpace /DeviceGray /BitsPerCoordinate 8 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [0 600 0 780 0 1]", coons_mesh(20))
    content = b"BT /F1 12 Tf 72 700 Td (BEFORE) Tj ET\n" + b"/Sh0 sh\n" * 400
    w("sh-mesh-x400.pdf", t.simple_page_pdf(content, resources="<< /Font << /F1 4 0 R >> /Shading << /Sh0 6 0 R >> >>", extra_objects=[mesh]))


main()
