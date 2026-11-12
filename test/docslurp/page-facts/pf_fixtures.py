"""Page-facts fixtures, built in memory: one PDF per fact family (A-J).

Image streams hold placeholder bytes: page facts read image dictionaries only, never data.
"""

import os
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402

HELV = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"


def doc(pages, extras=(), catalog_extra="", trailer_extra=""):
    """A PDF with numbered objects: 1 Catalog, 2 Pages, 3 Helvetica, then a page and its content
    stream per page, then <extras>. In any string, {N} is replaced by the object number of
    extras[N] and {P<i>} by that of page i (0-based)."""
    first_extra = 4 + 2 * len(pages)
    refs = {"P%d" % i: str(4 + 2 * i) for i in range(len(pages))}
    refs.update({str(i): str(first_extra + i) for i in range(len(extras))})

    def fill(text):
        text = text.decode("latin-1") if isinstance(text, bytes) else text
        for k, v in sorted(refs.items(), key=lambda kv: -len(kv[0])):
            text = text.replace("{%s}" % k, v)
        return text

    objects = [
        fill("<< /Type /Catalog /Pages 2 0 R %s >>" % catalog_extra),
        "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % (4 + 2 * i) for i in range(len(pages))), len(pages)),
        HELV,
    ]
    for i, p in enumerate(pages):
        box = " ".join(str(v) for v in p.get("mediabox", (0, 0, 612, 792)))
        res = fill(p.get("resources", "<< /Font << /F1 3 0 R >> >>"))
        objects.append(fill("<< /Type /Page /Parent 2 0 R /MediaBox [%s] /Resources %s /Contents %d 0 R %s >>" % (box, res, 5 + 2 * i, p.get("extra", ""))))
        content = p.get("content", b"")
        content = content if isinstance(content, bytes) else content.encode("latin-1")
        objects.append(t.stream("", content))
    for e in extras:
        objects.append(fill(e) if isinstance(e, str) else fill(e))
    return t.build_pdf(objects, trailer_extra=fill(trailer_extra))


def stream(dict_body, data):
    return t.stream(dict_body, data).decode("latin-1")


def words(n, y=700, invisible=False, font="F1"):
    """n words of text in lines of 10."""
    out = []
    mode = "3 Tr " if invisible else ""
    for i in range(0, n, 10):
        line = " ".join("word%d" % k for k in range(i, min(n, i + 10)))
        out.append("BT /%s 10 Tf %s72 %d Td (%s) Tj ET" % (font, mode, y - 12 * (i // 10), line))
    return "\n".join(out) + "\n"


def image(filter_name="/DCTDecode", w=2550, h=3300, cs="/DeviceRGB", bpc=8, extra=""):
    filt = "/Filter %s" % filter_name if filter_name else ""
    cs_part = "/ColorSpace %s" % cs if cs else ""
    bpc_part = "/BitsPerComponent %d" % bpc if bpc else ""
    return stream("/Type /XObject /Subtype /Image /Width %d /Height %d %s %s %s %s" % (w, h, cs_part, bpc_part, filt, extra), b"\xff\xd8placeholder\xff\xd9")


# --- A geometry ---------------------------------------------------------------


def geometry_pdf():
    text = words(20)
    return doc(
        [
            {"content": text},
            {"content": text, "extra": "/Rotate 90"},
            {"content": text, "mediabox": (0, 0, 20000, 20000)},
            {"content": text, "extra": "/UserUnit 2 /CropBox [50 50 500 700]"},
        ]
    )


# --- B text: scans with OCR layers, fonts, odd text -----------------------------


def scans_pdf():
    full = "q 612 0 0 792 0 0 cm /Im1 Do Q\n"
    strips = "".join("q 612 0 0 198 0 %d cm /Im2 Do Q\n" % (198 * k) for k in range(4))
    layered = "q 612 0 0 792 0 0 cm /Im1 Do Q\nq 612 0 0 792 0 0 cm /Mask1 Do Q\n"
    res = "<< /Font << /F1 3 0 R >> /XObject << /Im1 {0} 0 R /Im2 {1} 0 R /Mask1 {2} 0 R /Im3 {3} 0 R >> >>"
    return doc(
        [
            # a scan with an invisible OCR layer
            {"content": full + words(30, invisible=True), "resources": res},
            # a photo backdrop under 200 visible words: not a scan
            {"content": full + words(200), "resources": res},
            # four strips
            {"content": strips, "resources": res},
            # MRC: a colour background and a bilevel stencil text layer
            {"content": layered, "resources": res},
            # a soft-masked full-page image
            {"content": "q 612 0 0 792 0 0 cm /Im3 Do Q\n", "resources": res},
            # an inline image and a small picture
            {"content": "q 20 0 0 20 72 72 cm BI /W 2 /H 2 /CS /G /BPC 8 ID \x00\x80\x80\xff EI Q\nq 100 0 0 80 300 300 cm /Im1 Do Q\n" + words(10), "resources": res},
        ],
        extras=[
            image("/DCTDecode"),
            image("/FlateDecode", 2550, 825, "/DeviceGray", 8),
            image("/JBIG2Decode", 2550, 3300, None, 0, "/ImageMask true"),
            image("/JPXDecode", 2550, 3300, None, 0, "/SMask {4} 0 R"),
            image("/FlateDecode", 2550, 3300, "/DeviceGray", 8),
        ],
    )


CMAP = b"""/CIDInit /ProcSet findresource begin 12 dict begin begincmap
/CMapName /Odd def 1 begincodespacerange <00> <FF> endcodespacerange
4 beginbfchar <41> <E000> <42> <FFFD> <43> <2211> <44> <0001> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end"""


def fonts_pdf():
    type3 = (
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a {1} 0 R /b {1} 0 R >>"
        " /Encoding << /Type /Encoding /Differences [97 /a /b] >> /FirstChar 97 /LastChar 98 /Widths [600 600] /Resources << >> >>"
    )
    return doc(
        [
            {"content": "BT /T3 12 Tf 72 700 Td (abab abba baba) Tj ET\n", "resources": "<< /Font << /T3 {0} 0 R >> >>"},
            {"content": "BT /F2 12 Tf 72 700 Td (ABCD ABCD ABCD) Tj ET\n", "resources": "<< /Font << /F2 {2} 0 R >> >>"},
            # text turned a quarter: rotated content
            {"content": "".join("BT /F1 10 Tf 0 1 -1 0 %d 100 Tm (rotated words on a page) Tj ET\n" % (100 + 14 * k) for k in range(8))},
            # vertical writing
            {"content": "BT /V1 12 Tf 300 700 Td <0041004100410041> Tj ET\n", "resources": "<< /Font << /V1 {4} 0 R >> >>"},
        ],
        extras=[
            type3,
            stream("", "1000 0 0 0 1000 1000 d1 0 0 1000 1000 re f"),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /ToUnicode {3} 0 R >>",
            stream("", CMAP),
            "<< /Type /Font /Subtype /Type0 /BaseFont /MSMincho /Encoding /Identity-V /DescendantFonts [{5} 0 R] /ToUnicode {6} 0 R >>",
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /MSMincho /CIDSystemInfo << /Registry (Adobe) /Ordering (Japan1) /Supplement 2 >> /FontDescriptor {7} 0 R /DW 1000 >>",
            stream(
                "",
                b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapName /V def 1 begincodespacerange <0000> <FFFF> endcodespacerange "
                b"1 beginbfchar <0041> <65E5> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end",
            ),
            "<< /Type /FontDescriptor /FontName /MSMincho /Flags 4 /FontBBox [0 -141 1000 859] /ItalicAngle 0 /Ascent 859 /Descent -141 /CapHeight 700 /StemV 80 >>",
        ],
    )


# --- C images: encodings ------------------------------------------------------


def images_pdf():
    draws = "".join("q 100 0 0 100 %d %d cm /I%d Do Q\n" % (60 + 120 * (k % 4), 500 - 150 * (k // 4), k) for k in range(6))
    res = "<< /XObject << %s >> >>" % " ".join("/I%d {%d} 0 R" % (k, k) for k in range(6))
    return doc(
        [{"content": draws, "resources": res}],
        extras=[
            image("/DCTDecode", 300, 300, "/DeviceRGB", 8),
            image("/JPXDecode", 300, 300, None, 0),
            image("/JBIG2Decode", 300, 300, "/DeviceGray", 1),
            image("/CCITTFaxDecode", 300, 300, "/DeviceGray", 1, "/DecodeParms << /K -1 /Columns 300 >>"),
            image("/FlateDecode", 300, 300, "/DeviceCMYK", 8, "/SMask {5} 0 R"),
            image("[/ASCII85Decode /DCTDecode]", 300, 300, "/DeviceGray", 8, "/Decode [1 0]"),
        ],
    )


# --- D vectors ------------------------------------------------------------------


def vectors_pdf():
    grid = "0.5 w\n"
    for k in range(6):
        grid += "72 %d m 472 %d l S\n" % (600 - 40 * k, 600 - 40 * k)
        grid += "%d 400 m %d 600 l S\n" % (72 + 80 * k, 72 + 80 * k)
    cells = "".join("BT /F1 9 Tf %d %d Td (c%d) Tj ET\n" % (80 + 80 * (k % 5), 585 - 40 * (k // 5), k) for k in range(25))
    chart = "1 w 72 300 m\n" + "".join("%d %d %d %d %d %d c\n" % (80 + 20 * k, 320 + (k % 3) * 30, 90 + 20 * k, 280, 100 + 20 * k, 300 + (k % 4) * 20) for k in range(20)) + "S\n"
    chart += "".join("%d %d m %d %d l S\n" % (72 + 15 * k, 100, 72 + 15 * k + 10, 100 + 8 * k) for k in range(20))
    complexity = "/Sh1 sh\nq 100 100 200 200 re W n 0 0 612 792 re f Q\n/Pattern cs /P1 scn 300 300 100 100 re f\n"
    many = "".join("%d %d 1 1 re f\n" % (k % 600, (k // 600) % 780) for k in range(100005))
    return doc(
        [
            {"content": grid + cells},
            {"content": chart},
            {"content": complexity, "resources": "<< /Shading << /Sh1 {0} 0 R >> /Pattern << /P1 {1} 0 R >> >>"},
            {"content": many},
        ],
        extras=[
            "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >>",
            stream("/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 /Resources << >>", "0 0 5 5 re f"),
        ],
    )


# --- E forms ------------------------------------------------------------------------


def _ap(text):
    body = "/Tx BMC BT /Helv 10 Tf 2 5 Td (%s) Tj ET EMC" % text if text else ""
    return stream("/Type /XObject /Subtype /Form /BBox [0 0 150 20] /Resources << /Font << /Helv 3 0 R >> >>", body)


def forms_pdf(xfa=None):
    # extras: 0 AcroForm field list helper objects follow
    fields = [
        # (dict, appearance text or None)
        ("/FT /Tx /T (SecretFieldName) /V (Hello)", "Hello"),  # filled, shows it
        ("/FT /Tx /T (empty)", None),  # empty
        ("/FT /Tx /T (mismatch) /V (Value)", None),  # value without appearance
        ("/FT /Btn /T (cb1) /V /Yes /AS /Yes", "BTN"),  # checkbox on
        ("/FT /Btn /T (cb2) /V /Off /AS /Off", "BTN"),  # checkbox off
        ("/FT /Ch /T (ch) /V (opt) /Opt [(opt) (other)]", "opt"),  # choice
        ("/FT /Sig /T (sig1)", None),  # unsigned
        ("/FT /Sig /T (sig2) /V << /Type /Sig /Filter /Adobe.PPKLite >>", None),  # signed
        ("/FT /Btn /T (push) /Ff 65536", "Go"),  # push button
    ]
    extras = []
    refs = []
    n_fixed = 2  # extras 0 (AcroForm) and 1 (XFA) come first
    for i, (d, text) in enumerate(fields):
        ap_index = n_fixed + 2 * i + 1
        if "/Btn" in d and "Ff" not in d:
            apn = "<< /Yes {%d} 0 R /Off {%d} 0 R >>" % (ap_index, ap_index)
        else:
            apn = "{%d} 0 R" % ap_index
        extras.append("<< /Type /Annot /Subtype /Widget /Rect [72 %d 222 %d] /F 4 /P {P0} %s /AP << /N %s >> >>" % (700 - 30 * i, 720 - 30 * i, d, apn))
        extras.append(_ap(text if text and text != "BTN" else ("" if text is None else "X")))
        refs.append("{%d} 0 R" % (n_fixed + 2 * i))
    # a radio group: the parent holds /V; each kid has its own on state
    radio_parent = len(extras) + n_fixed
    extras.append("<< /FT /Btn /Ff 32768 /T (radio) /V /B /Kids [{%d} 0 R {%d} 0 R] >>" % (radio_parent + 1, radio_parent + 2))
    for k, (state, y) in enumerate((("A", 300), ("B", 270))):
        on = "/%s {%d} 0 R" % (state, radio_parent + 3)
        extras.append("<< /Type /Annot /Subtype /Widget /Parent {%d} 0 R /Rect [72 %d 92 %d] /F 4 /P {P0} /AS /%s /AP << /N << %s /Off {%d} 0 R >> >> >>" % (radio_parent, y, y + 20, state if state == "B" else "Off", on, radio_parent + 3))
    extras.append(_ap("X"))
    refs += ["{%d} 0 R" % (radio_parent + 1), "{%d} 0 R" % (radio_parent + 2)]
    xfa_part = " /XFA {1} 0 R" if xfa else ""
    acro = "<< /Fields [%s {%d} 0 R] /NeedAppearances false%s >>" % (" ".join(refs[:-2]), radio_parent, xfa_part)
    if xfa == "dynamic":
        xfa_obj = "[(preamble) {%d} 0 R (config) {%d} 0 R]" % (len(extras) + n_fixed, len(extras) + n_fixed + 1)
        tail = [stream("", "<xdp:xdp>"), stream("", "<config><acrobat><acrobat7><dynamicRender>\n required </dynamicRender></acrobat7></acrobat></config>")]
    elif xfa == "static":
        xfa_obj = stream("", "<xdp:xdp><config><present><pdf/></present></config></xdp:xdp>")
        tail = []
    else:
        xfa_obj = "null"
        tail = []
    all_extras = [acro, xfa_obj] + extras + tail
    return doc(
        [{"content": words(10, y=760), "extra": "/Annots [%s]" % " ".join(refs)}],
        extras=all_extras,
        catalog_extra="/AcroForm {0} 0 R",
    )


# --- F annotations ---------------------------------------------------------------------

ANNOT_SUBTYPES = ["Text", "FreeText", "Highlight", "Underline", "StrikeOut", "Squiggly", "Ink", "Stamp", "Square", "Circle", "Line", "Polygon", "FileAttachment", "Redact", "Popup", "Caret"]


def annots_pdf():
    extras = []
    refs = []
    for i, sub in enumerate(ANNOT_SUBTYPES):
        extra = ""
        if sub == "Square":
            extra = "/AP << /N {APSQ} 0 R >>"
        if sub == "FreeText":
            extra = "/DA (/Helv 10 Tf 0 g) /AP << /N {APFT} 0 R >>"
        if sub == "Ink":
            extra = "/InkList [[100 100 120 120]]"
        if sub == "Line":
            extra = "/L [100 100 200 200]"
        if sub == "Polygon":
            extra = "/Vertices [100 100 150 100 120 150]"
        if sub in ("Highlight", "Underline", "StrikeOut", "Squiggly", "Redact"):
            extra = "/QuadPoints [72 %d 172 %d 72 %d 172 %d]" % (720 - 40 * i, 720 - 40 * i, 700 - 40 * i, 700 - 40 * i)
        if sub == "FileAttachment":
            extra = "/FS << /Type /Filespec /F (secret-attachment.txt) >>"
        extras.append("<< /Type /Annot /Subtype /%s /Rect [72 %d 172 %d] /F 4 /Contents (note %d) %s >>" % (sub, 700 - 40 * (i % 16), 720 - 40 * (i % 16), i, extra))
        refs.append("{%d} 0 R" % i)
    n = len(extras)
    extras.append("<< /Type /Annot /Subtype /Link /Rect [300 700 400 720] /A << /S /URI /URI (https://secret.example/private/path) >> >>")
    extras.append("<< /Type /Annot /Subtype /Link /Rect [300 650 400 670] /Dest [{P1} /Fit] >>")
    refs += ["{%d} 0 R" % n, "{%d} 0 R" % (n + 1)]
    extras.append(stream("/Type /XObject /Subtype /Form /BBox [0 0 100 20]", "1 0 0 rg 0 0 100 20 re f"))
    extras.append(stream("/Type /XObject /Subtype /Form /BBox [0 0 100 20] /Resources << /Font << /Helv 3 0 R >> >>", "BT /Helv 10 Tf 2 5 Td (free text) Tj ET"))
    aps = {"APSQ": str(n + 2), "APFT": str(n + 3)}
    out = []
    for e in extras:
        for k, v in aps.items():
            e = e.replace("{%s}" % k, "{%s}" % v)
        out.append(e)
    return doc([{"content": words(10), "extra": "/Annots [%s]" % " ".join(refs)}, {"content": words(5)}], extras=out)


# --- G optional content -----------------------------------------------------------------------


def ocg_pdf():
    content = "/OC /oc1 BDC\n" + words(5, y=700) + "EMC\n/OC /oc2 BDC\n" + words(5, y=500) + "0 0 100 100 re f\nEMC\n"
    res = "<< /Font << /F1 3 0 R >> /Properties << /oc1 {0} 0 R /oc2 {1} 0 R >> >>"
    return doc(
        [{"content": content, "resources": res}, {"content": words(5)}],
        extras=["<< /Type /OCG /Name (Shown) >>", "<< /Type /OCG /Name (Hidden) >>"],
        catalog_extra="/OCProperties << /OCGs [{0} 0 R {1} 0 R] /D << /Order [{0} 0 R {1} 0 R] /OFF [{1} 0 R] >> >>",
    )


# --- H document level -----------------------------------------------------------------------------

PRODUCER = ("<&>\"q\\ " * 400 + "été \U0001f600").encode("utf-16-be")


def structure_pdf(lang="(en-US)"):
    producer = "<FEFF" + PRODUCER.hex().upper() + ">"
    return doc(
        [{"content": words(10)}, {"content": words(10)}],
        extras=[
            "<< /Producer %s /Creator (Writer\x01\x02 9) >>" % producer,
            "<< /Names [(doc.js) {2} 0 R] >>",
            "<< /S /JavaScript /JS (app.alert\\(1\\)) >>",
            "<< /Names [(a.txt) {4} 0 R (b.txt) {4} 0 R] >>",
            "<< /Type /Filespec /F (a.txt) >>",
            "<< /Type /Outlines /First {6} 0 R /Last {7} 0 R /Count 2 >>",
            "<< /Title (One) /Parent {5} 0 R /Next {7} 0 R /Dest [{P0} /Fit] /First {8} 0 R /Last {8} 0 R /Count 1 >>",
            "<< /Title (Two) /Parent {5} 0 R /Prev {6} 0 R /Dest [{P1} /Fit] >>",
            "<< /Title (One.1) /Parent {6} 0 R /Dest [{P0} /Fit] >>",
            "<< /Type /StructTreeRoot >>",
        ],
        catalog_extra="/Names << /JavaScript {1} 0 R /EmbeddedFiles {3} 0 R >> /OpenAction {2} 0 R /Outlines {5} 0 R /PageLabels << /Nums [0 << /S /r >>] >> /StructTreeRoot {9} 0 R /MarkInfo << /Marked true >> /Lang %s" % lang,
        trailer_extra="/Info {0} 0 R",
    )


def damaged_pdf():
    """A valid two-page PDF whose xref offsets are all wrong: poppler rebuilds it."""
    pdf = doc([{"content": words(10)}, {"content": words(10)}])
    import re

    head, sep, tail = pdf.partition(b"xref\n")
    tail = re.sub(rb"(?m)^(\d{10})( \d{5} n )$", lambda m: b"%010d" % (int(m.group(1)) + 7) + m.group(2), tail)
    return head + sep + tail


def incremental_pdf():
    """A PDF with one incremental update (an appended section with /Prev)."""
    base = doc([{"content": words(10)}])
    prev = int(base.rsplit(b"startxref\n", 1)[1].split(b"\n")[0])
    off = len(base)
    obj = b"6 0 obj\n<< /Producer (updated) >>\nendobj\n"
    xref = off + len(obj)
    upd = obj + b"xref\n0 1\n0000000000 65535 f \n6 1\n%010d 00000 n \ntrailer\n<< /Size 7 /Root 1 0 R /Info 6 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n" % (off, prev, xref)
    return base + upd


def warnings_pdf():
    return doc([{"content": words(5) + "1 2 foo\nBT /Nope 10 Tf 72 72 Td (x) Tj ET\n"}, {"content": words(5)}])


def multipage_pdf():
    """Pages that differ (text, an image, a table) for page locality and determinism."""
    res = "<< /Font << /F1 3 0 R >> /XObject << /Im1 {0} 0 R >> >>"
    grid = "".join("72 %d m 472 %d l S\n%d 400 m %d 600 l S\n" % (600 - 40 * k, 600 - 40 * k, 72 + 80 * k, 72 + 80 * k) for k in range(6))
    return doc(
        [
            {"content": words(40), "resources": res},
            {"content": "q 612 0 0 792 0 0 cm /Im1 Do Q\n" + words(30, invisible=True), "resources": res},
            {"content": grid + words(10), "resources": res},
            {"content": "q 200 0 0 150 72 420 cm /Im1 Do Q\n" + words(20), "resources": res, "extra": "/Rotate 270"},
            {"content": words(5), "resources": res},
        ],
        extras=[image("/DCTDecode")],
    )


ALL = {
    "geometry": geometry_pdf,
    "scans": scans_pdf,
    "fonts": fonts_pdf,
    "images": images_pdf,
    "vectors": vectors_pdf,
    "forms": forms_pdf,
    "forms_xfa_static": lambda: forms_pdf("static"),
    "forms_xfa_dynamic": lambda: forms_pdf("dynamic"),
    "annots": annots_pdf,
    "ocg": ocg_pdf,
    "structure": structure_pdf,
    "damaged": damaged_pdf,
    "incremental": incremental_pdf,
    "warnings": warnings_pdf,
    "multipage": multipage_pdf,
}
