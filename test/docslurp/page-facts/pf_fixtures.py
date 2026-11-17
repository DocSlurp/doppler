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


# --- mathtables: math and table encodings -------------------------------------------


def _row(y, cells, font="F1", size=10):
    return "".join("BT /%s %d Tf %d %d Td (%s) Tj ET\n" % (font, size, x, y, t) for x, t in cells)


def _prose(y, lines=3):
    return "".join("BT /F1 10 Tf 72 %d Td (The quarterly report shows revenue rising across every region again.) Tj ET\n" % (y - 12 * k) for k in range(lines))


def math_tables_pdf():
    """One encoding a page (mathtables family):
    1 booktabs table (heavier top and bottom rules), a heading rule and a footnote rule;
    2 an open table (equal rules between every row);
    3 lined paper and form lines (no table);
    4 a whitespace-aligned table (no rules) under prose;
    5 shaded rows without rules;
    6 a raster formula strip in the text flow, a photo, a lossless low-bytes table-like image;
    7 a Type 3 font whose glyph procedures paint inline image masks;
    8 tagged structure: Formula (TeX alt, MathML file), role-mapped Table/TR/TD with ColSpan 2,
      MathML-namespace elements, marked-content ActualText holding TeX;
    9 math-font classes, plain Unicode math in Helvetica, a fraction and a 2x2 matrix in CMMI10;
    10 a glyph-outline formula (a small curve-dense path) in the text flow."""
    cols = (80, 200, 320)
    booktabs = (
        _prose(740)
        + "BT /F1 12 Tf 72 690 Td (Results) Tj ET\n0.5 w 72 684 m 300 684 l S\n"
        + "0.8 w 72 650 m 400 650 l S\n"
        + _row(638, zip(cols, ("Method", "Score", "Time")))
        + "0.5 w 72 630 m 400 630 l S\n"
        + "".join(_row(618 - 14 * k, zip(cols, (name, "91.%d" % k, "1%d" % k))) for k, name in enumerate(("Alpha", "Bravo", "Charlie")))
        + "0.8 w 72 572 m 400 572 l S\n"
        + "0.4 w 72 120 m 160 120 l S\nBT /F1 8 Tf 72 110 Td (1 Footnote text under a short rule.) Tj ET\n"
    )
    open_table = _prose(740) + "".join("0.5 w 72 %d m 400 %d l S\n" % (y, y) for y in (660, 646, 632, 618, 604)) + "".join(_row(650 - 14 * k, zip(cols, ("Row%d" % k, "%d.5" % k, "x%d" % k))) for k in range(4))
    lined = "".join("0.5 w 72 %d m 540 %d l S\n" % (y, y) for y in range(700, 412, -24))
    lined += "".join("BT /F1 10 Tf 76 %d Td (Notes written on this line today about the plan) Tj ET\n" % (y + 4) for y in range(700, 412, -48))
    lined += "".join("BT /F1 10 Tf 72 %d Td (Name) Tj ET 0.5 w 120 %d m 300 %d l S BT /F1 10 Tf 320 %d Td (Date) Tj ET 360 %d m 500 %d l S\n" % (y, y, y, y, y, y) for y in (300, 276, 252))
    whitespace = _prose(740) + "".join(_row(660 - 14 * k, zip(cols, (name, "%d.%d" % (k, k), "Yes" if k % 2 else "No"))) for k, name in enumerate(("Lisbon", "Oslo", "Vienna", "Dublin", "Prague")))
    shaded = _prose(740) + "".join("0.85 g 72 %d 400 14 re f 0 g\n" % (y - 3) + _row(y, zip(cols, ("Item%d" % k, "%d" % (10 * k), "ok"))) for k, y in enumerate((660, 632, 604, 576)))
    raster = (
        _prose(700, 2)
        + "q 200 0 0 30 150 640 cm /Im1 Do Q\n"
        + _prose(628, 2)
        + "q 150 0 0 150 72 380 cm /Im2 Do Q\n"
        + _prose(370, 2)
        + "q 300 0 0 200 72 120 cm /Im3 Do Q\n"
        + _prose(110, 1)
    )
    type3 = "BT /T3 12 Tf 72 700 Td (abab abba baba) Tj ET\n" + _prose(680)
    tagged = (
        "/Formula << /MCID 0 >> BDC BT /F1 12 Tf 72 700 Td (a/b) Tj ET EMC\n"
        "/TD << /MCID 1 >> BDC BT /F1 12 Tf 72 680 Td (cell) Tj ET EMC\n"
        "/Span << /MCID 2 >> BDC BT /F1 12 Tf 72 660 Td (x) Tj ET EMC\n"
        "/Span << /ActualText (x^{n}) >> BDC BT /F1 12 Tf 72 640 Td (xn) Tj ET EMC\n"
    )
    fonts = (
        _prose(740)
        + _row(700, ((72, "a"),), "M1")
        + _row(700, ((100, "x"),), "M2")
        + _row(700, ((130, "x"),), "M3")
        + _row(700, ((160, "a"),), "M4")
        + _row(700, ((190, "x"),), "M5")
        + "BT /U1 10 Tf 72 660 Td <010203040506> Tj ET\n"
        # a fraction: a over a bar over b
        + _row(620, ((300, "a"),), "M1")
        + "0.5 w 299 616 m 309 616 l S\n"
        + _row(606, ((301, "b"),), "M1")
        # a 2x2 matrix between CMEX10 parentheses centred on its rows
        + _row(560, ((300, "a"), (315, "b")), "M1")
        + _row(548, ((300, "c"), (315, "d")), "M1")
        + _row(551, ((290, "\\("), (326, "\\)")), "M7")
    )
    outline = _prose(700, 2) + "q 300 676 m 302 684 304 684 306 676 c 308 668 310 668 312 676 c 314 684 316 684 318 676 c 320 668 322 668 324 676 c 326 684 328 684 330 676 c 332 668 334 668 336 676 c 338 684 340 684 342 676 c 344 668 346 668 348 676 c f Q\n" + _prose(664, 2)
    mask_proc = "600 0 0 0 500 500 d1 q 500 0 0 500 0 0 cm BI /IM true /W 8 /H 8 /BPC 1 ID \x00\x18\x3c\x7e\x7e\x3c\x18\x00 EI Q"
    t3 = (
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a {0} 0 R /b {1} 0 R >>"
        " /Encoding << /Type /Encoding /Differences [97 /a /b] >> /FirstChar 97 /LastChar 98 /Widths [600 600] /Resources << >> >>"
    )
    # a TrueType program that is only a table directory with a MATH entry (never drawn: page facts read the directory)
    sfnt = b"\x00\x01\x00\x00\x00\x02\x00\x20\x00\x01\x00\x00" + b"MATH" + b"\x00" * 12 + b"cmap" + b"\x00" * 12
    extras = [
        stream("", mask_proc),  # 0
        stream("", mask_proc.replace("a", "a")),  # 1
        t3,  # 2
        image(None, 800, 120, "/DeviceGray", 8),  # 3 strip
        image("/DCTDecode", 1200, 1200),  # 4 photo
        stream("/Type /XObject /Subtype /Image /Width 1200 /Height 800 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /FlateDecode", b"x" * 400),  # 5 table-like
        "<< /Type /StructTreeRoot /K {7} 0 R /ParentTree {16} 0 R /RoleMap << /MyTable /Table >> >>",  # 6
        "<< /Type /StructElem /S /Document /P {6} 0 R /K [{8} 0 R {9} 0 R {14} 0 R] >>",  # 7
        "<< /Type /StructElem /S /Formula /P {7} 0 R /Pg {P7} 0 R /K 0 /Alt (\\\\frac{a}{b}) /AF [{17} 0 R] >>",  # 8
        "<< /Type /StructElem /S /MyTable /P {7} 0 R /K {10} 0 R >>",  # 9
        "<< /Type /StructElem /S /TR /P {9} 0 R /K {11} 0 R >>",  # 10
        "<< /Type /StructElem /S /TD /P {10} 0 R /Pg {P7} 0 R /K 1 /A << /O /Table /ColSpan 2 >> >>",  # 11
        "<< /Type /Namespace /NS (http://www.w3.org/1998/Math/MathML) >>",  # 12
        "<< /Type /StructElem /S /mi /NS {12} 0 R /P {14} 0 R /Pg {P7} 0 R /K 2 >>",  # 13
        "<< /Type /StructElem /S /math /NS {12} 0 R /P {7} 0 R /K {13} 0 R >>",  # 14
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding << /Type /Encoding /Differences [1 /alpha /summation /twosuperior /minus /arrowright /integral] >> >>",  # 15
        "<< /Nums [0 [{8} 0 R {11} 0 R {13} 0 R]] >>",  # 16
        "<< /Type /Filespec /F (f.mml) /UF (f.mml) /EF << /F {18} 0 R >> /AFRelationship /Supplement >>",  # 17
        stream("/Type /EmbeddedFile /Subtype /application#2Fmathml+xml", "<math/>"),  # 18
        "<< /Type /Font /Subtype /Type1 /BaseFont /CMMI10 >>",  # 19
        "<< /Type /Font /Subtype /Type1 /BaseFont /KaTeX_Math-Italic >>",  # 20
        "<< /Type /Font /Subtype /TrueType /BaseFont /CambriaMath >>",  # 21
        "<< /Type /Font /Subtype /Type1 /BaseFont /Symbol >>",  # 22
        "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+PlainFace /FirstChar 32 /LastChar 126 /Widths [%s] /FontDescriptor {24} 0 R >>" % " ".join(["500"] * 95),  # 23
        "<< /Type /FontDescriptor /FontName /ABCDEF+PlainFace /Flags 32 /FontBBox [0 0 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 {25} 0 R >>",  # 24
        t.stream("", sfnt).decode("latin-1"),  # 25
        "<< /Type /Font /Subtype /Type1 /BaseFont /STIXTwoMath-Regular >>",  # 26
        "<< /Type /Font /Subtype /Type1 /BaseFont /CMEX10 >>",  # 27
    ]
    font_res = "<< /Font << /F1 3 0 R /M1 {19} 0 R /M2 {20} 0 R /M3 {21} 0 R /M4 {22} 0 R /M5 {26} 0 R /M6 {23} 0 R /U1 {15} 0 R /M7 {27} 0 R >> >>"
    fonts += _row(700, ((220, "x"),), "M6")
    return doc(
        [
            {"content": booktabs},
            {"content": open_table},
            {"content": lined},
            {"content": whitespace},
            {"content": shaded},
            {"content": raster, "resources": "<< /Font << /F1 3 0 R >> /XObject << /Im1 {3} 0 R /Im2 {4} 0 R /Im3 {5} 0 R >> >>"},
            {"content": type3, "resources": "<< /Font << /F1 3 0 R /T3 {2} 0 R >> >>"},
            {"content": tagged, "extra": "/StructParents 0"},
            {"content": fonts, "resources": font_res},
            {"content": outline},
        ],
        extras=extras,
        catalog_extra="/StructTreeRoot {6} 0 R /MarkInfo << /Marked true >>",
    )


def math_tables_hostile_pdf():
    """Hostile inputs for the mathtables reads, each bounded: a parent tree whose Kids name
    themselves and an element whose /P chain is a cycle (page 1); a flat parent tree of 200,000
    pairs (page 2, key 199,999); a Type 3 font with 20,000 glyph procedures, an embedded program
    whose table directory claims 65,535 tables, and 3,000 thin rules with words (page 3); 4,000
    math-font glyphs stacked in one column, every pair a stacked candidate (page 4: the analysis
    budget runs out, analysis-truncated)."""
    nums = " ".join("%d [{3} 0 R]" % k for k in range(200000))
    procs = " ".join("/g%d {6} 0 R" % k for k in range(20000))
    rules = "".join("0.5 w 72 %d m 540 %d l S\n" % (100 + k % 600, 100 + k % 600) for k in range(3000))
    sfnt = b"\x00\x01\x00\x00\xff\xff\x00\x20\x00\x01\x00\x00" + b"cmap" + b"\x00" * 12
    extras = [
        "<< /Type /StructTreeRoot /K {2} 0 R /ParentTree {1} 0 R >>",  # 0
        "<< /Kids [{1} 0 R {4} 0 R {5} 0 R] /Limits [0 199999] >>",  # 1: names itself
        "<< /Type /StructElem /S /Formula /P {3} 0 R /K 0 >>",  # 2
        "<< /Type /StructElem /S /Table /P {2} 0 R /K 0 >>",  # 3: P cycle with 2
        "<< /Kids [{1} 0 R] /Limits [0 5] >>",  # 4: back to 1
        "<< /Limits [0 199999] /Nums [%s] >>" % nums,  # 5
        stream("", "600 0 0 0 500 500 d1 0 0 500 500 re f"),  # 6
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << %s >> /Encoding << /Type /Encoding /Differences [97 /g0 /g1] >> /FirstChar 97 /LastChar 98 /Widths [600 600] /Resources << >> >>" % procs,  # 7
        "<< /Type /Font /Subtype /TrueType /BaseFont /HostileMath /FirstChar 32 /LastChar 126 /Widths [%s] /FontDescriptor {9} 0 R >>" % " ".join(["500"] * 95),  # 8
        "<< /Type /FontDescriptor /FontName /HostileMath /Flags 32 /FontBBox [0 0 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 {10} 0 R >>",  # 9
        t.stream("", sfnt).decode("latin-1"),  # 10
        "<< /Type /Font /Subtype /Type1 /BaseFont /CMMI10 >>",  # 11
    ]
    column = "".join("BT /M1 2 Tf 300 %.1f Td (a) Tj ET\n" % (60 + 0.17 * k) for k in range(4000))
    return doc(
        [
            {"content": "/P << /MCID 0 >> BDC " + words(10) + " EMC\n", "extra": "/StructParents 0"},
            {"content": words(10), "extra": "/StructParents 199999"},
            {"content": rules + words(400, 700) + "BT /T3 10 Tf 72 60 Td (abab) Tj ET BT /H1 10 Tf 72 40 Td (xyz) Tj ET\n", "resources": "<< /Font << /F1 3 0 R /T3 {7} 0 R /H1 {8} 0 R >> >>"},
            {"content": column, "resources": "<< /Font << /F1 3 0 R /M1 {11} 0 R >> >>"},
        ],
        extras=extras,
        catalog_extra="/StructTreeRoot {0} 0 R",
    )


def _empty_deflate(n_blocks):
    """A zlib stream of empty stored blocks: decodes to nothing, costs a full inflate."""
    return b"\x78\x01" + b"\x00\x00\x00\xff\xff" * n_blocks + b"\x01\x00\x00\xff\xff" + b"\x00\x00\x00\x01"


def math_tables_council_pdf():
    """Council negative cases for the mathtables reads (each must stay bounded):
    1 50,000 Formula elements sharing one 1 MB Alt string and one /AF array whose file is an
      untyped stream of empty stored deflate blocks (2 MB encoded, decodes to nothing);
    2 3,000 Tf of an inline (direct) Type 3 font whose glyph procedures are such streams, a form
      with its own inline Type 3 font drawn 2,000 times, and a TrueType program that is one.
    Objects are numbered directly (doc()'s placeholders would be quadratic in 50,000 objects)."""
    big = _empty_deflate(400000)
    elems = 50000
    # 1 catalog, 2 pages, 3 Helvetica, 4/5 page 1, 6/7 page 2, then:
    root, docel, ptree, leaves, alt, afarr, spec, efile, empty, form = range(8, 18)
    first = 18
    t3 = "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a %d 0 R /b %d 0 R >> /Encoding << /Type /Encoding /Differences [97 /a /b] >> /FirstChar 97 /LastChar 98 /Widths [600 600] /Resources << >> >>" % (empty, empty)
    tt = "<< /Type /Font /Subtype /TrueType /BaseFont /EmptyBlocks /FirstChar 32 /LastChar 126 /Widths [%s] /FontDescriptor << /Type /FontDescriptor /FontName /EmptyBlocks /Flags 32 /FontBBox [0 0 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 %d 0 R >> >>" % (" ".join(["500"] * 95), empty)
    page2 = "".join("BT /T3 10 Tf %d %d Td (ab) Tj ET\n" % (72 + (k % 40) * 12, 700 - (k // 40) * 8) for k in range(3000))
    page2 += "q /Fm Do Q\n" * 2000 + "BT /TT 10 Tf 72 40 Td (xyz) Tj ET\n"
    page1 = "/P << /MCID 0 >> BDC " + words(10) + " EMC\n"
    objs = [
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot %d 0 R >>" % root,
        "<< /Type /Pages /Kids [4 0 R 6 0 R] /Count 2 >>",
        HELV,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R /StructParents 0 >>",
        t.stream("", page1),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R /T3 %s /TT %s >> /XObject << /Fm %d 0 R >> >> /Contents 7 0 R >>" % (t3, tt, form),
        t.stream("", page2),
        "<< /Type /StructTreeRoot /K %d 0 R /ParentTree %d 0 R >>" % (docel, ptree),
        "<< /Type /StructElem /S /Document /P %d 0 R >>" % root,
        "<< /Nums [0 %d 0 R] >>" % leaves,
        "[%s]" % " ".join("%d 0 R" % (first + k) for k in range(elems)),
        "(\\\\frac{a}{b} %s)" % ("x" * (1 << 20)),
        "[%d 0 R]" % spec,
        "<< /Type /Filespec /F (f.bin) /EF << /F %d 0 R >> >>" % efile,
        t.stream("/Type /EmbeddedFile /Filter /FlateDecode", big),
        t.stream("/Filter /FlateDecode", big),
        t.stream("/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources << /Font << /T4 << /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a %d 0 R >> /Encoding << /Type /Encoding /Differences [97 /a] >> /FirstChar 97 /LastChar 97 /Widths [600] /Resources << >> >> >> >>" % empty, "BT /T4 10 Tf 0 0 Td (a) Tj ET"),
    ]
    assert len(objs) == first - 1
    objs += ["<< /Type /StructElem /S /Formula /P %d 0 R /K 0 /Alt %d 0 R /AF %d 0 R >>" % (docel, alt, afarr)] * elems
    return t.build_pdf(objs)

def math_tables_round2_pdf():
    """Council round 2 negative cases for the mathtables reads, one a page (objects numbered
    directly; doc()'s placeholders are quadratic in thousands of objects):
    1 Formula elements whose /AF file specification's /EF /F is the specification itself, a
      30,000-long chain of specifications, and /AF nested 30,000 arrays deep (no recursion);
    2 a form drawn with CMMI10 and q/Q around a CMMI10 run, then Helvetica body text: the class
      follows the graphics state (the body is text);
    3 300 embedded TrueType fonts (honest heads): no font-class budget is hit;
    4 a Type 3 font whose glyph procedure's deflate stream is cut inside BI, and a TrueType
      program that is corrupt deflate: unknown (analysis-truncated), never a definite class;
    5-20 4,000 Type 3 fonts (250 a page) sharing one 20,000-entry /XObject dictionary through
      procedures that run Do: the dictionary is read once;
    21 300 one-character /ActualText spans (ligatures): not truncated;
    22 a RoleMap chain of 12 hops and a two-type cycle: structure truncated, not 0;
    23 a Type 3 font whose glyph procedures put a 70 KB comment before BI /IM true (past the
       64 KB read): unknown, never a definite text font;
    24 a Formula whose Alt is 5,000 characters with no TeX in them (past the 4,096 scanned): TeX
       unknown (truncated);
    25 a parent-tree entry of 65,000 duplicate references to one Formula: counted once, not
       truncated."""
    import zlib

    objs = {}
    nxt = [100]

    def new(body):
        n = nxt[0]
        nxt[0] += 1
        objs[n] = body
        return n

    helv = new(HELV)
    pages = []

    def page(content, res="", extra=""):
        c = new(t.stream("", content).decode("latin-1"))
        pages.append((c, res, extra))

    # 1 /AF self-cycle, long chain, deep nesting
    selfspec = nxt[0]
    new("<< /Type /Filespec /EF << /F %d 0 R >> >>" % selfspec)
    chain0 = nxt[0]
    for k in range(30000):
        new("<< /Type /Filespec /EF << /F %d 0 R >> >>" % (chain0 + k + 1))
    new("<< >>")
    deep = "[" * 30000 + "]" * 30000
    root = new(None)
    docel = new(None)
    ptree = new(None)
    f1 = new(None)
    f2 = new(None)
    f3 = new(None)
    objs[f1] = "<< /Type /StructElem /S /Formula /P %d 0 R /K 0 /AF [%d 0 R] >>" % (docel, selfspec)
    objs[f2] = "<< /Type /StructElem /S /Formula /P %d 0 R /K 1 /AF [%d 0 R] >>" % (docel, chain0)
    objs[f3] = "<< /Type /StructElem /S /Formula /P %d 0 R /K 2 /AF %s >>" % (docel, deep)
    # 22: RoleMap chains
    r1 = new(None)
    r2 = new(None)
    objs[r1] = "<< /Type /StructElem /S /A0 /P %d 0 R /K 0 >>" % docel
    objs[r2] = "<< /Type /StructElem /S /X /P %d 0 R /K 1 >>" % docel
    rolemap = " ".join("/A%d /A%d" % (k, k + 1) for k in range(12)) + " /A12 /Table /X /Y /Y /X"
    objs[root] = "<< /Type /StructTreeRoot /K %d 0 R /ParentTree %d 0 R /RoleMap << %s >> >>" % (docel, ptree, rolemap)
    objs[docel] = "<< /Type /StructElem /S /Document /P %d 0 R >>" % root
    objs[ptree] = "<< /Nums [0 [%d 0 R %d 0 R %d 0 R] 1 [%d 0 R %d 0 R]] >>" % (f1, f2, f3, r1, r2)
    page("/P << /MCID 0 >> BDC " + words(10) + " EMC\n", extra="/StructParents 0")
    # 2 form and q/Q
    cmmi = new("<< /Type /Font /Subtype /Type1 /BaseFont /CMMI10 >>")
    form = new(t.stream("/Type /XObject /Subtype /Form /BBox [0 0 600 800] /Resources << /Font << /M1 %d 0 R >> >>" % cmmi, "BT /M1 10 Tf 72 700 Td (abc) Tj ET").decode("latin-1"))
    page("q /Fm Do Q\nBT /F1 10 Tf ET q BT /M1 10 Tf 72 650 Td (xy) Tj ET Q\nBT /F1 10 Tf 72 600 Td (body) Tj ET\n", res="/XObject << /Fm %d 0 R >> /Font << /F1 %d 0 R /M1 %d 0 R >>" % (form, helv, cmmi))
    # 3 300 honest embedded TrueType fonts
    sfnt = b"\x00\x01\x00\x00\x00\x02\x00\x20\x00\x01\x00\x00" + b"cmap" + b"\x00" * 12 + b"glyf" + b"\x00" * 12 + b"\x00" * 3000
    prog = new(t.stream("", sfnt, compress=True).decode("latin-1"))
    fonts = []
    for k in range(300):
        desc = new("<< /Type /FontDescriptor /FontName /Face%d /Flags 32 /FontBBox [0 0 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 %d 0 R >>" % (k, prog))
        fonts.append(new("<< /Type /Font /Subtype /TrueType /BaseFont /Face%d /FirstChar 32 /LastChar 126 /Widths [%s] /FontDescriptor %d 0 R >>" % (k, " ".join(["500"] * 95), desc)))
    page("".join("BT /T%d 8 Tf %d %d Td (ok) Tj ET\n" % (k, 40 + (k % 20) * 27, 760 - (k // 20) * 20) for k in range(300)), res="/Font << %s >>" % " ".join("/T%d %d 0 R" % (k, f) for k, f in enumerate(fonts)))
    # 4 cut and corrupt deflate heads
    proc = b"600 0 0 0 500 500 d1 q 500 0 0 500 0 0 cm BI /W 8 /H 8 /BPC 1 /IM true ID " + b"\x00" * 8 + b" EI Q" + b" % pad" * 400
    cut = zlib.compress(proc)[:30]
    cutproc = new(("<< /Filter /FlateDecode /Length %d >>\nstream\n" % len(cut)).encode("latin-1").decode("latin-1") + cut.decode("latin-1") + "\nendstream")
    t3cut = new("<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a %d 0 R >> /Encoding << /Type /Encoding /Differences [97 /a] >> /FirstChar 97 /LastChar 97 /Widths [600] /Resources << >> >>" % cutproc)
    junk = bytes((k * 37 + 11) % 256 for k in range(5000))
    badprog = new(("<< /Filter /FlateDecode /Length %d >>\nstream\n" % len(junk)) + junk.decode("latin-1") + "\nendstream")
    baddesc = new("<< /Type /FontDescriptor /FontName /Corrupt /Flags 32 /FontBBox [0 0 1000 1000] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 %d 0 R >>" % badprog)
    badtt = new("<< /Type /Font /Subtype /TrueType /BaseFont /Corrupt /FirstChar 32 /LastChar 126 /Widths [%s] /FontDescriptor %d 0 R >>" % (" ".join(["500"] * 95), baddesc))
    page("BT /C3 10 Tf 72 700 Td (aaaa) Tj ET BT /CT 10 Tf 72 680 Td (xyz) Tj ET\n" + words(10, 600), res="/Font << /F1 %d 0 R /C3 %d 0 R /CT %d 0 R >>" % (helv, t3cut, badtt))
    # 5-20 4,000 Type 3 fonts sharing one 20,000-entry /XObject dictionary
    mask = new(t.stream("/Type /XObject /Subtype /Image /Width 8 /Height 8 /ImageMask true /BitsPerComponent 1", b"\x00" * 8).decode("latin-1"))
    shared = new("<< %s >>" % " ".join("/Im%d %d 0 R" % (k, mask) for k in range(20000)))
    doproc = new(t.stream("", "600 0 0 0 500 500 d1 q 500 0 0 500 0 0 cm /Im0 Do Q").decode("latin-1"))
    t3s = [new("<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a %d 0 R >> /Encoding << /Type /Encoding /Differences [97 /a] >> /FirstChar 97 /LastChar 97 /Widths [600] /Resources << /XObject %d 0 R >> >>" % (doproc, shared)) for _ in range(4000)]
    for pg in range(16):
        ks = range(250 * pg, 250 * (pg + 1))
        page("".join("BT /G%d 6 Tf %d %d Td (a) Tj ET\n" % (k, 40 + (k % 25) * 20, 760 - ((k % 250) // 25) * 20) for k in ks), res="/Font << %s >>" % " ".join("/G%d %d 0 R" % (k, t3s[k]) for k in ks))
    # 21 300 one-character ActualText spans
    page("".join("/Span << /ActualText (fi) >> BDC BT /F1 10 Tf %d %d Td (x) Tj ET EMC\n" % (40 + (k % 30) * 18, 760 - (k // 30) * 14) for k in range(300)))
    # 22: the RoleMap page (parent-tree key 1)
    page("/P << /MCID 0 >> BDC " + words(5) + " EMC /P << /MCID 1 >> BDC " + words(5, 600) + " EMC\n", extra="/StructParents 1")
    # 23 a 5 KB comment before the image mask
    padproc = new(t.stream("", b"1000 0 0 0 750 750 d1\n%" + b"x" * 70000 + b"\nq 750 0 0 750 0 0 cm BI /IM true /W 8 /H 8 /BPC 1 ID " + b"\xff" * 8 + b" EI Q", compress=True).decode("latin-1"))
    t3pad = new("<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a %d 0 R >> /Encoding << /Type /Encoding /Differences [97 /a] >> /FirstChar 97 /LastChar 97 /Widths [1000] /Resources << >> >>" % padproc)
    page("BT /P3 10 Tf 72 700 Td (aaaa) Tj ET\n" + words(10, 600), res="/Font << /F1 %d 0 R /P3 %d 0 R >>" % (helv, t3pad))
    # 24, 25 (parent-tree keys 2 and 3)
    longalt = new(None)
    objs[longalt] = "<< /Type /StructElem /S /Formula /P %d 0 R /K 0 /Alt (%s) >>" % (docel, "plain words " * 420)
    dup = new(None)
    objs[dup] = "<< /Type /StructElem /S /Formula /P %d 0 R /K 0 >>" % docel
    dups = new("[%s]" % " ".join(["%d 0 R" % dup] * 65000))
    objs[ptree] = objs[ptree].replace("]] >>", "] 2 [%d 0 R] 3 %d 0 R] >>" % (longalt, dups))
    page("/P << /MCID 0 >> BDC " + words(5) + " EMC\n", extra="/StructParents 2")
    page("/P << /MCID 0 >> BDC " + words(5) + " EMC\n", extra="/StructParents 3")

    kids = []
    out = {1: "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot %d 0 R /MarkInfo << /Marked true >> >>" % root}
    for i, (c, res, extra) in enumerate(pages):
        pnum = 3 + i
        kids.append(pnum)
        r = res if "/Font" in res else res + " /Font << /F1 %d 0 R >>" % helv
        out[pnum] = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << %s >> /Contents %d 0 R %s >>" % (r, c, extra)
    out[2] = "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % k for k in kids), len(kids))
    out.update(objs)
    top = max(out)
    return t.build_pdf([out.get(n, "null") for n in range(1, top + 1)])


def math_matrix_pdf():
    """math-matrix and math-font-no-unicode precision (DocSlurp integration, 2026-11-16), a page each:
    1 a 2x2 matrix of text-size CMMI10 cells between CMEX10 parentheses: math-matrix;
    2 two fractions side by side inside CMEX10 parentheses (numerators over bars over
      denominators): no matrix;
    3 aligned equations, one CMEX10 parenthesis pair per row: no matrix;
    4 two sums side by side whose script-size limits line up, between parentheses: no matrix;
    5 math glyphs whose ToUnicode gives private-use code points: math-font-no-unicode;
    6 math glyphs whose ToUnicode gives the Adobe Symbol bracket pieces (U+F8EB...) beside
      readable math: delimiters, not unreadable;
    7 the same pieces from a font that is not a math extension font (CMSY10): unreadable;
    8 a Type 3 font whose glyph procedure is a stored deflate block, 100 KB of empty stored
      blocks, then BI /IM true (the read is cut before BI): unknown, not a vector font;
    9 a Type 3 font whose glyph procedures are 6 KB of path operators: a vector font, decided;
    10 a Type 3 font whose procedure runs Do (a form) in its first 4 KB and paints an inline image
       mask after it: a bitmap font (a Do in the head decides nothing)."""
    prose = _prose(740, 2)
    paren = lambda x, y: _row(y, ((x, "\\("),), "X")
    rparen = lambda x, y: _row(y, ((x, "\\)"),), "X")
    matrix = prose + _row(560, ((300, "a"), (318, "b")), "M") + _row(546, ((300, "c"), (318, "d")), "M") + paren(288, 551) + rparen(330, 551)
    fractions = prose + paren(288, 551) + _row(560, ((300, "a"), (318, "b")), "M") + "0.5 w 298 555 m 308 555 l S 316 555 m 326 555 l S\n" + _row(546, ((300, "c"), (318, "d")), "M") + rparen(330, 551)
    aligned = prose + "".join(paren(288, y) + _row(y, ((300, "x"), (318, "y")), "M") + rparen(330, y) for y in (560, 546))
    limits = prose + paren(288, 551) + "".join(_row(562, ((x, "n"),), "M", 7) + _row(548, ((x - 2, "S"),), "X") + _row(538, ((x, "k"),), "M", 7) for x in (300, 318)) + rparen(330, 551)
    cmap = lambda pairs: stream("", "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapName /X def 1 begincodespacerange <00> <FF> endcodespacerange %d beginbfchar %s endbfchar endcmap CMapName currentdict /CMap defineresource pop end end" % (len(pairs), " ".join("<%02X> <%04X>" % p for p in pairs)))
    pua = prose + _row(600, ((100, "abab abab"),), "P")
    pieces = prose + _row(600, ((100, "abab"),), "Q") + _row(600, ((200, "xyz w"),), "M")
    nonext = prose + _row(600, ((100, "abab"),), "N") + _row(600, ((200, "xyz w"),), "M")
    import zlib

    def stored(data, final=False):
        return bytes([1 if final else 0]) + len(data).to_bytes(2, "little") + (len(data) ^ 0xFFFF).to_bytes(2, "little") + data

    head = b"600 0 0 0 500 500 d1\n"
    tail = b"\nq 500 0 0 500 0 0 cm BI /IM true /W 8 /H 8 /BPC 1 ID " + b"\x00\x18\x3c\x7e\x7e\x3c\x18\x00" + b" EI Q"
    cutproc = b"\x78\x01" + stored(head) + stored(b"") * 20000 + stored(tail, True) + zlib.adler32(head + tail).to_bytes(4, "big")
    vecproc = "600 0 0 0 600 800 d1\n" + "".join("%d %d m %d %d %d %d %d %d c %d %d l h\n" % ((i * 37) % 500 + 50, (i * 53) % 700 + 50, 70, 55, 80, 75, 60, 90, 50, 80) for i in range(150)) + "f"
    t3 = "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a {%d} 0 R /b {%d} 0 R >> /Encoding << /Type /Encoding /Differences [97 /a /b] >> /FirstChar 97 /LastChar 98 /Widths [600 600] /Resources << >> >>"
    cut3 = prose + _row(600, ((100, "abab"),), "C")
    vec3 = prose + _row(600, ((100, "abab"),), "V")
    doproc = "1000 0 0 0 1000 1000 d1\nq /X1 Do Q\n" + ("% " + "x" * 76 + "\n") * 60 + "q 1000 0 0 1000 0 0 cm BI /IM true /W 8 /H 8 /BPC 1 ID \xff\x81\x81\x81\x81\x81\x81\xff EI\nQ\n"
    t3do = "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /a {11} 0 R /b {11} 0 R >> /Encoding << /Type /Encoding /Differences [97 /a /b] >> /FirstChar 97 /LastChar 98 /Widths [1000 1000] /Resources << /XObject << /X1 {12} 0 R >> >> >>"
    do3 = prose + _row(600, ((100, "abab"),), "D")
    res = "<< /Font << /F1 3 0 R /M {0} 0 R /X {1} 0 R /P {2} 0 R /Q {3} 0 R /N {6} 0 R /C {8} 0 R /V {10} 0 R /D {13} 0 R >> >>"
    return doc(
        [{"content": c, "resources": res} for c in (matrix, fractions, aligned, limits, pua, pieces, nonext, cut3, vec3, do3)],
        extras=[
            "<< /Type /Font /Subtype /Type1 /BaseFont /CMMI10 >>",  # 0
            "<< /Type /Font /Subtype /Type1 /BaseFont /CMEX10 >>",  # 1
            "<< /Type /Font /Subtype /Type1 /BaseFont /CMSY10 /ToUnicode {4} 0 R >>",  # 2
            "<< /Type /Font /Subtype /Type1 /BaseFont /CMEX10 /ToUnicode {5} 0 R >>",  # 3
            cmap([(0x61, 0xE001), (0x62, 0xE002)]),  # 4
            cmap([(0x61, 0xF8EB), (0x62, 0xF8ED)]),  # 5
            "<< /Type /Font /Subtype /Type1 /BaseFont /CMSY10 /ToUnicode {5} 0 R >>",  # 6
            stream("/Filter /FlateDecode", cutproc),  # 7
            t3 % (7, 7),  # 8
            stream("", vecproc),  # 9
            t3 % (9, 9),  # 10
            stream("", doproc),  # 11
            stream("/Type /XObject /Subtype /Form /BBox [0 0 1000 1000]", "0 0 100 100 re f"),  # 12
            t3do,  # 13
        ],
    )


def unsorted_kids_pdf():
    """A parent tree whose root /Kids are out of key order ([10-19, 0-9, 20-29], each kid's
    /Limits true): every page's entry (one Formula) is still found."""
    pages = [{"content": "/P << /MCID 0 >> BDC " + words(5) + " EMC\n", "extra": "/StructParents %d" % i} for i in range(30)]
    # extras: 0 root, 1 Document, 2-31 Formula elements, 32-34 kids, 35 tree
    extras = ["<< /Type /StructTreeRoot /K {1} 0 R /ParentTree {35} 0 R >>", "<< /Type /StructElem /S /Document /P {0} 0 R >>"]
    extras += ["<< /Type /StructElem /S /Formula /P {1} 0 R /K 0 >>"] * 30
    kid = lambda a, b: "<< /Limits [%d %d] /Nums [%s] >>" % (a, b, " ".join("%d {%d} 0 R" % (k, 2 + k) for k in range(a, b + 1)))
    extras += [kid(10, 19), kid(0, 9), kid(20, 29), "<< /Kids [{32} 0 R {33} 0 R {34} 0 R] >>"]
    return doc(pages, extras=extras, catalog_extra="/StructTreeRoot {0} 0 R /MarkInfo << /Marked true >>")


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
    "math_tables": math_tables_pdf,
    "math_tables_hostile": math_tables_hostile_pdf,
    "math_tables_council": math_tables_council_pdf,
    "math_tables_round2": math_tables_round2_pdf,
    "math_matrix": math_matrix_pdf,
    "unsorted_kids": unsorted_kids_pdf,
}
