"""Page facts on hostile input: what facts cost next to the pdftotext pass they ride on.

Each case is a small generated PDF built so that one facts path used to cost far more than the
text pass (an adversarial review measured each): its facts CPU (pdftotext -page-facts-fd at
level full, and pdfpagefacts) must stay within a few seconds of, and at most about three times,
the CPU of the plain `pdftotext -bbox-layout` pass. Also: pdftotext -timeout, the unchanged
stderr of pdftotext with facts, and (with --probe) what confine() denies.
"""

import argparse
import json
import os
import resource
import subprocess
import sys
import time
import zlib

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("--bindir", required=True)
parser.add_argument("--probe", default="")
args, _ = parser.parse_known_args()
BIN = args.bindir


class Doc:
    """Numbered PDF objects; reserve() hands out numbers, add() fills them."""

    def __init__(self):
        self.objs = {}
        self.n = 0

    def reserve(self):
        self.n += 1
        return self.n

    def add(self, body, num=None):
        num = num or self.reserve()
        self.objs[num] = body if isinstance(body, bytes) else body.encode("latin-1")
        return num

    def stream(self, data, extra="", compress=False):
        data = data if isinstance(data, bytes) else data.encode("latin-1")
        if compress:
            data = zlib.compress(data, 9)
            extra += " /Filter /FlateDecode"
        return self.add(b"<< " + extra.encode("latin-1") + b" /Length %d >>\nstream\n" % len(data) + data + b"\nendstream")

    def pages(self, contents, resources="", catalog_extra=""):
        """One page per content stream object number (repeats allowed); returns the PDF bytes."""
        pages = self.reserve()
        kids = [self.add("<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Contents %d 0 R /Resources << %s >> >>" % (pages, c, resources)) for c in contents]
        self.add("<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % k for k in kids), len(kids)), pages)
        root = self.add("<< /Type /Catalog /Pages %d 0 R %s >>" % (pages, catalog_extra))
        out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offs = {}
        for num in sorted(self.objs):
            offs[num] = len(out)
            out += b"%d 0 obj\n" % num + self.objs[num] + b"\nendobj\n"
        xref = len(out)
        out += b"xref\n0 %d\n0000000000 65535 f \n" % (self.n + 1)
        for i in range(1, self.n + 1):
            out += b"%010d 00000 n \n" % offs[i] if i in offs else b"0000000000 65535 f \n"
        out += b"trailer\n<< /Size %d /Root %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (self.n + 1, root, xref)
        return bytes(out)


# --- fixtures -------------------------------------------------------------------------------


def big_indexed():
    """An /Indexed colour space whose lookup string is 600 KB of hex."""
    return "[/Indexed /DeviceRGB 255 <" + "00" * 300000 + ">]"


def image_shared_space(draws=1000, distinct=1, icc=False):
    """<distinct> image XObjects naming one large indirect colour space (or an ICC profile whose
    /N is that large object), drawn <draws> times in all on one page."""
    d = Doc()
    big = d.add(big_indexed())
    if icc:
        prof = d.stream("x", "/N %d 0 R" % big)
        cs = "[/ICCBased %d 0 R]" % prof
    else:
        cs = "%d 0 R" % big
    xs = [d.stream("\xff", "/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace %s" % cs) for _ in range(distinct)]
    ops = "".join("q 100 0 0 100 %d %d cm /X%d Do Q\n" % (i % 5 * 100, i // 5 % 7 * 100, i % distinct) for i in range(draws))
    content = d.stream(ops, compress=True)
    return d.pages([content], resources="/XObject << %s >>" % " ".join("/X%d %d 0 R" % (i, x) for i, x in enumerate(xs)))


def ocmd_chain(depth=20, form=False, width=2):
    """An XObject whose /OC is an OCMD /VE of <depth> levels, each an /And naming the level
    below <width> times by reference (2^depth evaluations without memoisation)."""
    d = Doc()
    ocg = d.add("<< /Type /OCG /Name (a) >>")
    prev = d.add("[/And %d 0 R %d 0 R]" % (ocg, ocg))
    for _ in range(depth - 1):
        prev = d.add("[/And %s]" % " ".join(["%d 0 R" % prev] * width))
    ocmd = d.add("<< /Type /OCMD /VE %d 0 R >>" % prev)
    if form:
        x = d.stream("0 0 m 1 1 l S", "/Type /XObject /Subtype /Form /BBox [0 0 10 10] /OC %d 0 R" % ocmd)
    else:
        x = d.stream("\xff", "/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray /OC %d 0 R" % ocmd)
    content = d.stream("q 100 0 0 100 0 0 cm /X Do Q")
    return d.pages([content], resources="/XObject << /X %d 0 R >>" % x, catalog_extra="/OCProperties << /OCGs [%d 0 R] /D << >> >>" % ocg)


def ocmd_wide(operands=12000):
    """An image whose /OC is an OCMD /VE /And of <operands> shown groups: past the operand
    budget, so visible, with limit-hit."""
    d = Doc()
    ocg = d.add("<< /Type /OCG /Name (a) >>")
    ve = d.add("[/And %s]" % " ".join(["%d 0 R" % ocg] * operands))
    ocmd = d.add("<< /Type /OCMD /VE %d 0 R >>" % ve)
    x = d.stream("\xff", "/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray /OC %d 0 R" % ocmd)
    content = d.stream("q 300 0 0 300 0 0 cm /X Do Q")
    return d.pages([content], resources="/XObject << /X %d 0 R >>" % x, catalog_extra="/OCProperties << /OCGs [%d 0 R] /D << >> >>" % ocg)


def error_dense(pages=4, draws=50):
    """Pages drawing a form 50 times whose 1 MB content is unknown operators of high bytes: one
    poppler error per 121 bytes."""
    d = Doc()
    junk = (b"\x80" * 120 + b"\n") * 8000
    fx = d.stream(junk, "/Type /XObject /Subtype /Form /BBox [0 0 10 10]", compress=True)
    content = d.stream("/X Do\n" * draws, compress=True)
    return d.pages([content] * pages, resources="/XObject << /X %d 0 R >>" % fx)


def slim_graphics(pages=2000):
    """Pages whose drawing fills all 16x16 summary cells: the upper half diagonal strokes that
    chain into one figure, the lower half isolated dots."""
    w, h = 612.0 / 16, 792.0 / 16
    ops = []
    for r in range(16):
        for c in range(16):
            x0, y0 = c * w, r * h
            if r >= 8:
                ops.append("%.1f %.1f m %.1f %.1f l S" % (x0 + w / 2, y0 + h / 2, x0 + w / 2 + 0.5, y0 + h / 2 + 0.5))
            else:
                ops.append("%.1f %.1f m %.1f %.1f l S" % (x0 - 1, y0 + 1, x0 + w + 1, y0 + h - 1))
    d = Doc()
    content = d.stream("\n".join(ops), compress=True)
    return d.pages([content] * pages)


def rule_grid(pages=500, n=512):
    """Pages of n horizontal and n vertical hairline rules that all cross."""
    ops = ["0.1 w"]
    for i in range(n):
        ops.append("5 %.1f m 600 %.1f l S" % (10 + i * 1.5, 10 + i * 1.5))
    for i in range(n):
        ops.append("%.2f 5 m %.2f 785 l S" % (5 + i * 1.15, 5 + i * 1.15))
    d = Doc()
    content = d.stream("\n".join(ops), compress=True)
    return d.pages([content] * pages)


FONT = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"


def oc_marked(kind, sections, operands=9990):
    """A page that shows "shown" and draws "hidden" in <sections> optional-content sections,
    each naming one OCMD over an OFF group with <operands> operands:
      ve      BDC /OC naming an indirect OCMD whose /VE is a flat /Or of <operands> references
      ocgs    BDC /OC naming an indirect OCMD whose /OCGs array has <operands> entries (/AnyOn)
      direct  BDC /OC naming a direct OCMD in /Properties (/VE flat /Or): never cached, so the
              page's operand budget runs out and later sections count as visible, limit-hit
      xobject Do of a form whose /OC is the indirect /VE OCMD"""
    d = Doc()
    off = d.add("<< /Type /OCG /Name (off) >>")
    font = d.add(FONT)
    refs = " ".join(["%d 0 R" % off] * operands)
    res = "/Font << /F1 %d 0 R >>" % font
    if kind == "ocgs":
        oc = "%d 0 R" % d.add("<< /Type /OCMD /OCGs [%s] /P /AnyOn >>" % refs)
    elif kind == "direct":
        oc = "<< /Type /OCMD /VE [/Or %s] >>" % refs
    else:
        oc = "%d 0 R" % d.add("<< /Type /OCMD /VE %d 0 R >>" % d.add("[/Or %s]" % refs))
    if kind == "xobject":
        x = d.stream("BT /F1 12 Tf 0 0 Td (hidden) Tj ET", "/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources << %s >> /OC %s" % (res, oc))
        res += " /XObject << /X %d 0 R >>" % x
        body = "/X Do\n" * sections
    else:
        res += " /Properties << /P0 %s >>" % oc
        body = "/OC /P0 BDC BT /F1 12 Tf 72 600 Td (hidden) Tj ET EMC\n" * sections
    content = d.stream("BT /F1 12 Tf 72 700 Td (shown) Tj ET\n" + body, compress=True)
    return d.pages([content], resources=res, catalog_extra="/OCProperties << /OCGs [%d 0 R] /D << /OFF [%d 0 R] >> >>" % (off, off))


def oc_history(kind):
    """Two pages over shared /Properties, both naming the same OCMDs, so a whole-document run
    answers page 2 from what page 1 cached. Page 2 ends with "hidden" in a section over an OFF
    group:
      spent   22 distinct OCMDs /VE /And of 9,990 shown groups, on each page: page 2's operand
              budget runs out before its last section, so "hidden" counts as visible, limit-hit
      under   10 such OCMDs: within budget, so "hidden" stays hidden, no limit-hit
      shared  page 1's one section is /VE /And R (R an indirect /And of 9,998 shown groups);
              page 2's only one is /VE [/And R off]: past the 10,000-operand expression limit,
              so visible, limit-hit, even though page 1 evaluated R already"""
    d = Doc()
    on = d.add("<< /Type /OCG /Name (on) >>")
    off = d.add("<< /Type /OCG /Name (off) >>")
    font = d.add(FONT)
    props = {}
    if kind == "shared":
        r = d.add("[/And %s]" % " ".join(["%d 0 R" % on] * 9998))
        props["A"] = d.add("<< /Type /OCMD /VE [/And %d 0 R] >>" % r)
        props["H"] = d.add("<< /Type /OCMD /VE [/And %d 0 R %d 0 R] >>" % (r, off))
        early, late = "/OC /A BDC EMC\n", ""
    else:
        n = 22 if kind == "spent" else 10
        for k in range(n):
            props["M%d" % k] = d.add("<< /Type /OCMD /VE [/And %s] >>" % " ".join(["%d 0 R" % on] * 9990))
        props["H"] = d.add("<< /Type /OCMD /OCGs [%d 0 R] >>" % off)
        early = late = "".join("/OC /M%d BDC EMC\n" % k for k in range(n))
    res = "/Font << /F1 %d 0 R >> /Properties << %s >>" % (font, " ".join("/%s %d 0 R" % kv for kv in props.items()))
    c1 = d.stream(early + "BT /F1 12 Tf 72 700 Td (one) Tj ET\n")
    c2 = d.stream(late + "BT /F1 12 Tf 72 700 Td (two) Tj ET\n/OC /H BDC BT /F1 12 Tf 72 600 Td (hidden) Tj ET EMC\n")
    return d.pages([c1, c2], resources=res, catalog_extra="/OCProperties << /OCGs [%d 0 R %d 0 R] /D << /OFF [%d 0 R] >> >>" % (on, off, off))


def nested_marked(depth):
    """<depth> nested BMC sections around one word, between two others (a few KB compressed)."""
    d = Doc()
    font = d.add(FONT)
    ops = "BT /F1 12 Tf 72 700 Td (before) Tj ET\n" + "/A BMC\n" * depth + "BT /F1 12 Tf 72 600 Td (inside) Tj ET\n" + "EMC\n" * depth + "BT /F1 12 Tf 72 500 Td (after) Tj ET\n"
    return d.pages([d.stream(ops, compress=True)], resources="/Font << /F1 %d 0 R >>" % font)


def filter_names(pages=2, draws=20000, names=50000):
    """An image XObject whose /Filter (and /ColorSpace) is one indirect array of <names>
    /FlateDecode names, drawn <draws> times a page: the array was fetched and the 50,000-filter
    chain built again on every Do (past 120 s for 10 pages)."""
    d = Doc()
    big = d.add("[%s]" % " ".join(["/FlateDecode"] * names))
    x = d.stream("\xff", "/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace %d 0 R /Filter %d 0 R" % (big, big))
    content = d.stream("q 1 0 0 1 0 0 cm /X Do Q\n" * draws, compress=True)
    return d.pages([content] * pages, resources="/XObject << /X %d 0 R >>" % x)


def hex_chain(n):
    """A page whose content stream is ASCIIHex-encoded <n> times, with <n> filters."""
    d = Doc()
    font = d.add(FONT)
    data = b"BT /F1 12 Tf 72 700 Td (chained) Tj ET"
    for _ in range(n):
        data = data.hex().encode() + b">"
    content = d.stream(data, " ".join(["/Filter ["] + ["/ASCIIHexDecode"] * n + ["]"]))
    return d.pages([content], resources="/Font << /F1 %d 0 R >>" % font)


# --- measuring ------------------------------------------------------------------------------


def cpu_run(cmd, pass_fds=(), timeout=300):
    """(exit code, stdout, CPU seconds of the child)."""
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, pass_fds=pass_fds, timeout=timeout)
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    return p.returncode, p.stdout, (r1.ru_utime - r0.ru_utime) + (r1.ru_stime - r0.ru_stime)


def plain_cpu(*args):
    """Plain pdftotext on ARGS: (exit code or None after 60 s wall, stdout, CPU seconds). The
    bounds below are on CPU, not wall time: they guard against superlinear work, which a busy
    host's scheduling would otherwise hide or fake."""
    try:
        return cpu_run([os.path.join(BIN, "pdftotext"), "-q"] + list(args), timeout=60)
    except subprocess.TimeoutExpired:
        return None, b"(timed out)", 60.0


def pdftotext_facts(path, *extra):
    """pdftotext with the facts at level full on a descriptor: (exit code, facts, CPU s)."""
    out = os.path.join(os.path.dirname(path), "facts.jsonl")
    fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    try:
        rc, _, cpu = cpu_run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-page-facts-fd", str(fd), "-page-facts-level", "full"] + list(extra) + [path, os.devnull], pass_fds=(fd,))
    finally:
        os.close(fd)
    with open(out, "rb") as f:
        return rc, f.read(), cpu


def bounded(label, path, slack=3.0, factor=3.0):
    """Checks facts CPU against the plain pass: at most base + slack and factor x base (plus
    0.5 s for start-up noise on a fast base). Returns the pdfpagefacts lines."""
    _, _, base = cpu_run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", path, os.devnull])
    rc1, facts1, cpu1 = pdftotext_facts(path)
    rc2, facts2, cpu2 = cpu_run([os.path.join(BIN, "pdfpagefacts"), "-level", "full", "-timeout", "0", path])
    limit = min(base + slack, factor * base + 0.5)
    t.check(rc1 == 0 and rc2 == 0, "%s: both runs exit 0" % label, repr((rc1, rc2)))
    t.check(cpu1 <= limit and cpu2 <= limit, "%s: facts CPU within base + %.0f s and %.0fx base" % (label, slack, factor), "base %.2f s, pdftotext+facts %.2f s, pdfpagefacts %.2f s, limit %.2f s" % (base, cpu1, cpu2, limit))
    t.check(facts1 == facts2, "%s: pdftotext -page-facts-fd and pdfpagefacts agree" % label)
    print("  %s: base %.2f s, pdftotext+facts %.2f s, pdfpagefacts %.2f s" % (label, base, cpu1, cpu2))
    return [json.loads(l) for l in facts2.decode("ascii").splitlines()]


def page(ls, n):
    return next(l for l in ls if l["type"] == "page" and l["page"] == n)


with t.TempDir() as d:
    # 1. image dictionaries are read once per image XObject, and a large indirect colour space
    #    is parsed once per document (was 3.5 s against 0.02 s for 1000 draws)
    ls = bounded("1000 draws of an image with a 600 KB indirect /Indexed space", d.write("cs.pdf", image_shared_space()))
    im = page(ls, 1)["images"]
    t.check(im["imageCount"] == 1000 and im["items"] and all(i["colorSpace"] == "indexed" for i in im["items"]), "shared indirect colour space: every draw counted, indexed", repr({k: im[k] for k in ("imageCount", "items")})[:300])
    ls = bounded("1000 draws of an ICC image whose /N is a 600 KB object", d.write("icc.pdf", image_shared_space(icc=True)))
    t.check(page(ls, 1)["images"]["imageCount"] == 1000, "ICC /N by reference: every draw counted")
    ls = bounded("1000 distinct images sharing a 600 KB colour space", d.write("cs-distinct.pdf", image_shared_space(distinct=1000)))
    t.check(page(ls, 1)["images"]["imageCount"] == 1000, "distinct images sharing a colour space: every draw counted")

    # 2. optional-content expressions with shared indirect levels: linear, not 2^depth (depth 22
    #    took 17 s), in the facts paths and in poppler's own form drawing
    for form in (False, True):
        kind = "form" if form else "image"
        path = d.write("ocmd-%s.pdf" % kind, ocmd_chain(depth=22, form=form))
        ls = bounded("/OC /VE of 22 shared levels on an %s" % kind, path, factor=3.0)
        _, _, secs = plain_cpu(path, os.devnull)
        t.check(secs < 3, "/OC /VE of 22 shared levels on an %s: plain pdftotext is fast" % kind, "%.2f CPU-s" % secs)
        if not form:
            t.check(page(ls, 1)["images"]["imageCount"] == 1, "deep /VE of shown groups: the image is visible")
    ls = bounded("/OC /VE /And of 12000 operands", d.write("ocmd-wide.pdf", ocmd_wide()))
    p1 = page(ls, 1)
    t.check(p1["images"]["imageCount"] == 1 and "limit-hit" in p1["parseWarnings"], "/VE past the operand budget: visible, limit-hit", repr((p1["images"]["imageCount"], p1["parseWarnings"])))

    # 3. error-dense content: errors are classified from their format strings, never formatted
    #    for facts (was 11x the pass)
    path = d.write("errors.pdf", error_dense())
    ls = bounded("4 pages of 50 draws of 1 MB of unknown operators", path)
    t.check(all("bad-content-stream" in page(ls, n)["parseWarnings"] and "syntax-error" in page(ls, n)["parseWarnings"] for n in range(1, 5)), "error-dense pages: bad-content-stream and syntax-error", repr(page(ls, 1)["parseWarnings"]))
    # pdftotext prints poppler's messages exactly as without facts
    small = d.write("errors-small.pdf", t.simple_page_pdf(b"l\nl\n/Q0 Do\nBT /F1 12 Tf 72 700 Td (x) Tj ET\n\x80\x80 foo\n"))
    plain = subprocess.run([os.path.join(BIN, "pdftotext"), "-bbox-layout", small, os.devnull], capture_output=True).stderr
    r, w = os.pipe()
    withf = subprocess.run([os.path.join(BIN, "pdftotext"), "-bbox-layout", "-page-facts-fd", str(w), small, os.devnull], capture_output=True, pass_fds=(w,)).stderr
    os.close(w)
    os.close(r)
    t.check(plain and plain == withf, "pdftotext with facts: stderr unchanged", repr((plain[:200], withf[:200])))
    quiet = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", small, os.devnull], capture_output=True).stderr
    t.check(quiet == b"", "pdftotext -q: no messages")

    # 4. the figure-cluster merge is near-linear per page (was 10x the pass on 10,000 slim pages)
    ls = bounded("2000 pages of 256 drawing cells", d.write("slim.pdf", slim_graphics()))
    v = page(ls, 1)["vectors"]
    t.check(v["figureClusters"] == 129 and not v["limitHit"], "chained cells merge into one figure, isolated dots stay apart", repr((v["figureClusters"], v["limitHit"])))

    # 5. the rule-grid score is a sweep, not every row against every column (was 13x the pass)
    ls = bounded("500 pages of a 512 x 512 rule grid", d.write("grid.pdf", rule_grid()))
    v = page(ls, 1)["vectors"]
    t.check(v["ruleGridScore"] == 1 and v["hRules"] == 512 and v["vRules"] == 512, "a full rule grid scores 1", repr((v["ruleGridScore"], v["hRules"], v["vRules"])))
    ls = [json.loads(l) for l in cpu_run([os.path.join(BIN, "pdfpagefacts"), "-include", "vectors", d.write("grid-small.pdf", rule_grid(pages=1, n=3))])[1].decode().splitlines()]
    t.check(page(ls, 1)["vectors"]["ruleGridScore"] == 1, "3 x 3 crossing rules score 1", repr(page(ls, 1)["vectors"]))


    # 9. optional content: an OCMD named by many BDC /OC (or XObject /OC) entries is evaluated
    #    once, not once per section (4000 BDCs over a 9,990-operand /VE took 19 s, 20,000 over a
    #    9,990-entry /OCGs 51 s); a direct OCMD, which is not cached, is charged to a per-page
    #    operand budget, past which sections count as visible with limit-hit. Plain pdftotext too.
    for kind, sections in (("ve", 8000), ("ocgs", 5000), ("direct", 60000), ("xobject", 5000)):
        path = d.write("oc-%s.pdf" % kind, oc_marked(kind, sections))
        _, text, secs = plain_cpu(path, "-")
        text = text.decode()
        t.check(secs < 3, "%d sections over one %s OCMD: plain pdftotext within 3 CPU-s" % (sections, kind), "%.2f CPU-s" % secs)
        print("  oc %s x %d: pdftotext %.2f CPU-s" % (kind, sections, secs))
        if secs >= 3:
            continue
        ls = bounded("%d sections over one %s OCMD" % (sections, kind), path)
        warn = page(ls, 1)["parseWarnings"]
        if kind == "direct":
            t.check("shown" in text and "hidden" in text and "limit-hit" in warn, "direct OCMD past the page budget: visible, limit-hit", repr((text[:80], warn)))
        else:
            t.check("shown" in text and "hidden" not in text and "limit-hit" not in warn, "%s OCMD over an OFF group: hidden, no limit-hit" % kind, repr((text[:80], warn)))

    # 11. optional content: a page's visibility answers and limit-hit do not depend on what ran
    #     before it in the process. A cached OCMD is charged to the page's budget as if evaluated
    #     (22 sections over 9,991-operand OCMDs showed page 2's hidden layer cold but not after
    #     page 1), and /VE sub-expressions are not kept across evaluations.
    for kind, spent in (("spent", True), ("under", False), ("shared", True)):
        path = d.write("oc-history-%s.pdf" % kind, oc_history(kind))
        whole = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", path, "-"], capture_output=True, timeout=60).stdout.decode().split("\f")
        alone = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", "-f", "2", "-l", "2", path, "-"], capture_output=True, timeout=60).stdout.decode().split("\f")
        t.check(len(whole) > 2 and whole[1] == alone[0], "%s: pdftotext page 2 alone equals page 2 of the whole document" % kind, repr((whole[1:2], alone[:1])))
        facts = [[json.loads(l) for l in cpu_run([os.path.join(BIN, "pdfpagefacts"), "-level", "full", "-timeout", "0"] + extra + [path])[1].decode("ascii").splitlines()] for extra in ([], ["-f", "2", "-l", "2"])]
        p2 = [page(ls, 2) for ls in facts]
        t.check(p2[0] == p2[1], "%s: pdfpagefacts page 2 alone equals page 2 of the whole document" % kind, repr(p2)[:600])
        shown = "hidden" in alone[0] and "limit-hit" in p2[1]["parseWarnings"]
        hidden = "hidden" not in alone[0] and "limit-hit" not in p2[1]["parseWarnings"]
        t.check(shown if spent else hidden, "%s: page 2's last section %s" % (kind, "visible, limit-hit" if spent else "hidden, no limit-hit"), repr((alone[0][:80], p2[1]["parseWarnings"])))

    # 10. marked content: visibility is O(1) per BMC/BDC/EMC, not a walk of the stack (200,000
    #     nested BMC in 2.8 KB took 91 s), and nesting past 4096 is not tracked, with limit-hit
    path = d.write("nested-mc.pdf", nested_marked(100000))
    _, text, secs = plain_cpu(path, "-")
    text = text.decode()
    t.check(secs < 3 and text.split() == ["before", "inside", "after"], "100,000 nested BMC: plain pdftotext within 3 CPU-s, all text", "%.2f CPU-s %r" % (secs, text[:80]))
    print("  100,000 nested BMC: pdftotext %.2f CPU-s" % secs)
    if secs < 3:
        ls = bounded("100,000 nested BMC", path)
        t.check("limit-hit" in page(ls, 1)["parseWarnings"], "nesting past the cap: limit-hit", repr(page(ls, 1)["parseWarnings"]))
    ls = bounded("4,000 nested BMC", d.write("nested-mc-ok.pdf", nested_marked(4000)))
    t.check("limit-hit" not in page(ls, 1)["parseWarnings"], "nesting within the cap: no limit-hit", repr(page(ls, 1)["parseWarnings"]))


    # 11. stream filters: a /Filter array (or /DecodeParms) given by reference is fetched once
    #     per document, not per stream, and a chain of more than 16 filters is refused (an
    #     image whose /Filter names a 50,000-entry array, drawn 20,000 times a page, ran past
    #     120 s in plain pdftotext -bbox-layout)
    path = d.write("filter-names.pdf", filter_names())
    rc, _, secs = plain_cpu("-bbox-layout", path, os.devnull)
    t.check(rc == 0 and secs < 3, "40,000 Do of an image with a 50,000-name /Filter by reference: plain pdftotext within 3 CPU-s", "%.2f CPU-s, rc %r" % (secs, rc))
    print("  50,000-name /Filter x 40,000 Do: pdftotext %.2f CPU-s" % secs)
    if secs < 3:
        ls = bounded("50,000-name /Filter x 40,000 Do", path)
        t.check("limit-hit" in page(ls, 1)["parseWarnings"], "a refused filter chain: limit-hit", repr(page(ls, 1)["parseWarnings"]))
    for n, shown in ((16, True), (17, False)):
        text = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", d.write("hex%d.pdf" % n, hex_chain(n)), "-"], capture_output=True, timeout=60).stdout.decode()
        t.check(("chained" in text) == shown, "%d ASCIIHex filters: %s" % (n, "decoded" if shown else "refused"), repr(text[:60]))

    # 8. pdftotext -timeout: exit code 6, whole facts lines, no run line
    slow = d.write("slow.pdf", error_dense(pages=40))
    start = time.monotonic()
    rc, facts, _ = pdftotext_facts(slow, "-timeout", "1")
    secs = time.monotonic() - start
    fl = facts.decode("ascii").splitlines()
    t.check(rc == 6 and secs < 6, "-timeout 1: exit 6 within seconds", repr((rc, secs)))
    t.check(facts.endswith(b"\n") and all(json.loads(l)["type"] != "run" for l in fl) and json.loads(fl[0])["type"] == "doc", "-timeout: whole facts lines, no run line", repr(fl[-1:]))
    rc, _, _ = pdftotext_facts(slow, "-timeout", "0", "-timeout-per-page", "1", "-timeout-max", "1")
    t.check(rc == 6, "-timeout-max caps the per-page limit", repr(rc))
    rc, _, _ = pdftotext_facts(d.write("cs2.pdf", image_shared_space(draws=10)), "-timeout", "1", "-timeout-per-page", "1")
    t.check(rc == 0, "a fast run under -timeout: exit 0", repr(rc))
    for bad in (["-timeout", "-1"], ["-timeout", "1.5"], ["-timeout-per-page", "x"], ["-timeout", "86401"]):
        p = subprocess.run([os.path.join(BIN, "pdftotext"), "-q"] + bad + [slow, os.devnull], capture_output=True)
        t.check(p.returncode == 99, "pdftotext %s: usage error" % " ".join(bad), repr(p.returncode))

    # 6, 7. confinement: what confine() must deny (pdfpagefacts and pdfregions apply it)
    if args.probe:
        pd = os.path.join(d.path, "probe")
        os.mkdir(pd)
        with open(os.path.join(pd, "f"), "w") as f:
            f.write("data")
        out = subprocess.run([args.probe, pd], capture_output=True, timeout=60).stdout.decode()
        res = dict(l.split("=", 1) for l in out.splitlines() if "=" in l and not l.startswith("landlock="))
        head = out.splitlines()[0] if out else ""
        if "seccomp=1" in head:
            allowed = {"kill-self", "write-open-fd", "read", "prlimit-self", "setpriority-self", "ioctl-fionread", "ioctl-fioclex", "thread"}
            t.check(all(res.get(k) == "allowed" for k in allowed), "confined: this process, open descriptors, reads, threads and allow-listed ioctls still work", out)
            t.check(res.get("clone3", "errno-38") == "errno-38", "confined: clone3 is unavailable (ENOSYS)", repr(res.get("clone3")))
            bad = {k: v for k, v in res.items() if k not in allowed and k != "clone3" and v != "denied"}
            t.check(not bad and len(res) > 50, "confined: modes, owners, times, sizes, xattrs, names, signals and limits/priority/scheduling of others, other ioctls, locks, sessions, namespaces, keyrings, new processes, IPC and memfds denied", repr(bad))
            with open(os.path.join(pd, "f"), "rb") as f:
                t.check(f.read() == b"xata", "confined: the file kept its size (only the open descriptor wrote)")
            t.check(sorted(os.listdir(pd)) == ["f"], "confined: nothing created or renamed", repr(os.listdir(pd)))
        else:
            print("SKIP: kernel without seccomp; confinement probe not checked (%s)" % head)

t.finish()
