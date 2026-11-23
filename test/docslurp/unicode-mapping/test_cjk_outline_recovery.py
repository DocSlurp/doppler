"""Generate equivalent Type 3 and CID-keyed CFF glyphs from an installed face."""
import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import um_fixtures as fixtures

try:
    from fontTools import subset
    from fontTools.pens.recordingPen import RecordingPen
    from fontTools.ttLib import TTFont
except ImportError:
    sys.exit(77)

t = fixtures.t
utils = t.Utils.from_args()
if not shutil.which("fc-match"):
    sys.exit(77)
name, filename, index = subprocess.check_output(
    ["fc-match", "-f", "%{postscriptname}\n%{file}\n%{index}", "Noto Sans CJK JP"], text=True
).splitlines()
if name != "NotoSansCJKjp-Regular":
    sys.exit(77)
font = TTFont(filename, fontNumber=int(index))
char = "海"
glyph = font.getBestCmap()[ord(char)]
gid = font.getGlyphID(glyph)
width = font["hmtx"][glyph][0]
pen = RecordingPen()
font.getGlyphSet()[glyph].draw(pen)
commands = [f"{width} 0 -1000 -1000 2000 2000 d1"]
ops = {"moveTo": "m", "lineTo": "l", "curveTo": "c", "closePath": "h"}
for op, points in pen.value:
    commands.append(" ".join(str(v) for xy in points for v in xy) + " " + ops[op])
outline = "\n".join(commands) + "\nf"


def type3(program):
    return t.simple_page_pdf(
        b"BT /F2 15 Tf 40 500 Td <41> Tj ET", resources="<< /Font << /F2 6 0 R >> >>",
        extra_objects=[
            f"<< /Type /Font /Subtype /Type3 /FontBBox [-1000 -1000 2000 2000] /FontMatrix [.001 0 0 .001 0 0] /FirstChar 65 /LastChar 65 /Widths [{width}] /CharProcs << /g{gid:X} 7 0 R >> /Encoding << /Differences [65 /g{gid:X}] >> >>",
            t.stream("", program.encode()),
        ],
    )


def cid_cff():
    subsetter = subset.Subsetter()
    subsetter.populate(text=char)
    subsetter.subset(font)
    top = font["CFF "].cff[0]
    originals = top.charset
    compact = [".notdef"] + [f"cid{i:05d}" for i in range(1, len(originals))]
    cid = originals.index(glyph)
    top.CharStrings.charStrings = {new: top.CharStrings.charStrings[old] for old, new in zip(originals, compact)}
    top.charset = compact
    data = font.getTableData("CFF ")
    return t.simple_page_pdf(
        f"BT /F2 15 Tf 40 500 Td <{cid:04X}> Tj ET".encode(), resources="<< /Font << /F2 6 0 R >> >>",
        extra_objects=[
            f"<< /Type /Font /Subtype /Type0 /BaseFont /AAAAAA+{name} /Encoding /Identity-H /DescendantFonts [7 0 R] >>",
            f"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /AAAAAA+{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 8 0 R >>",
            f"<< /Type /FontDescriptor /FontName /AAAAAA+{name} /Flags 4 /FontBBox [-1000 -1000 2000 2000] /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 /ItalicAngle 0 /FontFile3 9 0 R >>",
            t.stream("/Subtype /CIDFontType0C", data),
        ],
    )


with t.TempDir() as tmp:
    for filename, data in [("path.pdf", type3(outline)), ("cff.pdf", cid_cff())]:
        pdf = tmp.write(filename, data)
        assert char not in utils.run("pdftotext", pdf, "-")
        assert char in utils.run("pdftotext", "-recover-fonts", pdf, "-"), filename
    for program in [outline.replace(" m", " m 1 1 l", 1), outline + "\n0 0 2 2 re f"]:
        pdf = tmp.write("different-path.pdf", type3(program))
        assert char not in utils.run("pdftotext", "-recover-fonts", pdf, "-")
    pdf = tmp.write("missing-font.pdf", type3(outline))
    config = tmp.write("fonts.conf", b"<fontconfig></fontconfig>")
    os.environ["FONTCONFIG_FILE"] = str(config)
    assert utils.run("pdftotext", "-recover-fonts", pdf, "-") == utils.run("pdftotext", pdf, "-")
