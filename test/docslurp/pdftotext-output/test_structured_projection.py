"""Structured writers share line bounds and word facts, including at coordinate zero."""

import os
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t, tsv_rows  # noqa: E402

utils = t.Utils.from_args()


def elements(root, name):
    return [e for e in root.iter() if e.tag.rsplit("}", 1)[-1] == name]


def bounds(element):
    return tuple(float(element.attrib[k]) for k in ("xMin", "yMin", "xMax", "yMax"))


def tsv_bounds(row):
    x, y, width, height = map(float, row[6:10])
    return x, y, x + width, y + height


def close(a, b, tolerance=0.000002):
    return all(abs(x - y) <= tolerance for x, y in zip(a, b))


CASES = [
    ("zero-x", b"0 700 Td", "", ()),
    ("negative-x", b"-2 700 Td", "", ()),
    ("negative-y", b"72 790 Td", "", ()),
    ("rotated-text", b"0 1 -1 0 300 300 Tm", "", ()),
    ("rotated-page", b"72 300 Td", "/Rotate 90", ()),
    ("crop-zero-x", b"50 300 Td", "/CropBox [50 50 450 350]", ("-cropbox",)),
    ("rotated-crop-scaled", b"72 300 Td", "/Rotate 270 /CropBox [50 50 450 350]", ("-cropbox", "-r", "144")),
]

with t.TempDir() as d:
    for name, position, page_extra, flags in CASES:
        path = d.write(name + ".pdf", t.simple_page_pdf(b"BT /F1 10 Tf 0.2 0.4 0.6 rg " + position + b" (One Two) Tj ET", page_extra=page_extra))
        layout = ET.fromstring(utils.run("pdftotext", "-bbox-layout", "-wordmeta", *flags, path, "-"))
        flat = ET.fromstring(utils.run("pdftotext", "-bbox", "-wordmeta", *flags, path, "-"))
        rows = tsv_rows(utils.run("pdftotext", "-tsv", "-wordmeta", *flags, path, "-"))
        lines = elements(layout, "line")
        tsv_lines = [r for r in rows if r[0] == "4"]
        words = elements(layout, "word")
        t.check([w.text for w in words] == ["One", "Two"], name + ": words retain their order", repr([w.text for w in words]))
        t.check(len(lines) == len(tsv_lines) == 1, name + ": one line in each format")
        for line, row in zip(lines, tsv_lines):
            boxes = [bounds(w) for w in elements(line, "word")]
            expected = (min(b[0] for b in boxes), min(b[1] for b in boxes), max(b[2] for b in boxes), max(b[3] for b in boxes))
            t.check(close(bounds(line), expected), name + ": XML line is the union of its words", repr((bounds(line), expected)))
            t.check(close(tsv_bounds(row), expected), name + ": TSV line shares the word union", repr((tsv_bounds(row), expected)))
        if name in ("zero-x", "crop-zero-x"):
            t.check(bounds(words[0])[0] == 0, name + ": first word starts at zero")
        if name == "negative-x":
            t.check(bounds(words[0])[0] < 0, name + ": first word starts before zero")
        if name == "negative-y":
            t.check(bounds(words[0])[1] < 0, name + ": word extends above the page")

        flat_words = {w.text: w for w in elements(flat, "word")}
        tsv_words = {r[11]: r for r in rows if r[0] == "5"}
        t.check(set(flat_words) == set(tsv_words) == {w.text for w in words}, name + ": all formats contain the same words")
        for word in words:
            t.check(word.attrib == flat_words[word.text].attrib, name + ": XML modes share geometry and metadata for " + word.text)
            row = tsv_words[word.text]
            # TSV word geometry and numeric metadata have two decimals; XML has six.
            t.check(close(bounds(word), tsv_bounds(row), 0.011), name + ": TSV word geometry for " + word.text)
            meta = dict(zip(rows[0], row))
            for xml_key, tsv_key in (("fontName", "font_name"), ("rotation", "rotation"), ("spaceAfter", "space_after"), ("invisible", "invisible"), ("color", "color")):
                t.check(word.attrib[xml_key] == meta[tsv_key], name + ": shared " + xml_key + " for " + word.text)
            for xml_key, tsv_key in (("fontSize", "font_size"), ("baseline", "baseline")):
                t.check(abs(float(word.attrib[xml_key]) - float(meta[tsv_key])) <= 0.005001, name + ": shared " + xml_key + " for " + word.text)

t.finish()
