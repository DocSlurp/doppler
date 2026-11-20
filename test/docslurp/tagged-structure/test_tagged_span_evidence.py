"""Same-walk tagged word evidence and geometry-preserving fallback."""

import os
import re
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.environ["DOCSLURP_TESTLIB_DIR"])
import docslurp_testlib as t

utils = t.Utils.from_args()
layout = ("-bbox-layout", "-wordmeta")
flag = "-tagged-span-evidence"
evidence_attrs = re.compile(r' (?:artifact|mcid|mcScope|mcForm|mcPage|taggedSpansUnavailable)="[^"]*"')


def word(text, x=30, y=700):
    return f"BT /F1 12 Tf {x} {y} Td ({text}) Tj ET\n".encode()


def mark(properties, payload):
    return b"/P " + properties + b" BDC\n" + payload + b"EMC\n"


def parse(output):
    root = ET.fromstring(output)
    pages = root.findall(".//{*}page")
    words = [(node.text, {k: v for k, v in node.attrib.items() if k in ("artifact", "mcid", "mcScope", "mcForm")})
             for node in root.findall(".//{*}word")]
    return pages, words


with t.TempDir() as d:
    form = t.stream("/Type /XObject /Subtype /Form /BBox [0 0 200 30] /Resources << /Font << /F1 4 0 R >> >>",
                    mark(b"<</MCID 0>>", word("Form", 0, 12)))
    resources = "<< /Font << /F1 4 0 R >> /Properties << /PageMC << /MCID 0 >> >> /XObject << /Fm 6 0 R >> >>"
    content = (
        b"/Artifact <</Type /Pagination /Subtype /Header>> BDC\n"
        b"/Span BMC\n" + word("Header", y=740) + b"EMC EMC\n"
        b"/Artifact BMC\n" + word("Generic", y=720) + b"EMC\n"
        + mark(b"/PageMC", b"/Span BMC\n" + word("Body") + b"EMC\n"
               + mark(b"<</MCID 8>>", word("Nested", y=685))
               + mark(b"/Missing", word("Unresolved", y=670))
               + b"q 1 0 0 1 30 650 cm /Fm Do Q\n"
               b"q 1 0 0 1 30 620 cm /Fm Do Q\n" + word("Restored", y=590))
        + mark(b"<</MCID 4 /ActualText (Replacement)>>", word("Painted", y=560))
        + b"BT /F1 12 Tf 30 530 Td /P <</MCID 5>> BDC (Mix) Tj EMC (ed) Tj ET\n"
        + mark(b"<</MCID 6>>", word("Overprint", y=500)) + word("Overprint", y=500)
        + word("Plain", y=470)
        + b"/Artifact <</Type /Pagination>> BDC q 1 0 0 1 30 430 cm /Fm Do Q EMC\n"
    )
    path = d.write("evidence.pdf", t.simple_page_pdf(content, resources=resources, extra_objects=(form,)))
    expected = [
        ("Header", {"artifact": "pagination"}), ("Generic", {}),
        ("Body", {"mcid": "0", "mcScope": "0"}),
        ("Nested", {"mcid": "8", "mcScope": "0"}), ("Unresolved", {}),
        ("Form", {"mcid": "0", "mcScope": "1", "mcForm": "6 0"}),
        ("Form", {"mcid": "0", "mcScope": "2", "mcForm": "6 0"}),
        ("Restored", {"mcid": "0", "mcScope": "0"}),
        ("Replacement", {}), ("Mixed", {}), ("Overprint", {}), ("Plain", {}),
        ("Form", {"artifact": "pagination"}),
    ]
    for mode in ((), ("-compact-layout",)):
        plain = utils.run("pdftotext", *layout, *mode, path, "-")
        tagged = utils.run("pdftotext", *layout, *mode, flag, path, "-")
        pages, words = parse(tagged)
        t.check(words == expected, f"explicit pagination, separate Form invocations and conservative word evidence {mode}", words)
        t.check(pages[0].get("mcPage") == "1" and "taggedSpansUnavailable" not in pages[0].attrib, "valid page evidence is available")
        t.check(plain == evidence_attrs.sub("", tagged), f"opt-in preserves exact text and geometry {mode}", tagged)

    for name, content in (
        ("underflow", mark(b"<</MCID 0>>", word("Kept")) + b"EMC\n"),
        ("unclosed", b"/P <</MCID 0>> BDC\n" + word("Kept")),
        ("depth", b"/P BMC\n" * 4096 + mark(b"<</MCID 0>>", word("Kept")) + b"EMC\n" * 4096),
        ("duplicate", mark(b"<</MCID 0>>", word("Kept")) + mark(b"<</MCID 0>>", word("Other", y=660))),
        ("budget", mark(b"<</MCID 0>>", word("Kept")) + b"/P BMC EMC\n" * 16384),
    ):
        path = d.write(f"{name}.pdf", t.simple_page_pdf(content))
        plain = utils.run("pdftotext", *layout, path, "-")
        tagged = utils.run("pdftotext", *layout, flag, path, "-")
        pages, words = parse(tagged)
        t.check(pages[0].get("taggedSpansUnavailable") == "1" and words and all(not attrs for _, attrs in words),
                f"{name} falls back for the entire page", tagged)
        t.check(plain == evidence_attrs.sub("", tagged), f"{name} preserves existing output")

    malformed_form = t.stream("/Type /XObject /Subtype /Form /BBox [0 0 200 30]",
                              b"EMC /P <</MCID 0>> BDC")
    path = d.write("cross-form.pdf", t.simple_page_pdf(
        mark(b"<</MCID 0>>", word("Before") + b"/Fm Do\n" + word("After", y=670)),
        resources=resources, extra_objects=(malformed_form,)))
    tagged = utils.run("pdftotext", *layout, flag, path, "-")
    pages, words = parse(tagged)
    t.check(pages[0].get("taggedSpansUnavailable") == "1" and words == [("Before", {}), ("After", {})],
            "a Form cannot close its caller's MCID and substitute its own", tagged)

    for name, dictionary, payload in (
        ("bad-bbox", "/Type /XObject /Subtype /Form", b"/Span BMC EMC"),
        ("aborted-form", "/Type /XObject /Subtype /Form /BBox [0 0 200 30]", b"Tf"),
        ("leftover-operand", "/Type /XObject /Subtype /Form /BBox [0 0 200 30]", b"12"),
    ):
        path = d.write(f"{name}.pdf", t.simple_page_pdf(
            mark(b"<</MCID 0>>", word("Kept")) + b"/Fm Do",
            resources=resources, extra_objects=(t.stream(dictionary, payload),)))
        plain = utils.run("pdftotext", *layout, path, "-")
        tagged = utils.run("pdftotext", *layout, flag, path, "-")
        pages, words = parse(tagged)
        t.check(pages[0].get("taggedSpansUnavailable") == "1" and words == [("Kept", {})],
                f"{name} withholds page evidence", tagged)
        t.check(plain == evidence_attrs.sub("", tagged), f"{name} preserves existing output")

    # Form recursion has a separate limit from the marked-content stack.
    for form_count in (100, 101):
        forms = []
        for form_index in range(form_count):
            dictionary = "/Type /XObject /Subtype /Form /BBox [0 0 200 30]"
            if form_index + 1 < form_count:
                dictionary += f" /Resources << /XObject << /Next {7 + form_index} 0 R >> >>"
                payload = b"/Next Do"
            else:
                dictionary += " /Resources << /Font << /F1 4 0 R >> >>"
                payload = word("Tail", 0, 12)
            forms.append(t.stream(dictionary, payload))
        path = d.write(f"forms-{form_count}.pdf", t.simple_page_pdf(
            mark(b"<</MCID 0>>", word("Kept")) + b"/Fm Do",
            resources=resources, extra_objects=forms))
        plain = utils.run("pdftotext", *layout, path, "-")
        tagged = utils.run("pdftotext", *layout, flag, path, "-")
        pages, words = parse(tagged)
        if form_count == 100:
            t.check(pages[0].get("mcPage") == "1" and len(words) == 2 and dict(words) == {"Kept": {"mcid": "0", "mcScope": "0"}, "Tail": {}},
                    "the deepest supported Form preserves page evidence", tagged)
        else:
            t.check(pages[0].get("taggedSpansUnavailable") == "1" and "mcPage" not in pages[0].attrib and words == [("Kept", {})],
                    "the Form display cutoff discards evidence for previously read words", tagged)
        t.check(plain == evidence_attrs.sub("", tagged), "the Form chain preserves text and geometry")

    # /Contents arrays are one page scope. Evidence failure on one page cannot
    # leak or suppress evidence on the next.
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Open 9 0 R >> >> /Contents 5 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        t.stream("", mark(b"<</MCID 0>>", word("First")) + b"/Open Do"),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents [7 0 R 8 0 R] >>",
        t.stream("", b"/P <</MCID 0>> BDC\n"),
        t.stream("", word("Second") + b"EMC\n"),
        t.stream("/Type /XObject /Subtype /Form /BBox [0 0 30 30]", b"/Span BMC"),
    ]
    path = d.write("pages.pdf", t.build_pdf(objects))
    plain = utils.run("pdftotext", *layout, path, "-")
    tagged = utils.run("pdftotext", *layout, flag, path, "-")
    pages, words = parse(tagged)
    t.check(plain == evidence_attrs.sub("", tagged), "unclosed Form leaves ordinary extraction unchanged")
    t.check(len(pages) == 2 and pages[0].get("taggedSpansUnavailable") == "1" and "taggedSpansUnavailable" not in pages[1].attrib
            and words == [("First", {}), ("Second", {"mcid": "0", "mcScope": "0"})], "page reset and contents array scope", words)
    pages, words = parse(utils.run("pdftotext", *layout, flag, "-f", 2, "-l", 2, path, "-"))
    t.check(len(pages) == 1 and pages[0].get("mcPage") == "2" and words == [("Second", {"mcid": "0", "mcScope": "0"})],
            "a subset identifies the original page", words)
    # Every page is below its local cap. The fifth exhausts the run budget,
    # and subsequent pages add neither word evidence nor page attributes.
    kids = " ".join(f"{4 + 2 * page} 0 R" for page in range(6))
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        f"<< /Type /Pages /Kids [{kids}] /Count 6 >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    for page in range(6):
        objects.append(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >>"
            f" /Contents {5 + 2 * page} 0 R >>")
        objects.append(t.stream("", mark(b"<</MCID 0>>", word("Kept")) + b"/P BMC EMC\n" * 16000))
    path = d.write("run-limit.pdf", t.build_pdf(objects))
    plain = utils.run("pdftotext", *layout, path, "-")
    tagged = utils.run("pdftotext", *layout, flag, path, "-")
    pages, words = parse(tagged)
    limit = '  <taggedSpansRunLimit firstPage="5"/>\n'
    t.check(len(pages) == 6 and [page.get("mcPage") for page in pages] == ["1", "2", "3", "4", None, None]
            and pages[4].get("taggedSpansUnavailable") == "1" and "taggedSpansUnavailable" not in pages[5].attrib
            and words == [("Kept", {"mcid": "0", "mcScope": "0"})] * 4 + [("Kept", {})] * 2
            and tagged.count(limit) == 1, "run cap suppresses evidence from the first affected page onward", words)
    t.check(plain == evidence_attrs.sub("", tagged.replace(limit, "")), "run cap preserves text and geometry")


t.finish()
