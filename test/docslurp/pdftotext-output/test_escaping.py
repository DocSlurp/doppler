"""TSV text fields must never contain tabs/newlines, and -bbox/-bbox-layout
output must be well-formed XML even for hostile word text (ActualText)."""

import os
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t, tsv_rows  # noqa: E402

utils = t.Utils.from_args()


def page(actual_text, shown=b"xyz"):
    """A page whose only text run replaces `shown` with ActualText `actual_text` (PDF string body)."""
    content = b"BT /F1 12 Tf 72 700 Td /Span << /ActualText (" + actual_text + b") >> BDC (" + shown + b") Tj EMC ET"
    return t.simple_page_pdf(content)


def xml_words(out):
    root = ET.fromstring(out.encode("utf-8"))
    return ["".join(e.itertext()) for e in root.iter() if e.tag.endswith("}word") or e.tag == "word"]


# --- TSV: tabs, newlines, CR, FF, VT must not break the row structure -------
with t.TempDir() as d:
    path = d.write("tsv.pdf", page(rb"a\tb\nc\rd\fe\013f\001g"))
    out = utils.run("pdftotext", "-tsv", path, "-")
    rows = tsv_rows(out)
    t.check(all(len(r) == 12 for r in rows), "every TSV row has exactly 12 columns", repr(out))
    t.check(len(rows) == 5, "header + page + block + line + word rows only (got %d)" % len(rows), repr(out))
    words = [r for r in rows if r[0] == "5"]
    t.check(len(words) == 1 and words[0][11] == "a b c d e f g", "word text has controls replaced by single spaces", repr(words))
    t.check("\r" not in out and "\f" not in out and "\v" not in out, "no stray CR/FF/VT in TSV output", repr(out))

# --- TSV: ordinary text is untouched -----------------------------------------
with t.TempDir() as d:
    path = d.write("plain.pdf", t.simple_page_pdf(b"BT /F1 12 Tf 72 700 Td (Plain words 100%) Tj ET"))
    rows = tsv_rows(utils.run("pdftotext", "-tsv", path, "-"))
    t.check([r[11] for r in rows if r[0] == "5"] == ["Plain", "words", "100%"], "ordinary TSV text unchanged", repr(rows))

# --- XML: specials survive a real XML parser round trip ----------------------
special = '<a&b> "q" \'s\''
with t.TempDir() as d:
    path = d.write("xml.pdf", page(rb"<a&b> \"q\" 's'"))
    for mode in ("-bbox", "-bbox-layout"):
        out = utils.run("pdftotext", mode, path, "-")
        try:
            words = xml_words(out)
            t.check(words == [special], mode + ": &, <, >, quote and apostrophe round-trip through an XML parser", repr(words))
        except ET.ParseError as e:
            t.check(False, mode + ": output is well-formed XML", "%s\n%r" % (e, out))

# --- XML: control characters (illegal in XML 1.0) and line breaks ------------
with t.TempDir() as d:
    path = d.write("ctl.pdf", page(rb"a\001b\fc\nd\te"))
    for mode in ("-bbox", "-bbox-layout"):
        out = utils.run("pdftotext", mode, path, "-")
        try:
            words = xml_words(out)
            t.check(words == ["a b c d e"], mode + ": control characters become spaces and output parses", repr(words))
        except ET.ParseError as e:
            t.check(False, mode + ": output is well-formed XML with control characters in ActualText", "%s\n%r" % (e, out))
t.finish()
