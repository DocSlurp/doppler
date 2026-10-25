"""PDF fixture builders shared by the unicode-mapping regression tests and benchmarks."""

import os
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402


def type0_page(content, encoding, tounicode=None, ordering="Japan1", registry="Adobe", extra_page_objects=()):
    """One-page PDF using font /F1: an unembedded Type0 font over a CIDFontType0 descendant.

    Objects: 1 Catalog, 2 Pages, 3 Page, 4 Type0 font, 5 contents, 6 descendant font,
    7 font descriptor, 8 ToUnicode stream (only if ``tounicode`` is given).
    """
    font = "<< /Type /Font /Subtype /Type0 /BaseFont /KozMinPro-Regular /Encoding /%s /DescendantFonts [6 0 R]" % encoding
    if tounicode is not None:
        font += " /ToUnicode 8 0 R"
    font += " >>"
    descendant = (
        "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /KozMinPro-Regular "
        "/CIDSystemInfo << /Registry (%s) /Ordering (%s) /Supplement 6 >> /FontDescriptor 7 0 R /DW 1000 >>" % (registry, ordering)
    )
    descriptor = "<< /Type /FontDescriptor /FontName /KozMinPro-Regular /Flags 4 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 700 /StemV 80 >>"
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        font,
        t.stream("", content),
        descendant,
        descriptor,
    ]
    if tounicode is not None:
        objects.append(t.stream("", tounicode))
    objects.extend(extra_page_objects)
    return t.build_pdf(objects)
