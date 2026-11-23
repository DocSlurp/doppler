"""Recover byte-encoded CFF glyphs only when their original CID and shape agree."""
import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import um_fixtures as fixtures

try:
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.t2CharStringPen import T2CharStringPen
    from fontTools.ttLib import TTFont
except ImportError:
    sys.exit(77)

t = fixtures.t
utils = t.Utils.from_args()
if not shutil.which("fc-match"):
    sys.exit(77)


def document(face, text, swap=False, mapping=""):
    name, path, index = subprocess.check_output(
        ["fc-match", "-f", "%{postscriptname}\n%{file}\n%{index}", face], text=True
    ).splitlines()
    if name != face:
        sys.exit(77)
    source = TTFont(path, fontNumber=int(index))
    glyphs = source.getGlyphSet()
    names = [source.getBestCmap()[ord(c)] for c in text]
    aliases = [f"cid{int(n[3:])}" for n in names]
    builder = FontBuilder(source["head"].unitsPerEm, isTTF=False)
    order = [".notdef", *reversed(aliases)]
    builder.setupGlyphOrder(order)
    strings = {}
    shapes = [".notdef", *(names if swap else reversed(names))]
    for alias, original in zip(order, shapes):
        pen = T2CharStringPen(source["hmtx"][original][0], glyphs)
        glyphs[original].draw(pen)
        strings[alias] = pen.getCharString()
    builder.setupCFF("FixtureSubset", {}, strings, {})
    data = builder.font.getTableData("CFF ")
    return t.simple_page_pdf(b"BT /F2 16 Tf 40 500 Td <5863> Tj ET", resources="<< /Font << /F2 6 0 R >> >>",
        extra_objects=[
            f"<< /Type /Font /Subtype /Type1 /BaseFont /ABCDEF+{face} /FirstChar 88 /LastChar 99 /Widths [{'1000 ' * 12}] /Encoding << /Differences [88 /{aliases[0]} 99 /{aliases[1]}] >> /FontDescriptor 7 0 R {mapping} >>",
            f"<< /Type /FontDescriptor /FontName /ABCDEF+{face} /Flags 4 /FontBBox [-200 -200 1200 1200] /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 /ItalicAngle 0 /FontFile3 8 0 R >>",
            t.stream("/Subtype /Type1C", data),
            t.stream("", b"begincmap 1 begincodespacerange <00> <ff> endcodespacerange 2 beginbfchar <58> <0050> <63> <0051> endbfchar endcmap"),
        ])


with t.TempDir() as tmp:
    for face, text in [("NotoSansCJKjp-Regular", "森雪"), ("NotoSansCJKsc-Regular", "雨林")]:
        pdf = tmp.write("named.pdf", document(face, text))
        assert text not in utils.run("pdftotext", pdf, "-")
        out = utils.run("pdftotext", "-recover-fonts", pdf, "-")
        assert text in out, (face, repr(out))
        pdf = tmp.write("wrong-outline.pdf", document(face, text, swap=True))
        assert utils.run("pdftotext", "-recover-fonts", pdf, "-") == utils.run("pdftotext", pdf, "-")
    pdf = tmp.write("mapped.pdf", document(face, text, mapping="/ToUnicode 9 0 R"))
    assert "PQ" in utils.run("pdftotext", "-recover-fonts", pdf, "-")
    pdf = tmp.write("missing.pdf", document(face, text))
    os.environ["FONTCONFIG_FILE"] = str(tmp.write("fonts.conf", b"<fontconfig></fontconfig>"))
    assert utils.run("pdftotext", "-recover-fonts", pdf, "-") == utils.run("pdftotext", pdf, "-")
