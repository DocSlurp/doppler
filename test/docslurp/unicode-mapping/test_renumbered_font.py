"""Recover compact symbolic TrueType subsets with a byte-to-glyph symbol cmap."""
import io
import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import um_fixtures as fixtures

try:
    from fontTools import subset
    from fontTools.ttLib import TTFont
    from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
    from fontTools.ttLib.tables._g_l_y_f import Glyph
except ImportError:
    sys.exit(77)

t = fixtures.t
utils = t.Utils.from_args()
if not shutil.which("fc-match"):
    sys.exit(77)
name, path = subprocess.check_output(
    ["fc-match", "-f", "%{postscriptname}\n%{file}", "DejaVu Serif"], text=True
).split("\n", 1)
if name != "DejaVuSerif":
    sys.exit(77)


def sample(letters, extra="", preserve_records=False, source_path=path, show=None):
    font = TTFont(source_path)
    chars = list(dict.fromkeys(letters))
    glyph_names = [font.getBestCmap()[ord(c)] for c in chars]
    original_ids = [font.getGlyphID(g) for g in glyph_names]
    subsetter = subset.Subsetter()
    subsetter.populate(text=letters)
    subsetter.subset(font)
    if preserve_records:
        font.recalcBBoxes = False
        # Copy simple records as a subsetter can do without rewriting glyph IDs.
        original = TTFont(source_path)
        offsets = original["loca"].locations
        outlines = original.reader["glyf"]
        for glyph, gid in zip(glyph_names, original_ids):
            if original["glyf"][glyph].numberOfContours > 0:
                font["glyf"][glyph] = Glyph(outlines[offsets[gid]:offsets[gid + 1]])
    assert original_ids != [font.getGlyphID(g) for g in glyph_names]
    symbol = CmapSubtable.newSubtable(4)
    symbol.platformID, symbol.platEncID, symbol.language = 3, 0, 0
    symbol.cmap = {0xf001 + i: glyph for i, glyph in enumerate(glyph_names)}
    font["cmap"].tables = [symbol]
    buf = io.BytesIO()
    font.save(buf)
    data = buf.getvalue()
    encoded = bytes(chars.index(c) + 1 for c in letters).hex()
    widths = " ".join(["680"] * len(chars))
    if show is None:
        show = f"<{encoded}> Tj"
    return t.simple_page_pdf(
        f"BT /F2 14 Tf 50 500 Td {show} ET".encode(),
        resources="<< /Font << /F2 6 0 R >> >>",
        extra_objects=[
            f"<< /Type /Font /Subtype /TrueType /BaseFont /AAAAAA+DejaVuSerif /FirstChar 1 /LastChar {len(chars)} /Widths [{widths}] /FontDescriptor 7 0 R {extra} >>",
            "<< /Type /FontDescriptor /FontName /AAAAAA+DejaVuSerif /Flags 4 /FontBBox [0 -200 1100 1000] /Ascent 900 /Descent -200 /CapHeight 800 /StemV 80 /ItalicAngle 0 /FontFile2 8 0 R >>",
            t.stream(f"/Length1 {len(data)}", data, compress=True),
            t.stream("", b"begincmap 1 begincodespacerange <00> <ff> endcodespacerange 1 beginbfrange <01> <04> <0057> endbfrange endcmap"),
        ],
    )


with t.TempDir() as tmp:
    pdf = tmp.write("renumbered.pdf", sample("2468"))
    assert "2468" not in utils.run("pdftotext", pdf, "-")
    result = utils.run("pdftotext", "-recover-fonts", pdf, "-")
    assert "2468" in result, repr(result)
    pdf = tmp.write("author-mapped.pdf", sample("2468", "/ToUnicode 9 0 R"))
    assert "WXYZ" in utils.run("pdftotext", "-recover-fonts", pdf, "-")
    # Latin e and Cyrillic ie share an outline in this face.
    pdf = tmp.write("latin.pdf", sample("true fret"))
    assert "true fret" in utils.run("pdftotext", "-recover-fonts", pdf, "-")
    pdf = tmp.write("mixed.pdf", sample("true fret ψξ"))
    mixed = utils.run("pdftotext", "-recover-fonts", pdf, "-")
    assert "ξ" in mixed and "true" not in mixed, repr(mixed)
    pdf = tmp.write("preserved-records.pdf", sample("true fret ψξ", preserve_records=True))
    recovered = utils.run("pdftotext", "-recover-fonts", pdf, "-")
    assert "true fret" in recovered and "ξ" in recovered, repr(recovered)
    pdf = tmp.write("currency.pdf", sample("turf ä €"))
    recovered = utils.run("pdftotext", "-recover-fonts", pdf, "-")
    assert "ä" in recovered, repr(recovered)

    # Neutral currency symbols must not unlock Latin guesses for unrelated
    # lookalikes (e.g. Latin e / Cyrillic ie) in a damaged mixed font.
    pdf = tmp.write("neutral-symbol.pdf", sample("true fret €"))
    assert "true fret" not in utils.run("pdftotext", "-recover-fonts", pdf, "-")

    # Model a face that uses the exact same glyph for comma and low quote.
    installed = TTFont(path)
    for table in installed["cmap"].tables:
        if table.isUnicode() and 0x201a in table.cmap:
            table.cmap[0x201a] = table.cmap[ord(",")]
    buf = io.BytesIO()
    installed.save(buf)
    face = tmp.write("reference.ttf", buf.getvalue())
    config = f"<fontconfig><dir>{Path(face).parent}</dir></fontconfig>"
    os.environ["FONTCONFIG_FILE"] = str(tmp.write("fonts.conf", config.encode()))

    for label, show, decimal in [
        ("one-string", "<01020304> Tj", True),
        ("kerned", "[<01> 2 <02> -1 <0304>] TJ", True),
        ("separate", "<0102> Tj <0304> Tj", False),
        ("large-gap", "[<0102> -900 <0304>] TJ", False),
        ("trailing", "<010302> Tj", False),
    ]:
        pdf = tmp.write(f"{label}.pdf", sample("7,34", source_path=face, show=show))
        recovered = utils.run("pdftotext", "-recover-fonts", pdf, "-")
        assert ("," in recovered) == decimal, (label, repr(recovered))
        if decimal:
            assert "7,34" in recovered, repr(recovered)
    for amount in ("−7,34 €", "+7,34%", "$07,30"):
        pdf = tmp.write("amount.pdf", sample(amount, source_path=face))
        recovered = utils.run("pdftotext", "-recover-fonts", pdf, "-")
        assert amount in recovered, (amount, repr(recovered))
