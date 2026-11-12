"""Page facts (doppler-page-facts/1): pdfpagefacts and pdftotext -page-facts-fd.

One fixture per fact family (pf_fixtures.py), checked for: the JSON Schema, the facts each
family is about, byte-identical repeated runs, page locality (a page's line from -f p -l p
equals its line in a whole run), pdftotext -page-facts-fd == pdfpagefacts, pdftotext XHTML
unchanged by facts, the -images scan/image attributes, exit codes, and that no document
string (URIs, field names, file names) reaches the output.
"""

import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402
import pf_fixtures as fx  # noqa: E402

import argparse  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("--bindir", required=True)
parser.add_argument("--testdata", default="")
args, _ = parser.parse_known_args()
BIN = args.bindir
HERE = os.path.dirname(os.path.abspath(__file__))
SCHEMA = json.load(open(os.path.join(HERE, "..", "..", "..", "docslurp", "page-facts", "doppler-page-facts-1.schema.json")))
FAMILIES = ["fonts", "oddtext", "images-detail", "vectors", "forms", "annots", "ocg", "structure"]


def facts(path, *extra, stdin=None):
    cmd = [os.path.join(BIN, "pdfpagefacts")] + list(extra) + [path]
    p = subprocess.run(cmd, capture_output=True, timeout=120, input=stdin)
    return p.returncode, p.stdout, p.stderr


def lines(out):
    return [json.loads(l) for l in out.decode("ascii").splitlines()]


def pdftotext_facts(path, *extra, images=False, glyphs=False):
    """pdftotext as pdftomarkdown runs it, with the facts on descriptor 3: (xhtml, facts)."""
    r, w = os.pipe()
    cmd = [os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-wordmeta", "-rules", "-compact-layout"]
    cmd += ["-images"] if images else []
    cmd += ["-glyphs"] if glyphs else []
    cmd += ["-page-facts-fd", str(w)] + list(extra) + [path, "-"]
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, pass_fds=(w,))
    os.close(w)
    chunks = []
    import threading

    def drain():
        while True:
            b = os.read(r, 65536)
            if not b:
                break
            chunks.append(b)

    th = threading.Thread(target=drain)
    th.start()
    xhtml, _ = p.communicate(timeout=120)
    th.join()
    os.close(r)
    return p.returncode, xhtml, b"".join(chunks)


def pdftotext_plain(path, images=False):
    cmd = [os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-wordmeta", "-rules", "-compact-layout"] + (["-images"] if images else []) + [path, "-"]
    return subprocess.run(cmd, capture_output=True, timeout=120).stdout


def check_schema(out, label):
    bad = []
    for i, line in enumerate(out.decode("ascii", errors="replace").splitlines()):
        try:
            obj = json.loads(line)
        except ValueError as e:
            bad.append("line %d: not JSON: %s" % (i + 1, e))
            continue
        bad += ["line %d: %s" % (i + 1, e) for e in t.schema_errors(obj, SCHEMA)]
    t.check(not bad and out.endswith(b"\n") and out.isascii(), "%s: every line valid against the schema, ASCII" % label, "\n".join(bad[:10]))


def page(ls, n):
    return next(l for l in ls if l["type"] == "page" and l["page"] == n)


def doc_line(ls):
    return ls[0]


with t.TempDir() as d:
    paths = {name: d.write(name + ".pdf", make()) for name, make in fx.ALL.items()}

    # --- schema, line order, determinism, level shape -----------------------------
    for name, path in sorted(paths.items()):
        for level in (["-level", "basic"], ["-level", "full"]):
            runs = [facts(path, *level) for _ in range(3)]
            rc, out, err = runs[0]
            t.check(rc == 0, "%s %s: exit 0" % (name, level[1]), err.decode(errors="replace"))
            check_schema(out, "%s %s" % (name, level[1]))
            t.check(all(r[1] == out for r in runs), "%s %s: three runs byte-identical" % (name, level[1]))
            ls = lines(out)
            types = [l["type"] for l in ls]
            t.check(types[0] == "doc" and types[-1] == "run" and all(x == "page" for x in types[1:-1]), "%s %s: doc, pages, run" % (name, level[1]), repr(types))
            nums = [l["page"] for l in ls if l["type"] == "page"]
            t.check(nums == sorted(nums) and len(nums) == ls[-1]["pagesEmitted"] + ls[-1]["pagesFailed"], "%s %s: pages ascending and counted" % (name, level[1]), repr(nums))
            # no document strings in the output (producer/creator excepted, which these fixtures do not hold)
            if name in ("annots", "forms"):
                t.check(b"secret" not in out.lower() and b"note " not in out and b"Hello" not in out, "%s: no URI, field name, file name or text in the output" % name)
        for fam in FAMILIES:
            rc, out, err = facts(paths[name], "-include", fam)
            check_schema(out, "%s +%s" % (name, fam)) if name in ("scans", "forms", "structure") else None

    # basic has only the basic families
    ls = lines(facts(paths["scans"])[1])
    p1 = page(ls, 1)
    t.check(set(k for k in p1 if k not in ("schema", "type", "page")) == {"geometry", "text", "images", "parseWarnings", "unusualFeatures"}, "basic: geometry, text, images, warnings, features only", repr(sorted(p1)))
    t.check("controlCharRatio" not in p1["text"] and "items" not in p1["images"], "basic: no odd-text ratios or image items")
    t.check(doc_line(ls)["level"] == "basic" and "producer" not in doc_line(ls) and "acroForm" not in doc_line(ls), "basic doc line: no structure or form fields")
    ls = lines(facts(paths["scans"], "-include", "fonts,vectors")[1])
    p1 = page(ls, 1)
    t.check("fonts" in p1 and "vectors" in p1 and "forms" not in p1 and doc_line(ls)["include"] == ["fonts", "vectors"], "-include adds exactly the named families", repr(sorted(p1)))
    t.check(facts(paths["scans"], "-include", "fonts,bogus")[0] == 99 and facts(paths["scans"], "-level", "max")[0] == 99, "bad level or family: exit 99")

    # --- A geometry -------------------------------------------------------------------
    ls = lines(facts(paths["geometry"], "-level", "full")[1])
    g = [page(ls, n)["geometry"] for n in range(1, 5)]
    t.check(g[0] == {"widthPt": 612, "heightPt": 792, "rotation": 0, "userUnit": 1}, "geometry: plain page", repr(g[0]))
    t.check(g[1]["rotation"] == 90 and g[1]["widthPt"] == 792 and "rotated-page" in page(ls, 2)["unusualFeatures"], "geometry: /Rotate 90 swaps the size, rotated-page", repr(g[1]))
    t.check("huge-page" in page(ls, 3)["unusualFeatures"] and "huge-page" not in page(ls, 1)["unusualFeatures"], "geometry: huge-page")
    t.check(g[3]["userUnit"] == 2 and g[3]["cropBox"] == [50, 50, 500, 700] and g[3]["mediaBox"] == [0, 0, 612, 792] and g[3]["widthPt"] == 450, "geometry: UserUnit, CropBox != MediaBox", repr(g[3]))
    t.check("mediaBox" not in g[0], "geometry: boxes absent when equal")

    # --- B text and C images: scans --------------------------------------------------------
    ls = lines(facts(paths["scans"], "-level", "full")[1])
    s1, s2, s3, s4, s5, s6 = (page(ls, n) for n in range(1, 7))
    t.check(s1["images"]["scan"] == "single" and s1["images"]["scanImage"] == 1 and s1["text"]["textLayer"] == "invisible", "scan with an invisible OCR layer: single, invisible text", repr(s1["images"]))
    t.check(s1["text"]["invisibleOverFullPageImage"] and s1["text"]["textOverImageRatio"] == 1 and "scan-page" in s1["unusualFeatures"], "OCR layer over the full-page image")
    t.check(s1["images"]["items"][0]["filter"] == "dct" and s1["images"]["items"][0]["copy"] == "jpeg" and s1["images"]["items"][0]["dpi"] == [300, 300], "scan image: DCT, copy jpeg, 300 dpi", repr(s1["images"]["items"]))
    t.check(s2["images"]["scan"] == "none" and s2["images"]["fullPageImage"] and s2["text"]["visibleWordCount"] == 200, "a backdrop under 200 visible words is not a scan", repr(s2["images"]))
    t.check(s3["images"]["scan"] == "tiled" and s3["images"]["imageCount"] == 4 and s3["images"]["largestImageCoverage"] == 0.25, "four strips: tiled", repr(s3["images"]))
    t.check(s4["images"]["scan"] == "layered" and "layered-scan" in s4["unusualFeatures"] and s4["images"]["maskCount"] == 1, "background + stencil: layered", repr(s4["images"]))
    t.check(s5["images"]["scan"] == "layered" and s5["images"]["items"][0]["softMask"], "soft-masked full-page image: layered")
    t.check(s6["images"]["inlineCount"] == 1 and s6["images"]["items"][0]["inline"] and s6["images"]["items"][0]["copy"] == "render" and not s6["images"]["fullPageImage"], "inline image: render class, no scan", repr(s6["images"]))

    # --- B fonts and odd text -------------------------------------------------------------------
    ls = lines(facts(paths["fonts"], "-level", "full")[1])
    f1, f2, f3, f4 = (page(ls, n) for n in range(1, 5))
    t.check(f1["fonts"]["type3"] == 1 and "type3-font" in f1["unusualFeatures"], "Type 3 font", repr(f1["fonts"]))
    t.check(f1["fonts"]["noToUnicode"] == 1 and "no-tounicode" in f1["unusualFeatures"], "Type 3 without ToUnicode: no-tounicode")
    t.check(f2["text"]["privateUseRatio"] == 0.25 and f2["text"]["replacementCharRatio"] == 0.25 and f2["text"]["mathGlyphRatio"] == 0.25 and f2["text"]["controlCharRatio"] == 0.25, "PUA, U+FFFD, math and control characters", repr(f2["text"]))
    t.check(f2["fonts"]["noToUnicode"] == 0 and f2["fonts"]["nonEmbedded"] == 1, "a ToUnicode font, not embedded", repr(f2["fonts"]))
    t.check(f3["text"]["rotatedTextRatio"] == 1 and "rotated-content" in f3["unusualFeatures"], "rotated text: rotated-content", repr(f3["text"]))
    t.check(f4["text"]["verticalCharRatio"] == 1 and f4["text"]["rotatedTextRatio"] == 0 and f4["fonts"]["cid"] == 1, "vertical writing", repr(f4["text"]))
    t.check(all("unmappedGlyphRatio" in page(ls, n)["fonts"] for n in range(1, 5)), "unmappedGlyphRatio present where glyphs were drawn")

    # --- C image encodings -----------------------------------------------------------------------------
    items = page(lines(facts(paths["images"], "-level", "full")[1]), 1)["images"]["items"]
    got = [(i["filter"], i.get("bpc"), i["colorSpace"], i["copy"], i["softMask"], i["decode"]) for i in items]
    t.check(
        got
        == [
            ("dct", 8, "rgb", "jpeg", False, False),
            ("jpx", None, "none", "decode", False, False),
            ("jbig2", 1, "gray", "bilevel", False, False),
            ("ccitt", 1, "gray", "bilevel", False, False),
            ("flate", 8, "cmyk", "render", True, False),
            ("dct", 8, "gray", "decode", False, True),
        ],
        "image encodings: filter, bpc, colour space, copy class, masks",
        repr(got),
    )
    t.check([i["obj"] for i in items] == sorted(i["obj"] for i in items) and all(i["px"] == [300, 300] for i in items), "image object numbers and pixel sizes")

    # --- D vectors ---------------------------------------------------------------------------------------
    ls = lines(facts(paths["vectors"], "-level", "full")[1])
    v1, v2, v3, v4 = (page(ls, n)["vectors"] for n in range(1, 5))
    t.check(v1["hRules"] == 6 and v1["vRules"] == 6 and v1["ruleGridScore"] == 1, "ruled table: rules and grid score 1", repr(v1))
    t.check(v2["figureClusters"] >= 1 and v2["uncoveredRatio"] == 1 and v2["stroked"] == 21 and v2["coverage"] > 0.05, "vector chart: clusters, uncovered", repr(v2))
    t.check(v3["shadings"] == 1 and v3["patterns"] == 1 and v3["clips"] == 1, "shading, pattern and clip counted", repr(v3))
    t.check(v4["limitHit"] and v4["paths"] == 100000 and "drawing-limit" in page(ls, 4)["unusualFeatures"] and "limit-hit" in page(ls, 4)["parseWarnings"], "drawing-limit at 100000 paths", repr(v4))

    # --- E forms -----------------------------------------------------------------------------------------------
    for name, xfa in (("forms", "none"), ("forms_xfa_static", "static"), ("forms_xfa_dynamic", "dynamic")):
        ls = lines(facts(paths[name], "-level", "full")[1])
        dl = doc_line(ls)
        t.check(dl["acroForm"] == "present" and dl["xfa"] == xfa and dl["needAppearances"] is False and (("xfa-present" in dl["unusualFeatures"]) == (xfa != "none")), "%s: AcroForm, xfa %s" % (name, xfa), repr(dl))
        fm = page(ls, 1)["forms"]
        t.check(
            fm == {"fields": 11, "text": 3, "checkbox": 2, "radio": 2, "choice": 1, "signature": 2, "pushButton": 1, "filled": 5, "empty": 3, "valueAppearanceMismatch": 1, "signed": 1, "unsigned": 1},
            "%s: fields by type, filled/empty, value without appearance, signed/unsigned" % name,
            repr(fm),
        )
        t.check({"acroform-filled", "acroform-empty"} <= set(page(ls, 1)["unusualFeatures"]), "%s: acroform-filled and acroform-empty" % name)

    # --- F annotations ----------------------------------------------------------------------------------------------
    ls = lines(facts(paths["annots"], "-level", "full")[1])
    an = page(ls, 1)["annots"]
    expect = {"text", "freeText", "highlight", "underline", "strikeOut", "squiggly", "ink", "stamp", "square", "circle", "line", "polygon", "fileAttachment", "popup", "caret", "link", "other"}
    t.check(set(an["bySubtype"]) == expect and an["bySubtype"]["link"] == 2 and an["bySubtype"]["other"] == 1 and an["count"] == 18, "every annotation subtype counted (Redact as other)", repr(an))
    t.check(an["linkUri"] == 1 and an["linkInternal"] == 1 and an["visibleAppearance"] >= 2, "links URI vs internal; visible appearances", repr(an))
    t.check(
        set(page(ls, 1)["unusualFeatures"]) >= {"annotation-text", "annotation-freetext", "annotation-highlight", "annotation-other", "link-uri", "link-internal"},
        "annotation flags",
        repr(page(ls, 1)["unusualFeatures"]),
    )
    t.check(page(ls, 2)["annots"]["count"] == 0 and page(ls, 2)["unusualFeatures"] == [], "a page without annotations")

    # --- G optional content ---------------------------------------------------------------------------------------------
    ls = lines(facts(paths["ocg"], "-level", "full")[1])
    t.check(doc_line(ls)["ocgCount"] == 2 and doc_line(ls)["ocgOffByDefault"] == 1, "OCG count and off by default", repr(doc_line(ls)))
    l1, l2 = page(ls, 1), page(ls, 2)
    t.check(l1["layers"]["hiddenContent"] and l1["layers"]["visibleContent"] and {"ocg-hidden", "ocg-visible"} <= set(l1["unusualFeatures"]), "hidden and visible layer content", repr(l1["layers"]))
    t.check(not l2["layers"]["hiddenContent"] and not l2["layers"]["visibleContent"], "no layer content")
    t.check(l1["text"]["wordCount"] == 5, "hidden-layer text is not counted as text", repr(l1["text"]))

    # --- H document line ---------------------------------------------------------------------------------------------------
    raw = facts(paths["structure"], "-level", "full")[1]
    dl = doc_line(lines(raw))
    t.check(dl["tagged"] and dl["markedContent"] and dl["lang"] == "en-US" and dl["outlineCount"] == 3 and dl["pageLabels"], "tagged, marked, lang, outline, page labels", repr(dl))
    t.check(dl["javascript"] and dl["openAction"] and dl["embeddedFiles"] == 2 and {"javascript-present", "embedded-files"} <= set(dl["unusualFeatures"]), "JavaScript and embedded files", repr(dl))
    t.check(len(dl["producer"]) == 128 and "strings" in dl["truncated"] and dl["creator"] == "Writer 9", "producer capped at 128 code points, control characters dropped", repr((len(dl["producer"]), dl.get("creator"))))
    first = raw.split(b"\n")[0]
    t.check(b"<" not in first and b"&" not in first and b"\\u003c" in first and b"\\u0026" in first, "producer escaped: no < or & on the wire")
    bad_lang = doc_line(lines(facts(d.write("lang.pdf", fx.structure_pdf("(<script>x)")), "-level", "full")[1]))
    t.check("lang" not in bad_lang, "a /Lang that is not a language tag is absent")
    t.check(doc_line(lines(facts(paths["damaged"], "-level", "full")[1]))["xref"] == "repaired", "damaged xref: repaired")
    t.check(doc_line(lines(facts(paths["incremental"], "-level", "full")[1]))["incrementalUpdates"] == 1, "one incremental update")
    t.check(doc_line(lines(facts(paths["geometry"], "-level", "full")[1]))["incrementalUpdates"] == 0, "no incremental update")

    # --- I run health ------------------------------------------------------------------------------------------------------------
    ls = lines(facts(paths["warnings"])[1])
    w1 = page(ls, 1)["parseWarnings"]
    t.check({"bad-content-stream", "missing-font"} <= set(w1) and page(ls, 2)["parseWarnings"] == [], "parse warnings by stage, per page", repr(w1))
    rc, out, _ = facts(paths["geometry"], "-timing")
    ls = lines(out)
    t.check(all("factsMs" in l for l in ls if l["type"] == "page") and "peakRssKiB" in ls[-1] and "totalMs" in ls[-1], "-timing adds factsMs, totalMs, peakRssKiB")
    t.check(all("factsMs" not in l for l in lines(facts(paths["geometry"])[1])), "no timing by default")
    run = lines(facts(paths["geometry"])[1])[-1]
    t.check(run["pagesEmitted"] == 4 and run["pagesFailed"] == 0 and run["version"] and run["commit"], "run line", repr(run))

    # --- page locality -----------------------------------------------------------------------------------------------------------
    for level in ("basic", "full"):
        whole = lines(facts(paths["multipage"], "-level", level)[1])
        ok = True
        for n in range(1, 6):
            alone = lines(facts(paths["multipage"], "-level", level, "-f", str(n), "-l", str(n))[1])
            ok = ok and page(alone, n) == page(whole, n) and alone[0] == whole[0]
        t.check(ok, "page locality (%s): -f p -l p equals the whole run's line" % level)

    # --- pdftotext -page-facts-fd == pdfpagefacts; XHTML unchanged ------------------------------------------------------------------
    for name, path in sorted(paths.items()):
        for level in ("basic", "full"):
            ref = facts(path, "-level", level)[1]
            images = level == "full"
            rc, xhtml, got = pdftotext_facts(path, "-page-facts-level", level, images=images)
            t.check(got == ref, "%s %s: pdftotext -page-facts-fd == pdfpagefacts" % (name, level), "%r\n%r" % (got[:300], ref[:300]))
            t.check(xhtml == pdftotext_plain(path, images=images), "%s %s: XHTML unchanged by facts" % (name, level))
            if images:
                t.check(pdftotext_facts(path, "-page-facts-level", level, images=True, glyphs=True)[2] == ref, "%s: same facts with -images -glyphs" % name)
    t.check(facts(paths["multipage"], "-level", "full", "-f", "2", "-l", "4")[1] == pdftotext_facts(paths["multipage"], "-page-facts-level", "full", "-f", "2", "-l", "4")[2], "a page range: same lines")

    # --- pdftotext -images: scan and image attributes ---------------------------------------------------------------------------------
    x = pdftotext_plain(paths["scans"], images=True).decode()
    import re

    pages = re.findall(r"<page [^>]*>", x)
    scans = [re.search(r' scan="(\w+)" scanImage="(\d+)"', p) for p in pages]
    t.check([s.groups() if s else None for s in scans] == [("single", "1"), None, ("tiled", "1"), ("layered", "1"), ("layered", "1"), None], "-images: scan= and scanImage= on <page>", repr(pages))
    t.check(all(p.rstrip(">").endswith('"') and (' scan="' not in p or re.search(r' scanImage="\d+"$', p.rstrip(">"))) for p in pages), "-images: scan attributes are last on <page>")
    imgs = re.findall(r"<image [^>]*/>", x)
    t.check(re.search(r' filter="dct" bpc="8" cs="rgb" copy="jpeg" orient="0" overlaid="[01]"/>', imgs[0]) and ' ref="' in imgs[0], "-images: filter, bpc, cs, copy, then orient and overlaid last on <image>", imgs[0])
    t.check(any(' inline="1" copy="render"' in i for i in imgs) and any(' smask="1"' in i for i in imgs), "-images: inline and smask attributes", repr(imgs))
    t.check(re.findall(r' scan="', pdftotext_plain(paths["scans"]).decode()) == [] and b"filter=" not in pdftotext_plain(paths["scans"]), "without -images: no scan or image attributes")
    rc = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-images", "-scan-max-words", "300", paths["scans"], "-"], capture_output=True).stdout.decode()
    t.check(re.findall(r' scan="(\w+)"', rc)[:2] == ["single", "single"], "-scan-max-words raises the word threshold")

    # --- exit codes, stdin, limits, confinement ------------------------------------------------------------------------------------------
    rc, out, err = facts(os.path.join(d.path, "missing.pdf"))
    t.check(rc == 1 and lines(out)[0].get("error") == "open-failed" and b"missing" not in err and b"missing" not in out, "cannot open: exit 1, error line, no path echoed", repr((rc, out, err)))
    rc, out, err = facts(d.write("garbage.pdf", b"not a pdf at all"))
    t.check(rc == 1 and lines(out)[0]["type"] == "doc" and "error" in lines(out)[0], "not a PDF: exit 1 with an error doc line", repr((rc, out)))
    rc, out, _ = facts("-", stdin=open(paths["multipage"], "rb").read())
    t.check(rc == 0 and out == facts(paths["multipage"])[1], "stdin (-) gives the same lines")
    rc, out, err = facts(paths["geometry"], "-max-pages", "2")
    ls = lines(out)
    t.check(rc == 3 and ls[-1]["stop"] == "max-pages" and ls[-1]["pagesEmitted"] == 2, "-max-pages: exit 3 after the lines", repr((rc, ls[-1])))
    t.check(facts(paths["geometry"], "-confine", "off")[1] == facts(paths["geometry"], "-confine", "auto")[1], "-confine off and auto: same lines")
    rc, out, err = facts(paths["geometry"], "-confine", "require")
    t.check(rc in (0, 5), "-confine require: confined or exit 5", repr(rc))
    t.check(facts(paths["geometry"], "-confine", "nope")[0] == 99 and facts(paths["geometry"], "-f", "9")[0] == 99, "bad -confine or an empty range: exit 99")
    for bad in (["-page-facts-fd", "1"], ["-page-facts-fd", "2"], ["-page-facts-level", "full"]):
        p = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout"] + bad + [paths["geometry"], "-"], capture_output=True)
        t.check(p.returncode == 99, "pdftotext %s: usage error" % " ".join(bad), repr(p.returncode))
    p = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", "-page-facts-fd", "3", paths["geometry"], "-"], capture_output=True, pass_fds=())
    t.check(p.returncode == 99, "pdftotext -page-facts-fd without -bbox-layout: usage error")

    # --- council regressions: hostile structure costs, xref streams, clipped text, locality ----------------------------------------------
    import time

    def timed(path, *extra):
        start = time.monotonic()
        rc, out, err = facts(path, *extra)
        return rc, out, time.monotonic() - start

    # widgets sharing one large indirect /V: fetched once, not once per widget (was 43 s)
    rc, out, secs = timed(d.write("widgets.pdf", fx.shared_widget_value_pdf()), "-include", "forms")
    fm = page(lines(out), 1)["forms"] if rc == 0 else {}
    t.check(rc == 0 and secs < 10 and fm.get("text") == 2000 and fm.get("empty") == 2000, "2000 widgets sharing a large /V: fast, all counted", repr((rc, secs, fm)))
    check_schema(out, "shared widget value")
    # name-tree nodes sharing one large /Names array: read once (was 37 s), counted once
    rc, out, secs = timed(d.write("names.pdf", fx.shared_names_pdf()), "-include", "structure")
    dl = doc_line(lines(out)) if rc == 0 else {}
    t.check(rc == 0 and secs < 10 and dl.get("embeddedFiles") == 300000, "1000 name-tree nodes sharing one /Names: fast, counted once", repr((rc, secs, dl.get("embeddedFiles"))))
    # more than 10^6 entries: capped, flagged, never an overflowed count
    rc, out, secs = timed(d.write("many-names.pdf", fx.many_names_pdf()), "-include", "structure")
    dl = doc_line(lines(out)) if rc == 0 else {}
    t.check(dl.get("embeddedFiles") == 1000000 and "embeddedFiles" in dl.get("truncated", []) and "embedded-files" in dl.get("unusualFeatures", []), "name tree over 10^6 entries: capped at 10^6 and truncated", repr(dl))
    check_schema(out, "many names")
    # overlapping /Prev sections: the walk reads a bounded number of bytes (was 10 s)
    rc, out, secs = timed(d.write("nested-prev.pdf", fx.nested_prev_pdf()), "-include", "structure")
    t.check(rc == 0 and secs < 6, "nested-string /Prev chain: bounded walk", repr((rc, secs)))
    check_schema(out, "nested prev")
    # cross-reference streams: incremental updates are counted
    for name, want in (("xref_stream", 0), ("xref_stream_update", 1), ("table_then_stream", 1)):
        dl = doc_line(lines(facts(paths[name], "-include", "structure")[1]))
        t.check(dl.get("incrementalUpdates") == want and dl.get("xref") != "repaired", "%s: incrementalUpdates %d" % (name, want), repr(dl))
    t.check("incrementalUpdates" not in doc_line(lines(facts(paths["damaged"], "-include", "structure")[1])), "a rebuilt xref: no incrementalUpdates")
    # clipped-out text is invisible text: the word counts, text layer and scan test agree
    ls = lines(facts(paths["clipped_text"], "-level", "full")[1])
    c1, c2 = page(ls, 1), page(ls, 2)
    t.check(c1["text"]["visibleWordCount"] == 0 and c1["text"]["invisibleWordCount"] == 50 and c1["text"]["textLayer"] == "invisible" and c1["text"]["visibleCharCount"] == 0, "clipped text: invisible words and layer", repr(c1["text"]))
    t.check(c2["images"]["scan"] == "single" and c2["text"]["textLayer"] == "invisible" and c2["text"]["invisibleOverFullPageImage"], "clipped OCR layer over a full-page image: a scan", repr((c2["images"].get("scan"), c2["text"])))
    xc = pdftotext_plain(paths["clipped_text"], images=True).decode()
    t.check(re.findall(r' scan="(\w+)"', xc) == ["single"], "-images: the clipped OCR page is a scan", repr(re.findall(r"<page [^>]*>", xc)))
    # the run-wide image budget does not change a later page's facts (page locality)
    budget = d.write("budget.pdf", fx.image_budget_pdf())
    whole = lines(facts(budget, "-level", "full")[1])
    alone = lines(facts(budget, "-level", "full", "-f", "52", "-l", "52")[1])
    t.check(page(whole, 52) == page(alone, 52) and page(whole, 52)["images"]["scan"] == "single" and not page(whole, 52)["images"]["truncated"], "image budget spent by earlier pages: page 52 unchanged, still a scan", repr(page(whole, 52)["images"]))
    t.check(pdftotext_facts(budget, "-page-facts-level", "full", images=True)[2] == facts(budget, "-level", "full")[1], "image budget: pdftotext -images facts == pdfpagefacts")
    # small UserUnit keeps its value (schema minimum 0.0001)
    uu = d.write("userunit.pdf", fx.doc([{"content": fx.words(5), "extra": "/UserUnit 0.001"}, {"content": fx.words(5), "extra": "/UserUnit 0.00001"}]))
    out = facts(uu, "-level", "full")[1]
    check_schema(out, "small UserUnit")
    t.check([page(lines(out), n)["geometry"]["userUnit"] for n in (1, 2)] == [0.001, 0.0001], "UserUnit 0.001 and a clamped 0.00001", out.decode()[:400])
    # /Lang: control characters are not dropped into a valid-looking tag; UTF-16 ASCII is read
    t.check("lang" not in doc_line(lines(facts(d.write("lang2.pdf", fx.structure_pdf("(en-US\\nx)")), "-include", "structure")[1])), "a /Lang with a control character is absent")
    t.check(doc_line(lines(facts(d.write("lang3.pdf", fx.structure_pdf("<FEFF0065006E002D00470042>")), "-include", "structure")[1])).get("lang") == "en-GB", "a UTF-16 /Lang is read")
    # copy classes: /DecodeParms on DCT, /Decode on 1-bit, external data
    items = page(lines(facts(paths["copy_class"], "-level", "full")[1]), 1)["images"]["items"]
    t.check([i["copy"] for i in items] == ["decode", "decode", "render", "render"], "copy class: DCT with DecodeParms, inverted CCITT, external /F", repr([(i["filter"], i["copy"]) for i in items]))
    # XFA through an image codec is not decoded
    t.check(doc_line(lines(facts(d.write("xfa-codec.pdf", fx.forms_pdf("dynamic-codec")), "-include", "forms")[1]))["xfa"] == "static", "an XFA packet behind an image codec is not decoded")
    # a facts reader that closed its end: pdftotext finishes the XHTML and exits 4
    r, w = os.pipe()
    os.close(r)
    p = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-wordmeta", "-rules", "-compact-layout", "-page-facts-fd", str(w), paths["multipage"], "-"], capture_output=True, pass_fds=(w,))
    os.close(w)
    t.check(p.returncode == 4 and p.stdout == pdftotext_plain(paths["multipage"]), "closed facts reader: no SIGPIPE death, XHTML complete, exit 4", repr(p.returncode))
    # pdfpagefacts: confinement is required by default
    rc, out, err = facts(paths["geometry"])
    t.check(rc == 0 and facts(paths["geometry"], "-confine", "require")[1] == out, "default -confine is require (confined here)", repr((rc, err)))
    t.check(facts(paths["geometry"], "-timeout", "x")[0] == 99 and facts(paths["geometry"], "-timeout", "30")[0] == 0, "-timeout is checked")

    # --- encrypted files (poppler-test) ------------------------------------------------------------------------------------------------------
    td = os.path.join(args.testdata, "unittestcases") if args.testdata else ""
    if td and os.path.isdir(td):
        rc, out, _ = facts(os.path.join(td, "PasswordEncrypted.pdf"))
        ls = lines(out)
        t.check(rc == 2 and ls[0]["locked"] and ls[0]["encryption"] == "user-password-required" and "encrypted-user-password" in ls[0]["unusualFeatures"] and len(ls) == 2, "user password required: locked doc line, exit 2", repr((rc, ls)))
        check_schema(out, "locked")
        rc, out, _ = facts(os.path.join(td, "Gday garçon - owner.pdf"))
        ls = lines(out)
        t.check(rc == 0 and ls[0]["encryption"] == "empty-user-password" and "permissions" in ls[0] and "encrypted-empty-password" in ls[0]["unusualFeatures"], "owner password only: empty-user-password with permissions", repr(ls[0]))
        check_schema(out, "owner-only")
    else:
        print("SKIP: encrypted cases need --testdata (the poppler-test checkout)")

t.finish()
