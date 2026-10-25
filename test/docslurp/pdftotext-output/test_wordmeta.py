"""-wordmeta adds per-word metadata to -bbox, -bbox-layout and -tsv, and leaves
default output untouched when absent."""

import os
import re
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t, tsv_rows  # noqa: E402

utils = t.Utils.from_args()

CONTENT = (
    b"BT /F1 12 Tf 1 0 0 rg 72 700 Td (Red words ) Tj /F1 9 Tf 0 0 1 rg (blue) Tj ET "  # red, then smaller blue run
    b"BT /F1 12 Tf 3 Tr 0 g 72 600 Td (hidden ocr) Tj ET "  # render mode 3: invisible
    b"BT /F2 10 Tf 0 Tr 0.5 g 72 500 Td (odd) Tj ET"  # font name that needs XML escaping
)
EXTRA_FONT = ["<< /Type /Font /Subtype /Type1 /BaseFont /Foo#26Bar#22Q#3Cx#3E /Encoding /WinAnsiEncoding >>"]
RES = "<< /Font << /F1 4 0 R /F2 6 0 R >> >>"
PDF = t.simple_page_pdf(CONTENT, resources=RES, extra_objects=EXTRA_FONT)

BASE_COLS = 12
META_COLS = ["font_name", "font_size", "rotation", "space_after", "invisible", "color", "baseline"]
XML_ATTRS = ["fontName", "fontSize", "rotation", "spaceAfter", "invisible", "color", "baseline"]


def xml_words(out):
    root = ET.fromstring(out.encode("utf-8"))
    return {"".join(e.itertext()): e.attrib for e in root.iter() if e.tag.endswith("}word") or e.tag == "word"}


def expect(w, name, size, rot, space, inv, color, label):
    t.check(w["fontName"] == name, label + ": font name %r" % name, repr(w))
    t.check(abs(float(w["fontSize"]) - size) < 0.01, label + ": font size %g" % size, repr(w))
    t.check(w["rotation"] == str(rot), label + ": rotation %d" % rot, repr(w))
    t.check(w["spaceAfter"] == str(space), label + ": space_after %d" % space, repr(w))
    t.check(w["invisible"] == str(inv), label + ": invisible %d" % inv, repr(w))
    t.check(w["color"] == color, label + ": colour " + color, repr(w))
    t.check(re.fullmatch(r"-?\d+(\.\d+)?", w["baseline"]) is not None, label + ": numeric baseline", repr(w))


with t.TempDir() as d:
    path = d.write("meta.pdf", PDF)

    # --- XML modes -----------------------------------------------------------
    for mode in ("-bbox", "-bbox-layout"):
        words = xml_words(utils.run("pdftotext", mode, "-wordmeta", path, "-"))
        t.check(set(words) == {"Red", "words", "blue", "hidden", "ocr", "odd"}, mode + ": all words present", repr(sorted(words)))
        if set(words) != {"Red", "words", "blue", "hidden", "ocr", "odd"}:
            continue
        expect(words["Red"], "Helvetica", 12, 0, 1, 0, "#ff0000", mode + " Red")
        expect(words["blue"], "Helvetica", 9, 0, 0, 0, "#0000ff", mode + " blue")
        expect(words["hidden"], "Helvetica", 12, 0, 1, 1, "#000000", mode + " hidden")
        expect(words["ocr"], "Helvetica", 12, 0, 0, 1, "#000000", mode + " ocr")
        expect(words["odd"], 'Foo&Bar"Q<x>', 10, 0, 0, 0, "#808080", mode + " odd (escaped font name round-trips)")
        t.check(float(words["Red"]["baseline"]) == float(words["words"]["baseline"]), mode + ": words on a line share a baseline", repr(words))

    # --- TSV -----------------------------------------------------------------
    rows = tsv_rows(utils.run("pdftotext", "-tsv", "-wordmeta", path, "-"))
    t.check(rows[0][BASE_COLS:] == META_COLS, "-tsv: header gains the metadata columns", repr(rows[0]))
    t.check(all(len(r) == BASE_COLS + len(META_COLS) for r in rows), "-tsv: every row has the same number of columns", repr(rows))
    by_text = {r[11]: dict(zip(META_COLS, r[BASE_COLS:])) for r in rows if r[0] == "5"}
    t.check(by_text["Red"]["color"] == "#ff0000" and by_text["Red"]["invisible"] == "0" and by_text["Red"]["space_after"] == "1", "-tsv: Red word metadata", repr(by_text["Red"]))
    t.check(by_text["blue"]["font_size"] == "9.00" and by_text["blue"]["color"] == "#0000ff", "-tsv: blue word metadata", repr(by_text["blue"]))
    t.check(by_text["hidden"]["invisible"] == "1" and by_text["ocr"]["invisible"] == "1", "-tsv: Tr 3 text is invisible", repr(by_text))
    t.check(by_text["odd"]["font_name"] == 'Foo&Bar"Q<x>', "-tsv: font name is not XML-escaped", repr(by_text["odd"]))
    t.check(all(not any(r[BASE_COLS:]) for r in rows[1:] if r[0] != "5"), "-tsv: page/block/line rows have empty metadata columns", repr(rows))

    # --- rotation field --------------------------------------------------------
    rot_pdf = t.simple_page_pdf(b"BT /F1 12 Tf 0 1 -1 0 300 300 Tm (Up) Tj ET BT /F1 12 Tf 0 -1 1 0 100 400 Tm (Down) Tj ET BT /F1 12 Tf -1 0 0 -1 400 600 Tm (Flip) Tj ET BT /F1 12 Tf 72 700 Td (Flat) Tj ET")
    words = xml_words(utils.run("pdftotext", "-bbox", "-wordmeta", d.write("rot.pdf", rot_pdf), "-"))
    got = {k: v["rotation"] for k, v in words.items()}
    t.check(sorted(got.values()) == ["0", "1", "2", "3"], "rotation field takes values 0..3 for rotated text", repr(got))

    # --- default output is untouched --------------------------------------------
    out = utils.run("pdftotext", "-bbox", path, "-")
    t.check(not any(a in out for a in XML_ATTRS), "-bbox without -wordmeta has no extra attributes", out)
    t.check(re.search(r'<word xMin="[-\d.]+" yMin="[-\d.]+" xMax="[-\d.]+" yMax="[-\d.]+">Red</word>', out) is not None, "-bbox word element has exactly the four coordinates", out)
    out = utils.run("pdftotext", "-bbox-layout", path, "-")
    t.check(not any(a in out for a in XML_ATTRS), "-bbox-layout without -wordmeta has no extra attributes", out)
    rows = tsv_rows(utils.run("pdftotext", "-tsv", path, "-"))
    t.check(all(len(r) == BASE_COLS for r in rows) and rows[0][-1] == "text", "-tsv without -wordmeta keeps 12 columns", repr(rows[0]))

    # --- misuse -------------------------------------------------------------
    proc = utils.run("pdftotext", "-wordmeta", path, "-", check=False)
    t.check(proc == "", "-wordmeta alone writes no text", repr(proc))
t.finish()
