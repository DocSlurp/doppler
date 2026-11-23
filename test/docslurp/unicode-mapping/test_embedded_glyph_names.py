"""The post table of a compact TrueType subset can survive loss of ToUnicode."""
from io import BytesIO
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import um_fixtures as fixtures

try:
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.ttGlyphPen import TTGlyphPen
except ImportError:
    sys.exit(77)

t = fixtures.t
utils = t.Utils.from_args()
names = [".notdef", "A", "uni03B2", "u1F642", "uniD800", "cid00001"]
font = FontBuilder(1000, isTTF=True)
font.setupGlyphOrder(names)
pen = TTGlyphPen(None)
pen.moveTo((0, 0))
pen.lineTo((250, 500))
pen.lineTo((500, 0))
pen.closePath()
glyph = pen.glyph()
font.setupGlyf({n: glyph for n in names})
font.setupHorizontalMetrics({n: (600, 0) for n in names})
font.setupHorizontalHeader(ascent=800, descent=-200)
font.setupCharacterMap({0x41: "A"})
font.setupNameTable({"familyName": "FixtureNamedSubset", "styleName": "Regular"})
font.setupOS2()
font.setupPost()
buf = BytesIO()
font.save(buf)
data = buf.getvalue()

with t.TempDir() as tmp:
    pdf = tmp.write("embedded-names.pdf", t.simple_page_pdf(
        b"BT /F2 15 Tf 40 500 Td <00010002000300040005> Tj ET",
        resources="<< /Font << /F2 6 0 R >> >>",
        extra_objects=[
            "<< /Type /Font /Subtype /Type0 /BaseFont /AAAAAA+FixtureNamedSubset /Encoding /Identity-H /DescendantFonts [7 0 R] >>",
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /AAAAAA+FixtureNamedSubset /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 8 0 R /CIDToGIDMap /Identity /DW 600 >>",
            "<< /Type /FontDescriptor /FontName /AAAAAA+FixtureNamedSubset /Flags 4 /FontBBox [0 0 500 500] /Ascent 800 /Descent -200 /CapHeight 500 /StemV 80 /ItalicAngle 0 /FontFile2 9 0 R >>",
            t.stream(f"/Length1 {len(data)}", data),
        ],
    ))
    assert "Aβ🙂" not in utils.run("pdftotext", pdf, "-")
    text = utils.run("pdftotext", "-recover-fonts", pdf, "-").strip()
    assert text.startswith("Aβ🙂"), repr(text)
