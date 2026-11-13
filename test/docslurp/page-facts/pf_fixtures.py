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


def scan_thresholds_pdf():
    """One page at each scan threshold's boundary (-scan-min-coverage, -scan-layer-overlap,
    -scan-max-words); with the defaults none of them is layered or a scan beyond page 2's single."""
    res = "<< /Font << /F1 3 0 R >> /XObject << /Im1 {0} 0 R /Im2 {1} 0 R >> >>"
    full = "q 612 0 0 792 0 0 cm /Im1 Do Q\n"
    return doc(
        [
            # one image over 650/792 = 0.8207 of the page (grid union 53/64 = 0.828)
            {"content": "q 612 0 0 650 0 0 cm /Im1 Do Q\n", "resources": res},
            # a full-page image and a second over 330/792 = 0.4167 of the page, on top of it
            {"content": full + "q 612 0 0 330 0 0 cm /Im2 Do Q\n", "resources": res},
            # a full-page image under 25 visible words
            {"content": full + words(25), "resources": res},
        ],
        extras=[image("/DCTDecode"), image("/FlateDecode", 2550, 1100, "/DeviceGray", 8)],
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



def bars_pdf():
    """Bar charts drawn only with filled boxes (doppler#38), and box-heavy pages that are not charts."""
    axis = "0 g 0.5 w 100 300 m 400 300 l S 100 300 m 100 520 l S 96 400 m 100 400 l S\n"
    # tick labels: a scale left of the plot (bar charts have one; redactions and blank forms do not)
    axis += "".join("BT /F1 8 Tf 84 %d Td (%s) Tj ET\n" % (y, v) for y, v in ((297, "0"), (397, "50"), (497, "100")))
    vertical = "0.16 0.62 0.56 rg 110 300 30 80 re f 160 300 30 130 re f 210 300 30 50 re f 260 300 30 170 re f\n" + axis
    haxis = "0 g 0.5 w 100 380 m 100 540 l S 100 380 m 420 380 l S\n"
    haxis += "".join("BT /F1 8 Tf %d 370 Td (%s) Tj ET\n" % (x, v) for x, v in ((98, "0"), (198, "100"), (298, "200")))
    horizontal = "0.48 0.38 1 rg 100 400 200 20 re f 100 440 120 20 re f 100 480 280 20 re f\n" + haxis
    # a shaded table: cells of different widths on one row, each holding words, on a rule
    table = "0 g 0.5 w 72 300 m 540 300 l S\n0.85 0.85 0.85 rg 72 300 128 20 re f 200 300 200 20 re f 400 300 140 20 re f\n"
    table += "0 g " + "".join("BT /F1 10 Tf %d 306 Td (%s) Tj ET\n" % (x, w) for x, w in ((80, "Region"), (210, "Revenue"), (410, "Growth")))
    # zebra stripes: rows of one length on a shared left edge, beside a rule
    zebra = "0 g 0.5 w 72 200 m 72 560 l S\n0.95 0.95 0.95 rg " + "".join("72 %d 468 15 re f " % (200 + 30 * k) for k in range(6)) + "\n"
    # highlighted text: boxes behind words, on a rule
    highlight = "0 g 0.5 w 72 380 m 540 380 l S\n1 1 0 rg 72 380 78 20 re f 160 380 100 30 re f 270 380 30 10 re f\n0 g "
    highlight += "".join("BT /F1 10 Tf %d 385 Td (%s) Tj ET\n" % (x, w) for x, w in ((75, "launch"), (165, "expansion"), (272, "go")))
    # cost (council M4): 1,320 runs of three 2 pt boxes (thicker than the 1.5 pt the producer reads as a rule) that pass the
    # spread test, each with its own axis, plus 4,000 short strokes: past the 64-run cap, and every check is reached
    many = "0.2 0.4 0.8 rg "
    for row in range(66):
        y = 90 + 9 * row
        for run in range(20):
            x = 40 + 27 * run
            many += "%d %d 2 3 re f %d %d 2 5 re f %d %d 2 4 re f\n" % (x, y, x + 3, y, x + 6, y)
            # a scale left of each run: every number on the page at its own height, its value increasing with height
            many += "".join("BT /F1 3 Tf %.2f %.2f Td (%d) Tj ET\n" % (x - 9, y + 1 + 2.5 * k + 0.05 * run, round((y + 2.5 + 2.5 * k + 0.05 * run) * 100)) for k in range(3))
    many += "0 g 0.4 w " + "".join("%d %d m %d %d l S\n" % (40 + 27 * run - 1, 90 + 9 * row, 40 + 27 * run + 9, 90 + 9 * row) for row in range(66) for run in range(20))
    many += "".join("%d 60 m %d 62 l S\n" % (30 + k % 500, 30 + k % 500) for k in range(4000))
    # council M1/M2 negatives: a redacted table column (black boxes, ragged, by the column border), a redacted paragraph
    redacted = "0 g 0.5 w 72 300 m 540 300 l S 72 300 m 72 500 l S 200 300 m 200 500 l S\n0 g " + "".join("202 %d %d 10 re f " % (310 + 30 * k, 60 + 23 * k) for k in range(6)) + "\n"
    redacted += "".join("BT /F1 10 Tf 80 %d Td (Witness) Tj ET\n" % (310 + 30 * k) for k in range(6))
    paragraph = "0.04 0.04 0.04 rg " + "".join("72 %d %d 11 re f " % (600 - 14 * k, 468 - 31 * k) for k in range(6)) + "\n0 g 0.5 w 72 520 m 540 520 l S\n"
    # tinted form fields stacked on one left edge, each filled and stroked (B): its own outline is no axis
    form = "0 G 0.5 w 0.88 0.93 1 rg " + "".join("200 %d %d 18 re B " % (500 - 30 * k, w) for k, w in enumerate((300, 120, 200, 80))) + "\n0 g "
    form += "".join("BT /F1 10 Tf 100 %d Td (Name) Tj ET\n" % (504 - 30 * k) for k in range(4))
    # a shaded two-level table header: a rowspan cell sharing its bottom edge with sub-header cells, every cell holding text
    header = "0 g 0.5 w 72 500 m 540 500 l S 72 500 m 72 560 l S\n0.85 0.85 0.85 rg 72 500 100 40 re f 172 500 120 20 re f 292 500 120 20 re f 412 500 128 20 re f\n0 g "
    header += "".join("BT /F1 8 Tf %d 504 Td (%s) Tj ET\n" % (x, w) for x, w in ((80, "Region"), (180, "Q1"), (300, "Q2"), (420, "Q3")))
    # zebra rows holding text, and a narrower shaded note box on the same left edge
    note = "0 g 0.5 w 72 200 m 72 560 l S\n0.95 0.95 0.95 rg " + "".join("72 %d 468 15 re f " % (200 + 30 * k) for k in range(6)) + "72 380 300 15 re f\n0 g "
    note += "".join("BT /F1 8 Tf 76 %d Td (Bookings %d) Tj ET\n" % (204 + 30 * k, k) for k in range(6)) + "BT /F1 8 Tf 76 384 Td (Note: record year) Tj ET\n"
    # yellow highlights behind 8 pt values in a ruled table
    marks = "0 g 0.5 w 72 300 m 540 300 l S 72 300 m 72 420 l S\n1 1 0 rg " + "".join("%d 300 %d 12 re f " % (100 + 110 * k, 30 + 12 * k) for k in range(4)) + "\n0 g "
    marks += "".join("BT /F1 8 Tf %d 303 Td (%d.5) Tj ET\n" % (102 + 110 * k, 10 + k) for k in range(4))
    # bars with a drawn trend line: one cluster (joined into the drawing's), as doppler-md makes one figure
    curve = "0 0 1 RG 1 w 110 380 m 150 420 190 360 230 430 c 270 400 300 460 330 410 c S\n"
    # council r2: redactions in CMYK black, grey and navy, each a cell 1 pt inside the column rule (which spans the run)
    def redactions(fill):
        page = "0 g 0.5 w 72 300 m 540 300 l S 72 300 m 72 500 l S 200 300 m 200 500 l S\n" + fill + " " + "".join("201 %d %d 10 re f " % (310 + 30 * k, 60 + 23 * k) for k in range(6)) + "\n0 g "
        return page + "".join("BT /F1 10 Tf 80 %d Td (Witness) Tj ET\n" % (310 + 30 * k) for k in range(6))
    # a blank flattened form: tinted fields of different widths, 0.5 pt right of a divider that runs down the whole form
    divider = "0 g 0.5 w 150 200 m 150 560 l S\n0.86 0.91 1 rg " + "".join("150.5 %d %d 18 re f " % (520 - 40 * k, w) for k, w in enumerate((330, 70, 200, 140, 260))) + "\n0 g "
    divider += "".join("BT /F1 10 Tf 80 %d Td (Field) Tj ET\n" % (524 - 40 * k) for k in range(5))
    # a black bar chart (common in print): bars filled 0 g, with its axes and scale
    black = "0 g 110 300 30 80 re f 160 300 30 130 re f 210 300 30 50 re f 260 300 30 170 re f\n" + axis
    combo = vertical + curve
    return doc([{"content": vertical}, {"content": horizontal}, {"content": table}, {"content": zebra}, {"content": highlight}, {"content": many},
                {"content": redacted}, {"content": paragraph}, {"content": form}, {"content": header}, {"content": note}, {"content": marks},
                {"content": combo}, {"content": curve},
                {"content": redactions("0 0 0 1 k")}, {"content": redactions("0.15 g")}, {"content": redactions("0.05 0.05 0.2 rg")}, {"content": divider},
                {"content": black}])


def curve_pdf():
    """The trend line of bars_pdf's last page alone: its facts must not depend on the pages before it."""
    return doc([{"content": "0 0 1 RG 1 w 110 380 m 150 420 190 360 230 430 c 270 400 300 460 330 410 c S\n"}])

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
    if xfa in ("dynamic", "dynamic-codec"):
        # dynamic-codec: the config packet behind an image codec, which is never run to read it
        xfa_obj = "[(preamble) {%d} 0 R (config) {%d} 0 R]" % (len(extras) + n_fixed, len(extras) + n_fixed + 1)
        codec = "/Filter /JPXDecode" if xfa == "dynamic-codec" else ""
        tail = [stream("", "<xdp:xdp>"), stream(codec, "<config><acrobat><acrobat7><dynamicRender>\n required </dynamicRender></acrobat7></acrobat></config>")]
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


def _objects_of(pdf):
    """The object bodies before a classic xref table, and each object's offset."""
    import re

    body = pdf[: pdf.rindex(b"xref\n")]
    return body, {int(m.group(1)): m.start() for m in re.finditer(rb"(?m)^(\d+) 0 obj", body)}


def _xref_stream(num, entries, index, trailer):
    """An xref stream object <num>: entries (type, field 2, field 3) for the subsections <index>."""
    data = b"".join(bytes([k]) + a.to_bytes(4, "big") + b.to_bytes(2, "big") for k, a, b in entries)
    head = b"%d 0 obj\n<< /Type /XRef /W [1 4 2] /Index [%s] %s /Length %d >>\nstream\n" % (num, b" ".join(b"%d %d" % i for i in index), trailer, len(data))
    return head + data + b"\nendstream\nendobj\n"


def xref_stream_pdf(update=None):
    """One page with a cross-reference stream instead of a table. <update>: None, "stream" (an
    incremental update whose section is another xref stream) or "table-then-stream" (a classic
    file updated with an xref stream section)."""
    base = doc([{"content": words(10)}])
    body, offs = _objects_of(base)
    n = max(offs) + 1
    out = bytearray(body)
    if update == "table-then-stream":
        out = bytearray(base)
        prev = int(base.rsplit(b"startxref\n", 1)[1].split(b"\n")[0])
    else:
        prev = len(out)
        entries = [(0, 0, 65535)] + [(1, offs[i], 0) for i in range(1, n)] + [(1, prev, 0)]
        out += _xref_stream(n, entries, [(0, n + 1)], b"/Size %d /Root 1 0 R" % (n + 1))
        if update is None:
            out += b"startxref\n%d\n%%%%EOF\n" % prev
            return bytes(out)
        out += b"startxref\n%d\n%%%%EOF\n" % prev
    info = n + 1
    info_off = len(out)
    out += b"%d 0 obj\n<< /Producer (updated) >>\nendobj\n" % info
    x2 = len(out)
    out += _xref_stream(info + 1, [(1, info_off, 0), (1, x2, 0)], [(info, 2)], b"/Size %d /Root 1 0 R /Info %d 0 R /Prev %d" % (info + 2, info, prev))
    out += b"startxref\n%d\n%%%%EOF\n" % x2
    return bytes(out)


def objstm_pdf(plain, packed):
    """A PDF whose objects <packed> ({number: body}) sit in one Flate object stream, with an xref
    stream; <plain> ({number: body}) are ordinary objects. Object 1 must be the catalog."""
    import zlib

    out = bytearray(b"%PDF-1.7\n")
    offs = {}
    for num in sorted(plain):
        offs[num] = len(out)
        out += b"%d 0 obj\n" % num + t._b(plain[num]) + b"\nendobj\n"
    nums = sorted(packed)
    stm = max(list(plain) + nums) + 1
    bodies, header, pos = b"", b"", 0
    for num in nums:
        header += b"%d %d " % (num, pos)
        body = t._b(packed[num]) + b"\n"
        bodies += body
        pos += len(body)
    data = zlib.compress(header + bodies, 9)
    offs[stm] = len(out)
    out += b"%d 0 obj\n<< /Type /ObjStm /N %d /First %d /Filter /FlateDecode /Length %d >>\nstream\n" % (stm, len(nums), len(header), len(data)) + data + b"\nendstream\nendobj\n"
    xs = stm + 1
    entries = [(0, 0, 65535)]
    for num in range(1, xs + 1):
        if num == xs:
            entries.append((1, len(out), 0))
        elif num in offs:
            entries.append((1, offs[num], 0))
        else:
            entries.append((2, stm, nums.index(num)))
    x = len(out)
    out += _xref_stream(xs, entries, [(0, xs + 1)], b"/Size %d /Root 1 0 R" % (xs + 1))
    out += b"startxref\n%d\n%%%%EOF\n" % x
    return bytes(out)


def shared_widget_value_pdf(widgets=2000, items=300000):
    """One page whose widgets all name one large indirect /V (each re-fetch would re-parse it)."""
    w = "<< /Type /Annot /Subtype /Widget /Rect [0 0 1 1] /FT /Tx /V {1} 0 R >>"
    return doc([{"extra": "/Annots {0} 0 R"}], extras=["[" + w * widgets + "]", "[" + "1 " * items + "]"])


def shared_widget_string_pdf(pages=3, widgets=9999, size=8 << 20):
    """Pages sharing one /Annots array of widgets that all name one large string /V, packed in a
    Flate object stream (a small file): copying the string per widget is gigabytes."""
    plain = {1: b"<< /Type /Catalog /Pages 2 0 R >>", 2: b"<< /Type /Pages /Kids [" + b" ".join(b"%d 0 R" % (5 + i) for i in range(pages)) + b"] /Count %d >>" % pages}
    plain[3] = b"[" + b"<< /Type /Annot /Subtype /Widget /Rect [0 0 1 1] /FT /Tx /V 4 0 R >>" * widgets + b"]"
    for i in range(pages):
        plain[5 + i] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots 3 0 R >>"
    return objstm_pdf(plain, {4: b"(" + b"A" * size + b")"})


def form_cache_fill_pdf(fill_pages=3, big_pages=2, items=300000, widgets=9999, distinct=False):
    """Pages of filler widgets naming many refs, then pages sharing one /Annots of widgets whose
    /V is a large uncompressed array. distinct=False: the filler refs are generation variants of
    one compressed object ("4 0 R", "4 1 R", ...), which XRef::fetch reads as the same object;
    distinct=True: each names its own small object, past the form-object budget."""
    plain = {1: b"<< /Type /Catalog /Pages 2 0 R >>"}
    npages = fill_pages + big_pages
    first_page = 6 + max(fill_pages, 4)
    plain[2] = b"<< /Type /Pages /Kids [" + b" ".join(b"%d 0 R" % (first_page + i) for i in range(npages)) + b"] /Count %d >>" % npages
    plain[3] = b"[" + b"<< /Type /Annot /Subtype /Widget /Rect [0 0 1 1] /FT /Tx /V 5 0 R >>" * widgets + b"]"
    plain[5] = b"[" + b"1 " * items + b"]"
    first_obj = first_page + npages
    packed = {4: b"null"}
    g = 0
    for k in range(fill_pages):
        ws = []
        for i in range(widgets):
            if distinct:
                ws.append(b"<< /Type /Annot /Subtype /Widget /Rect [0 0 1 1] /FT /Tx /V %d 0 R >>" % (first_obj + g))
                plain[first_obj + g] = b"(v)"
                g += 1
            else:
                ws.append(b"<< /Type /Annot /Subtype /Widget /Rect [0 0 1 1] /FT 4 %d R /Ff 4 %d R /V 4 %d R >>" % (g, g + 1, g + 2))
                g += 3
        plain[6 + k] = b"[" + b"".join(ws) + b"]"
    for k in range(fill_pages, 4):
        plain[6 + k] = b"null"
    for i in range(npages):
        annots = (6 + i) if i < fill_pages else 3
        plain[first_page + i] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots %d 0 R >>" % annots
    return objstm_pdf(plain, packed)


def xfa_bomb_pdf(refs=64, inner_mib=64, layers=3):
    """An XFA array naming one config packet <refs> times; the packet's last Flate layer is
    <inner_mib> MiB of empty stored blocks (decoding to nothing), under <layers> Flate filters."""
    import zlib

    data = b"\x78\x01" + b"\x00\x00\x00\xff\xff" * (inner_mib * (1 << 20) // 5) + b"\x03\x00" + b"\x00\x00\x00\x01"
    for _ in range(layers - 1):
        data = zlib.compress(data, 9)
    xs = stream("/Filter [%s]" % " ".join(["/FlateDecode"] * layers), data.decode("latin-1"))
    arr = "[" + " ".join("(config) {1} 0 R" for _ in range(refs)) + "]"
    return doc([{"content": words(10)}], extras=["<< /Fields [] /XFA {2} 0 R >>", xs, arr], catalog_extra="/AcroForm {0} 0 R")


def shared_names_pdf(nodes=1000, items=600000):
    """An embedded-files name tree whose leaf nodes (direct, in one indirect /Kids array) all name
    one large /Names array."""
    return doc(
        [{"content": words(5)}],
        extras=["<< /Kids {1} 0 R >>", "[" + "<< /Names {2} 0 R >>" * nodes + "]", "[" + "1 " * items + "]"],
        catalog_extra="/Names << /EmbeddedFiles {0} 0 R >>",
    )


def many_names_pdf(leaves=3, items=800000):
    """An embedded-files name tree with more than 10^6 entries, small on disk (object stream)."""
    plain = {1: b"<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles 4 0 R >> >>", 2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>", 3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>"}
    packed = {4: b"<< /Kids [" + b" ".join(b"%d 0 R" % (5 + i) for i in range(leaves)) + b"] >>"}
    for i in range(leaves):
        packed[5 + i] = b"<< /Names [" + b"1 " * items + b"] >>"
    return objstm_pdf(plain, packed)


def nested_prev_pdf(sections=999, pad=2000000):
    """A /Prev chain whose sections overlap: each section's first entry token is a string holding
    every deeper section, so following the chain naively lexes the file once per section."""
    buf = bytearray(b"%PDF-1.4\n")
    objs = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>", b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>"]
    for i, o in enumerate(objs, 1):
        buf += b"%d 0 obj\n" % i + o + b"\nendobj\n"
    base = len(buf)
    head = b"xref\n0 1\n("
    offs = [base + k * len(head) for k in range(sections + 1)]
    s = b"xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 4 /Root 1 0 R >>\n" + b"A" * pad
    for k in range(sections - 1, -1, -1):
        s = head + s + b") 0 n\ntrailer\n<< /Size 4 /Root 1 0 R /Prev %d >>\n" % offs[k + 1]
    buf += s + b"startxref\n%d\n%%%%EOF\n" % base
    return bytes(buf)


def clipped_text_pdf():
    """Text drawn outside the clip: alone, and over a full-page image (an OCR layer hidden by a
    clip instead of render mode 3)."""
    res = "<< /Font << /F1 3 0 R >> /XObject << /Im1 {0} 0 R >> >>"
    clip = "0 0 1 1 re W n\n"
    return doc(
        [
            {"content": "q " + clip + words(50) + "Q\n"},
            {"content": "q 612 0 0 792 0 0 cm /Im1 Do Q\nq " + clip + words(50) + "Q\n", "resources": res},
        ],
        extras=[image("/DCTDecode")],
    )


def image_budget_pdf(busy_pages=51, draws=1990):
    """Pages that together draw more images than pdftotext's run-wide budget (100000), then a
    scan page: its image facts must not depend on the pages before it."""
    res = "<< /Font << /F1 3 0 R >> /XObject << /Im1 {0} 0 R >> >>"
    busy = "q 10 0 0 10 100 100 cm /Im1 Do Q\n" * draws
    scan = "q 612 0 0 792 0 0 cm /Im1 Do Q\n" + words(30, invisible=True)
    return doc([{"content": busy, "resources": res}] * busy_pages + [{"content": scan, "resources": res}], extras=[image("/DCTDecode")])


def image_refusal_pdf(spenders=22000):
    """Page 1 draws <spenders> images whose colour spaces cost 3 fetches each (an indirect family
    name, an ICC profile and its /N by reference): past the 65,536-fetch budget of the image
    cache, then one more such image L (refused). Page 2 draws a /DeviceGray image (no lookup);
    page 3 draws L again. Only pages 1 and 3 are limited."""
    objs = []

    def add(body):
        objs.append(body if isinstance(body, bytes) else body.encode("latin-1"))
        return len(objs)

    # 1 Catalog, 2 Pages, 3 Helvetica, 4-9 pages and contents, then the rest
    for _ in range(9):
        add(b"null")
    gray = add(t.stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray", b"\x80"))
    xs = []
    for _ in range(spenders + 1):
        fam = add("/ICCBased")
        n = add("3")
        prof = add(t.stream("/N %d 0 R" % n, b"x"))
        xs.append(add(t.stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace [%d 0 R %d 0 R]" % (fam, prof), b"\x80\x80\x80")))
    res = lambda names: "<< /XObject << %s >> >>" % " ".join("/X%d %d 0 R" % (i, x) for i, x in names)
    contents = [
        ("".join("q 1 0 0 1 %d %d cm /X%d Do Q\n" % (i % 600, i // 600 % 780, i) for i in range(spenders + 1)), res(enumerate(xs))),
        ("q 100 0 0 100 100 100 cm /G Do Q\n", "<< /XObject << /G %d 0 R >> >>" % gray),
        ("q 100 0 0 100 100 100 cm /L Do Q\n", "<< /XObject << /L %d 0 R >> >>" % xs[-1]),
    ]
    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objs[1] = b"<< /Type /Pages /Kids [4 0 R 6 0 R 8 0 R] /Count 3 >>"
    objs[2] = HELV.encode("latin-1")
    for i, (c, r) in enumerate(contents):
        objs[3 + 2 * i] = ("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources %s /Contents %d 0 R >>" % (r, 5 + 2 * i)).encode("latin-1")
        objs[4 + 2 * i] = t.stream("", c.encode("latin-1"), compress=True)
    return t.build_pdf(objs)


def copy_class_pdf():
    """Images whose dictionaries must not be copied as they are."""
    res = "<< /XObject << %s >> >>" % " ".join("/I%d {%d} 0 R" % (i, i) for i in range(4))
    content = "".join("q 100 0 0 100 %d 400 cm /I%d Do Q\n" % (72 + 110 * i, i) for i in range(4))
    return doc(
        [{"content": content, "resources": res}],
        extras=[
            image("/DCTDecode", w=300, h=300, extra="/DecodeParms << /ColorTransform 0 >>"),
            image("/CCITTFaxDecode", w=300, h=300, cs="/DeviceGray", bpc=1, extra="/Decode [1 0]"),
            image(None, w=300, h=300, extra="/F (external.jpg) /FFilter /DCTDecode"),
            image(None, w=300, h=300, extra="/F /DCTDecode"),
        ],
    )


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


# --- issue 40: ruled grids, words inside images, image bytes per pixel ----------------------


def _hlines(x0, x1, ys):
    return "".join("%g %g m %g %g l S\n" % (x0, y, x1, y) for y in ys)


def _vlines(xs, y0, y1):
    return "".join("%g %g m %g %g l S\n" % (x, y0, x, y1) for x in xs)


def _cells(xs, ys, label):
    """A word in each cell of the grid with column lines <xs> and row lines <ys> (PDF space)."""
    out = []
    for r in range(len(ys) - 1):
        for c in range(len(xs) - 1):
            out.append("BT /F1 9 Tf %g %g Td (%s%d%d) Tj ET\n" % (xs[c] + 4, ys[r + 1] + 5, label, r, c))
    return "".join(out)


def grids_pdf():
    """Ruled tables. Page 1: two tables, the second with a header cell merged across its three
    columns (its column lines start below the header row). Page 2: two tables sharing a width,
    told apart by the jump in rule spacing. Page 3: 72 small grids (the list holds 64).
    Page 4: a table ruled only horizontally. Page 5: white rules only. Page 6: a table of
    shaded row boxes with column rules."""
    # page 1, table A: 4 x 4, PDF y 700..580; table B: header 450..430, body 430..370
    ax, ay = [72, 172, 272, 372, 472], [700, 670, 640, 610, 580]
    a = _hlines(72, 472, ay) + _vlines(ax, 580, 700) + _cells(ax, ay, "a")
    bx, by = [72, 202, 332, 462], [450, 430, 410, 390, 370]
    b = _hlines(72, 462, by) + _vlines([72, 462], 370, 450) + _vlines([202, 332], 370, 430)
    b += "BT /F1 9 Tf 200 437 Td (Merged header) Tj ET\n" + _cells(bx, by[1:], "b")
    # page 2: rules 20 apart, then a 70 point gap (under the 80 point row gap), then rules 20 apart
    ys2 = [700, 680, 660, 640, 570, 550, 530, 510]
    p2 = _hlines(72, 400, ys2) + _vlines([72, 236, 400], 640, 700) + _vlines([72, 236, 400], 510, 570) + words(4, y=300)
    # page 3: 9 rows x 8 columns of 3-rule grids, 40 points wide, 85 points apart vertically
    p3 = "".join(_hlines(20 + 72 * c, 60 + 72 * c, [770 - 85 * r, 765 - 85 * r, 760 - 85 * r]) for r in range(9) for c in range(8))
    # page 4: booktabs-style rules, words in columns
    p4 = _hlines(72, 472, [700, 680, 600]) + "".join("BT /F1 10 Tf %d %d Td (v%d) Tj ET\n" % (80 + 100 * (k % 4), 665 - 15 * (k // 4), k) for k in range(16))
    # page 5: a white grid (separators on a coloured page): not rules a table reads
    p5 = "1 1 1 RG\n" + _hlines(72, 472, [700, 670, 640, 610]) + _vlines([72, 272, 472], 610, 700) + words(4, y=300)
    # page 6: shaded row boxes (each a filled rectangle) with two column rules
    p6 = "0.9 g\n" + "".join("72 %d 400 18 re f\n" % (700 - 30 * k) for k in range(4)) + "0 g\n" + _vlines([180, 290], 610, 718) + words(4, y=300)
    return doc([{"content": "0.5 w\n" + a + b}, {"content": "0.5 w\n" + p2}, {"content": "0.5 w\n" + p3}, {"content": "0.5 w\n" + p4}, {"content": "0.5 w\n" + p5}, {"content": "0.5 w\n" + p6}])


def _noise(n, seed=7):
    """<n> deterministic bytes standing in for compressed image data (no '{', which doc() fills)."""
    out = bytearray()
    x = seed
    while len(out) < n:
        x = (x * 1103515245 + 12345) & 0x7FFFFFFF
        b = (x >> 16) & 0xFF
        out.append(b if b != 0x7B else 0x7C)
    return bytes(out)


def image_words_pdf():
    """Page 1: a 300 dpi JPEG scan under an invisible OCR layer of 300 words. Page 2: a photo
    with a 6-word caption drawn over it and 20 words below it. Page 3: a JPEG scan next to a
    flat chart (Flate). Page 4: an image whose /Length is an indirect object. Page 5: an inline
    image (no bytes per pixel)."""
    import zlib

    res = "<< /Font << /F1 3 0 R >> /XObject << /Scan {0} 0 R /Photo {1} 0 R /Jpeg {2} 0 R /Chart {3} 0 R /Ind {4} 0 R >> >>"
    scan = image("/DCTDecode", 2550, 3300, "/DeviceRGB", 8)
    photo = stream("/Type /XObject /Subtype /Image /Width 600 /Height 400 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode", _noise(600 * 400 // 4))
    jpeg = stream("/Type /XObject /Subtype /Image /Width 800 /Height 600 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode", _noise(800 * 600 * 3 // 8, seed=11))
    chart = stream("/Type /XObject /Subtype /Image /Width 800 /Height 600 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode", zlib.compress(b"\xff" * (800 * 600 * 3), 9))
    data = _noise(5000, seed=3)
    indirect = "<< /Type /XObject /Subtype /Image /Width 100 /Height 50 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode /Length {5} 0 R >>\nstream\n" + data.decode("latin-1") + "\nendstream"
    caption = "BT /F1 10 Tf 80 410 Td (Sunset over the harbour at dusk) Tj ET\n"
    return doc(
        [
            {"content": "q 612 0 0 792 0 0 cm /Scan Do Q\n" + words(300, invisible=True), "resources": res},
            {"content": "q 300 0 0 200 72 400 cm /Photo Do Q\n" + caption + words(20, y=300), "resources": res},
            {"content": "q 250 0 0 188 50 500 cm /Jpeg Do Q\nq 250 0 0 188 320 500 cm /Chart Do Q\n", "resources": res},
            {"content": "q 200 0 0 100 72 500 cm /Ind Do Q\n", "resources": res},
            {"content": "q 100 0 0 100 72 500 cm BI /W 2 /H 2 /CS /G /BPC 8 ID \x00\x80\x80\xff EI Q\n", "resources": res},
        ],
        extras=[scan, photo, jpeg, chart, indirect, str(len(data))],
    )


def _raw_image(length_entry, data, w=100, h=50):
    """An image XObject body with a /Length entry as given (or none) over <data>."""
    return "<< /Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode %s >>\nstream\n" % (w, h, length_entry) + data.decode("latin-1") + "\nendstream"


def lengths_pdf(break_xref=False):
    """One image per page, 5000 bytes each, under /Length: 5000; 4995 (short); missing; 0;
    10^12; 5005 (past endstream); 5000 by reference. With <break_xref>, a first page draws a
    form XObject whose cross-reference offset is wrong, so a whole run rebuilds the table before
    the other pages are read and a single-page run does not."""
    data = _noise(5000, seed=5)
    entries = ["/Length 5000", "/Length 4995", "", "/Length 0", "/Length 1000000000000", "/Length 5005", "/Length {7} 0 R"]
    names = " ".join("/I%d {%d} 0 R" % (k, k) for k in range(len(entries)))
    res = "<< /XObject << %s /Bad {8} 0 R >> >>" % names
    pages = [{"content": "q 200 0 0 100 72 500 cm /I%d Do Q\n" % k, "resources": res} for k in range(len(entries))]
    if break_xref:
        pages.insert(0, {"content": "/Bad Do\n", "resources": res})
    extras = [_raw_image(e, data) for e in entries] + ["5000", stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "0 0 5 5 re f")]
    pdf = doc(pages, extras=extras)
    if break_xref:
        # point the form's xref entry at the wrong place (the start of the file's header)
        num = 4 + 2 * len(pages) + 8
        xref = pdf.rindex(b"xref\n0 ")
        at = xref + len(b"xref\n0 %d\n" % (int(pdf[xref + 7 : pdf.index(b"\n", xref + 5)]),)) + 20 * num
        pdf = pdf[:at] + b"%010d" % 3 + pdf[at + 10 :]
    return pdf


def grid_caps_pdf():
    """Page 1: a word of size 5000 (outside the body-size bins) and a ruled table. Page 2: 256
    one-letter words of distinct large sizes, then 200 words of 10 points and the table: the
    body size is still 10. Page 3: 4,100 horizontal rules (the grid search takes 4,096). Page 4:
    20,005 tiny fills (the page's rule cap), then the table."""
    table = "0.5 w\n" + _hlines(72, 472, [700, 670, 640, 610]) + _vlines([72, 272, 472], 610, 700)
    big = "BT /F1 5000 Tf 0 0 Td (x) Tj ET\n"
    decoys = "".join("BT /F1 %g Tf 10 10 Td 3 Tr (x) Tj ET\n" % (100 + 0.5 * k) for k in range(256))
    many = "0.5 w\n" + "".join("72 %.2f m 472 %.2f l S\n" % (780 - 0.18 * k, 780 - 0.18 * k) for k in range(4100))
    tiny = "".join("%d %d 1 0.2 re f\n" % (k % 600, 5 + (k // 600) % 30) for k in range(20005))
    return doc([{"content": big + words(20, y=300) + table}, {"content": decoys + words(200, y=500) + table}, {"content": many}, {"content": tiny + table}])


# --- issue 43: thin rules per grid, words centred inside images -----------------------------


def grid_rules_pdf():
    """Page 1: a 3 x 4 table ruled cell by cell (each cell a stroked rectangle). Page 2: a zebra
    table of shaded row boxes and no rules. Page 3: shaded rows with two column lines (no outer
    sides). Page 4: a table in a 4-point frame with thin inner rules (the frame is too thick to
    count). Page 5: a fully ruled table with a stray underline in one cell and a white rule.
    Page 6: a ruled table, then 20,005 tiny fills (the page's rule cap): no rule counts."""
    xs, ys = [72, 172, 272, 372, 472], [700, 670, 640, 610]
    p1 = "".join("%g %g %g %g re S\n" % (xs[c], ys[r + 1], xs[c + 1] - xs[c], ys[r] - ys[r + 1]) for r in range(3) for c in range(4)) + _cells(xs, ys, "c")
    zebra = "0.85 g\n" + "".join("72 %d 400 18 re f\n" % (700 - 30 * k) for k in range(5)) + "0 g\n"
    p2 = zebra + words(4, y=300)
    p3 = zebra + "0.5 w\n" + _vlines([205, 338], 580, 718) + words(4, y=300)
    frame = "4 w\n72 580 400 120 re S\n0.5 w\n" + _hlines(72, 472, [670, 640, 610]) + _vlines([172, 272, 372], 580, 700)
    p4 = frame + _cells(xs, [700, 670, 640, 610, 580], "f")
    p5 = "0.5 w\n" + _hlines(72, 472, ys) + _vlines(xs, 610, 700) + _cells(xs, ys, "u") + "0.6 w\n80 676 m 140 676 l S\n1 1 1 RG\n72 655 m 472 655 l S\n"
    p6 = "0.5 w\n" + _hlines(72, 472, ys) + _vlines(xs, 610, 700) + "".join("%d %d 1 0.2 re f\n" % (k % 600, 5 + (k // 600) % 30) for k in range(20005))
    return doc([{"content": "0.5 w\n" + p1}, {"content": p2}, {"content": p3}, {"content": p4}, {"content": p5}, {"content": p6}])


def grid_split_pdf():
    """Ruled tables whose connected thin rules the grid search does not see as one box (council,
    issue 43). Page 1: column lines through two blocks of rules told apart by a jump in spacing
    (one walker grid, two grid-search grids). Page 2: a 118-point row between two ruled blocks,
    column lines through all of it. Page 3: the left column line rises to a 20-point rule above
    the table. Page 4: the column lines drop to a 10-point rule below the last row. Page 5: a
    table ruled only horizontally with one short vertical tick that touches nothing. Page 6:
    5,001 short dashes, then a ruled table (the layout walker reads 5,000 rules a page and never
    sees the table). Page 7: 4,990 dashes, then the same table (the walker sees it)."""
    ys1 = [700, 680, 660, 640, 570, 550, 530, 510]
    p1 = _hlines(72, 400, ys1) + _vlines([72, 236, 400], 510, 700)
    ys2 = [700, 680, 660, 542, 522, 502]
    p2 = _hlines(72, 400, ys2) + _vlines([72, 236, 400], 502, 700)
    ys3 = [700, 680, 660, 640]
    p3 = _hlines(72, 400, ys3) + _vlines([72, 236, 400], 640, 700) + _vlines([72], 700, 730) + _hlines(62, 82, [730])
    p4 = _hlines(72, 400, ys3) + _vlines([72, 236, 400], 600, 700) + _hlines(230, 242, [600])
    p5 = _hlines(72, 400, [700, 680, 660, 640, 620]) + _vlines([120], 645, 655)
    def dashes(n):
        return "".join("%d %d m %d %d l S\n" % (20 + 5 * (k % 110), 100 + 2 * (k // 110), 22 + 5 * (k % 110), 100 + 2 * (k // 110)) for k in range(n))

    table = _hlines(72, 472, [700, 670, 640, 610]) + _vlines([72, 172, 272, 372, 472], 610, 700)
    return doc([{"content": "0.5 w\n" + c + words(4, y=300)} for c in (p1, p2, p3, p4, p5, dashes(5001) + table, dashes(4990) + table)])


def grid_rebuilt_pdf():
    """A ruled table in a file whose catalog names a missing /Outlines object: the doc line reads
    it (pdftotext never does) and, since issue 45, read-only: the refused rebuild leaves the
    table the pages are drawn from alone (doc xref repaired)."""
    table = "0.5 w\n" + _hlines(72, 472, [700, 670, 640, 610]) + _vlines([72, 172, 272, 372, 472], 610, 700)
    return doc([{"content": table}], catalog_extra="/Outlines 99 0 R")


def grid_reopened_pdf():
    """A ruled table on 2 pages of a file whose startxref is wrong: poppler rebuilds the
    cross-reference table at open, as every run of it does."""
    table = "0.5 w\n" + _hlines(72, 472, [700, 670, 640, 610]) + _vlines([72, 172, 272, 372, 472], 610, 700)
    pdf = doc([{"content": table}, {"content": table}])
    at = pdf.rindex(b"startxref\n") + len(b"startxref\n")
    end = pdf.index(b"\n", at)
    return pdf[:at] + b"%d" % (int(pdf[at:end]) + 7) + pdf[end:]


def grid_titled_pdf():
    """A ruled table in a file whose document information /Title names a missing object: only
    pdftotext's XHTML header reads it, never the page facts; since issue 45 that read cannot
    rebuild the table (XRef::RepairGuard)."""
    table = "0.5 w\n" + _hlines(72, 472, [700, 670, 640, 610]) + _vlines([72, 172, 272, 372, 472], 610, 700)
    return doc([{"content": table}], extras=["<< /Title 99 0 R >>"], trailer_extra="/Info {0} 0 R")


def grid_far_pdf():
    """A ruled table at the foot of a 2 x 10^9-point page (displayed y near 2 x 10^9): past 10^9
    points, no counts."""
    y = 400
    table = "0.5 w\n" + "".join("72 %d m 472 %d l S\n" % (v, v) for v in (y, y - 30, y - 60)) + "".join("%d %d m %d %d l S\n" % (x, y - 60, x, y) for x in (72, 272, 472))
    return doc([{"content": table, "mediabox": (0, 0, 612, 2000000000)}])


def caption_straddle_pdf():
    """A photo (PDF y 400..600) with a caption line whose words straddle its bottom edge with
    their centres inside, a line straddling it with centres outside, and a line well below."""
    res = "<< /Font << /F1 3 0 R >> /XObject << /Photo {0} 0 R >> >>"
    photo = stream("/Type /XObject /Subtype /Image /Width 600 /Height 400 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode", _noise(600 * 400 // 4))
    # 4 words on a baseline at the photo's edge: boxes cross it, centres inside
    text = "BT /F1 10 Tf 80 400 Td (Harbour at golden hour) Tj ET\n"
    # 4 words 5 points lower: boxes cross the edge, centres below it
    text += "BT /F1 10 Tf 200 395 Td (Photo by the crew) Tj ET\n"
    text += words(10, y=300)
    return doc([{"content": "q 300 0 0 200 72 400 cm /Photo Do Q\n" + text, "resources": res}], extras=[photo])


def _chain_pdf(newest, older_rows, objs_new, objs_old, trailer_new, trailer_old):
    """Body: objs_new then objs_old (dicts of number -> body bytes). An older classic section at
    the end lists older_rows (numbers, from objs_old first, else objs_new); then the newest
    section: newest = ("stream", numbers, size) for an xref stream, or ("table", numbers) for a
    classic table, whose /Prev is the older one."""
    import struct
    out = bytearray(b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n")
    off_new, off_old = {}, {}
    for n in sorted(objs_new):
        off_new[n] = len(out)
        out += b"%d 0 obj\n" % n + objs_new[n] + b"\nendobj\n"
    for n in sorted(objs_old):
        off_old[n] = len(out)
        out += b"%d 0 obj\n" % n + objs_old[n] + b"\nendobj\n"
    old_at = len(out)
    top = max(older_rows) + 1
    out += b"xref\n0 %d\n0000000000 65535 f \n" % top
    for n in range(1, top):
        o = off_old.get(n, off_new.get(n)) if n in older_rows else None
        out += b"%010d 00000 n \n" % o if o is not None else b"0000000000 00000 f \n"
    out += b"trailer\n<< /Size %d %s >>\n" % (top, trailer_old)
    new_at = len(out)
    if newest[0] == "stream":
        _, nums, size = newest
        rows = b"\x00" + struct.pack(">I", 0) + b"\xff\xff" + b"".join(b"\x01" + struct.pack(">I", off_new[n]) + b"\x00\x00" for n in nums)
        out += b"30 0 obj\n<< /Type /XRef /Size %d /Index [0 %d] /W [1 4 2] %s /Prev %d /Length %d >>\nstream\n" % (size, len(nums) + 1, trailer_new, old_at, len(rows)) + rows + b"\nendstream\nendobj\n"
    else:
        _, nums, size = newest
        out += b"xref\n0 1\n0000000000 65535 f \n" + b"".join(b"%d 1\n%010d 00000 n \n" % (n, off_new[n]) for n in nums)
        out += b"trailer\n<< /Size %d %s /Prev %d >>\n" % (size, trailer_new, old_at)
    out += b"startxref\n%d\n%%%%EOF\n" % new_at
    return bytes(out)


def _grid_form():
    form = ("0 0 0 RG 0.5 w\n" + _hlines(72, 472, [700, 670, 640, 610]) + _vlines([72, 172, 272, 372, 472], 610, 700)).encode()
    return b"<< /Type /XObject /Subtype /Form /BBox [0 0 612 792] /Length %d >>\nstream\n" % len(form) + form + b"\nendstream"


def prev_redefined_pdf():
    """Issue 45. An incremental update whose newest section (an xref stream, /Size 6) lists
    objects 0-4; the older classic section lists 1-8, with an older body of the page's content
    stream (4) and the page's ruled form (6), font (8) and outlines (5) that only it lists. The
    newest section wins for 4; 6 and 8 resolve through the older section in every run, not only
    after something (the doc line's /Outlines read) has made poppler read it. Upstream's run
    without facts drew nothing on the page."""
    page_content = b"q /X Do Q BT /F1 12 Tf 72 740 Td (Newest content) Tj ET"
    old_content = b"BT /F1 12 Tf 72 740 Td (Older content) Tj ET"
    objs_new = {
        1: b"<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /XObject << /X 6 0 R >> /Font << /F1 8 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(page_content) + page_content + b"\nendstream",
    }
    objs_old = {
        4: b"<< /Length %d >>\nstream\n" % len(old_content) + old_content + b"\nendstream",
        5: b"<< /Type /Outlines /Count 0 >>",
        6: _grid_form(),
        7: b"<< /Producer (older) >>",
        8: HELV.encode(),
    }
    return _chain_pdf(("stream", [1, 2, 3, 4], 6), set(range(1, 9)), objs_new, objs_old, b"/Root 1 0 R", b"/Root 1 0 R")


def prev_unlisted_pdf():
    """Issue 45. Classic tables: the newest section lists 0-4 (/Size 10), the older one 1-8; the
    page's ruled form (9) is in the file but in no section. The first fetch of 9 rebuilds the
    table in every run, flagged as a rebuild after open. (Upstream flagged it too, through
    fetch's own rebuild; this guards that the eager chain read keeps it.)"""
    page_content = b"q /X Do Q BT /F1 12 Tf 72 740 Td (Unlisted form) Tj ET"
    objs_new = {
        1: b"<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /XObject << /X 9 0 R >> /Font << /F1 8 0 R >> >> >>",
        4: b"<< /Length %d >>\nstream\n" % len(page_content) + page_content + b"\nendstream",
        9: _grid_form(),
    }
    objs_old = {5: b"<< /Type /Outlines /Count 0 >>", 6: b"<< >>", 7: b"<< >>", 8: HELV.encode()}
    return _chain_pdf(("table", [1, 2, 3, 4], 10), set(range(1, 9)), objs_new, objs_old, b"/Root 1 0 R", b"/Root 1 0 R")


def _content(text, extra=b""):
    c = extra + b"BT /F1 12 Tf 72 700 Td (" + text + b") Tj ET"
    return b"<< /Length %d >>\nstream\n" % len(c) + c + b"\nendstream"


def _classic(objs, listed, size, trailer=b"", before_xref=b"", prev=None, tail_objs=()):
    """A classic-table PDF: objs (number -> body), listed numbers in the table (with /Size size),
    raw bytes before the table, and tail_objs ((number, body), ...) written after every listed
    body but listed nowhere (a rebuild takes the last body of a number)."""
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    off = {}
    for n in sorted(objs):
        off[n] = len(out)
        out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
    for n, body in tail_objs:
        out += b"%d 0 obj\n" % n + body + b"\nendobj\n"
    out += before_xref
    xat = len(out)
    out += b"xref\n0 1\n0000000000 65535 f \n" + b"".join(b"%d 1\n%010d 00000 n \n" % (n, off[n]) for n in sorted(listed))
    out += b"trailer\n<< /Size %d /Root 1 0 R%s%s >>\nstartxref\n%d\n%%%%EOF\n" % (size, trailer, b" /Prev %d" % prev if prev is not None else b"", xat)
    return bytes(out)


def prev_broken_pdf():
    """Issue 45. The newest section lists everything the page draws; the catalog's /Outlines (5)
    is listed only by an older section, which is garbage. Object 4 (the page's content) has a
    second, unlisted body later in the file ("Bravo"). Upstream read the older section only when
    the doc line asked for /Outlines, rebuilt there, and the page then drew Bravo with facts and
    Alpha without. Now the chain ends at the broken section in every run, the doc line's guarded
    read of /Outlines is refused (doc xref repaired), and every run draws Alpha."""
    objs = {
        1: b"<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 6 0 R >> >> >>",
        4: _content(b"Alpha"),
        5: b"<< /Type /Outlines /Count 0 >>",
        6: HELV.encode(),
    }
    probe = _classic(objs, [1, 2, 3, 4, 6], 7, tail_objs=[(4, _content(b"Bravo"))], before_xref=b"xre? 0 9\ngarbage\n")
    old_at = probe.index(b"xre? ")
    return _classic(objs, [1, 2, 3, 4, 6], 7, tail_objs=[(4, _content(b"Bravo"))], before_xref=b"xre? 0 9\ngarbage\n", prev=old_at)


def facts_only_read_pdf():
    """Issue 45 (council). One classic section; page 1's image names /ColorSpace 7, which no
    section lists, and page 2's content (11) has a second, unlisted body ("Bravo"). Drawing never
    resolves the colour space without -images; page facts do, under a RepairGuard, so they cannot
    rebuild the table and change what page 2 draws."""
    img = b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace 7 0 R /Length 12 >>\nstream\n" + b"\x00" * 12 + b"\nendstream"
    objs = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R 10 0 R] /Count 2 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im 6 0 R >> >> >>",
        4: _content(b"Page one", b"q 100 0 0 100 72 400 cm /Im Do Q "),
        5: HELV.encode(),
        6: img,
        7: b"/DeviceRGB",
        10: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 11 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        11: _content(b"Alpha"),
    }
    return _classic(objs, [1, 2, 3, 4, 5, 6, 10, 11], 12, tail_objs=[(11, _content(b"Bravo"))])


ALL = {
    "geometry": geometry_pdf,
    "scans": scans_pdf,
    "scan_thresholds": scan_thresholds_pdf,
    "fonts": fonts_pdf,
    "images": images_pdf,
    "vectors": vectors_pdf,
    "bars": bars_pdf,
    "bars_curve": curve_pdf,
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
    "xref_stream": xref_stream_pdf,
    "xref_stream_update": lambda: xref_stream_pdf("stream"),
    "table_then_stream": lambda: xref_stream_pdf("table-then-stream"),
    "clipped_text": clipped_text_pdf,
    "copy_class": copy_class_pdf,
    "grids": grids_pdf,
    "image_words": image_words_pdf,
    "lengths": lengths_pdf,
    "lengths_rebuilt": lambda: lengths_pdf(break_xref=True),
    "grid_caps": grid_caps_pdf,
    "grid_rules": grid_rules_pdf,
    "caption_straddle": caption_straddle_pdf,
    "grid_split": grid_split_pdf,
    "grid_rebuilt": grid_rebuilt_pdf,
    "grid_far": grid_far_pdf,
    "grid_reopened": grid_reopened_pdf,
    "grid_titled": grid_titled_pdf,
    "prev_redefined": prev_redefined_pdf,
    "prev_unlisted": prev_unlisted_pdf,
    "prev_broken": prev_broken_pdf,
    "facts_only_read": facts_only_read_pdf,
}
