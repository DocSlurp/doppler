"""pdfpagecount counts leaf pages from the page tree and ignores every claim.

The count drives billing and limits, so documents must not be able to forge
it through Info, XMP, /Count or text, and hostile trees must fail with a
distinct exit code and an empty stdout instead of a wrong number.
"""

import hashlib
import os
import re
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402

import subprocess  # noqa: E402

OK, OPEN_ERROR, ENCRYPTED, LIMIT, MALFORMED, USAGE = 0, 1, 2, 3, 4, 99

# Planted in document strings and names; must never reach stderr.
MARKER = "ZZDOCMARKER"

utils = t.Utils.from_args()


def run(util, *args, timeout=60):
    cmd = [os.path.join(utils.bindir, util)] + [str(a) for a in args]
    proc = subprocess.run(cmd, capture_output=True, timeout=timeout)
    return proc.returncode, proc.stdout, proc.stderr


class Objects:
    """Allocates numbered indirect objects; object 1 is the catalog."""

    def __init__(self):
        self.bodies = [None]

    def alloc(self):
        self.bodies.append(None)
        return len(self.bodies)

    def set(self, num, body):
        self.bodies[num - 1] = body

    def add(self, body):
        num = self.alloc()
        self.set(num, body)
        return num

    def pdf(self, catalog_extra="", trailer_extra="", pages=2):
        self.set(1, "<< /Type /Catalog /Pages %d 0 R %s >>" % (pages, catalog_extra))
        return t.build_pdf(self.bodies, trailer_extra=trailer_extra)


def page_body(parent, extra="", typed=True):
    return "<< %s/Parent %d 0 R /MediaBox [0 0 612 792] %s >>" % ("/Type /Page " if typed else "", parent, extra)


def tree_pdf(n, fanout=None, count=None, catalog_extra="", trailer_extra="", page_extra=""):
    """n pages under one root (or a two-level tree when fanout is set)."""
    objs = Objects()
    root = objs.alloc()
    kids = []
    if fanout:
        for start in range(0, n, fanout):
            node = objs.alloc()
            leaves = [objs.add(page_body(node, page_extra)) for _ in range(min(fanout, n - start))]
            objs.set(node, "<< /Type /Pages /Parent %d 0 R /Kids [%s] /Count %d >>" % (root, " ".join("%d 0 R" % k for k in leaves), len(leaves)))
            kids.append(node)
    else:
        kids = [objs.add(page_body(root, page_extra)) for _ in range(n)]
    objs.set(root, "<< /Type /Pages /Kids [%s] /Count %s >>" % (" ".join("%d 0 R" % k for k in kids), n if count is None else count))
    return objs.pdf(catalog_extra=catalog_extra, trailer_extra=trailer_extra)


def expect(d, name, data, code, out=None, args=(), what=None):
    path = d.write(name, data)
    rc, stdout, stderr = run("pdfpagecount", *args, path)
    label = what or name
    t.check(rc == code, "%s: exit %d" % (label, code), "got %d, stdout %r, stderr %r" % (rc, stdout, stderr))
    if code == OK and out is not None:
        t.check(stdout == out, "%s: stdout is exactly %r" % (label, out), repr(stdout))
    if code != OK:
        t.check(stdout == b"", "%s: nothing on stdout" % label, repr(stdout))
    t.check(MARKER.encode() not in stderr, "%s: stderr does not echo document strings" % label, repr(stderr))
    return path


def poppler_rendered_pages(path):
    """Pages pdftotext emits (one form feed per page)."""
    rc, stdout, _ = run("pdftotext", path, "-")
    return stdout.count(b"\f")


# --- RC4 encryption (standard security handler, R2) -------------------------

PAD = bytes.fromhex("28bf4e5e4e758a4164004e56fffa01082e2e00b6d0683e802f0ca9fe6453697a")


def rc4(key, data):
    s = list(range(256))
    j = 0
    for i in range(256):
        j = (j + s[i] + key[i % len(key)]) % 256
        s[i], s[j] = s[j], s[i]
    i = j = 0
    out = bytearray()
    for c in data:
        i = (i + 1) % 256
        j = (j + s[i]) % 256
        s[i], s[j] = s[j], s[i]
        out.append(c ^ s[(s[i] + s[j]) % 256])
    return bytes(out)


def encrypted_pdf(user_pw, owner_pw=b"owner", n=4):
    """n pages without strings or streams, so only /O and /U need computing."""
    padded = lambda pw: (pw + PAD)[:32]  # noqa: E731
    file_id = bytes(range(16))
    perms = -4
    o_value = rc4(hashlib.md5(padded(owner_pw)).digest()[:5], padded(user_pw))
    key = hashlib.md5(padded(user_pw) + o_value + (perms & 0xFFFFFFFF).to_bytes(4, "little") + file_id).digest()[:5]
    u_value = rc4(key, PAD)
    objs = Objects()
    root = objs.alloc()
    kids = [objs.add(page_body(root)) for _ in range(n)]
    objs.set(root, "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % k for k in kids), n))
    enc = objs.add("<< /Filter /Standard /V 1 /R 2 /O <%s> /U <%s> /P %d >>" % (o_value.hex(), u_value.hex(), perms))
    return objs.pdf(trailer_extra="/Encrypt %d 0 R /ID [<%s> <%s>]" % (enc, file_id.hex(), file_id.hex()))


with t.TempDir() as d:
    # --- normal documents -------------------------------------------------
    for n in (1, 3):
        path = expect(d, "flat-%d.pdf" % n, tree_pdf(n), OK, b"%d\n" % n)
        t.check(poppler_rendered_pages(path) == n, "flat-%d: matches pages pdftotext renders" % n)
    path = expect(d, "nested-250.pdf", tree_pdf(250, fanout=10), OK, b"250\n")
    t.check(poppler_rendered_pages(path) == 250, "nested-250: matches pages pdftotext renders")
    rc, stdout, _ = run("pdfinfo", path)
    t.check(b"Pages:           250" in stdout, "nested-250: pdfinfo agrees on an honest file")

    # stdin
    rc = subprocess.run([os.path.join(utils.bindir, "pdfpagecount"), "-"], input=tree_pdf(7), capture_output=True, timeout=60)
    t.check(rc.returncode == OK and rc.stdout == b"7\n", "stdin: reads '-'", repr(rc.stdout))

    # --- forged claims ------------------------------------------------------
    for label, title in (
        ("title-newline", "(Report\\nPages: 1 %s)" % MARKER),
        ("title-u2028", "<FEFF%s2028%s>" % ("".join("%04X" % ord(c) for c in "Report"), "".join("%04X" % ord(c) for c in "Pages: 1 " + MARKER))),
    ):
        objs = Objects()
        root = objs.alloc()
        kids = [objs.add(page_body(root)) for _ in range(50)]
        objs.set(root, "<< /Type /Pages /Kids [%s] /Count 50 >>" % " ".join("%d 0 R" % k for k in kids))
        info = objs.add("<< /Title %s /Subject %s /Pages 1 >>" % (title, title))
        path = expect(d, label + ".pdf", objs.pdf(trailer_extra="/Info %d 0 R" % info), OK, b"50\n", what=label)

    xmp = '<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmpTPg="http://ns.adobe.com/xap/1.0/t/pg/"><xmpTPg:NPages>1</xmpTPg:NPages></rdf:Description></rdf:RDF></x:xmpmeta>'
    objs = Objects()
    root = objs.alloc()
    kids = [objs.add(page_body(root)) for _ in range(50)]
    objs.set(root, "<< /Type /Pages /Kids [%s] /Count 50 >>" % " ".join("%d 0 R" % k for k in kids))
    meta = objs.add(t.stream("/Type /Metadata /Subtype /XML", xmp))
    expect(d, "xmp-1.pdf", objs.pdf(catalog_extra="/Metadata %d 0 R" % meta), OK, b"50\n")

    expect(d, "count-1-kids-50.pdf", tree_pdf(50, count=1), OK, b"50\n")
    expect(d, "count-1-kids-50.pdf", tree_pdf(50, count=1), OK, b"50\n1\n", args=("-claimed",), what="count-1-kids-50 -claimed")
    expect(d, "count-1-kids-50.pdf", tree_pdf(50, count=1), OK, b"50\n1\n", args=("--claimed",), what="count-1-kids-50 --claimed")
    expect(d, "count-1000-kids-3.pdf", tree_pdf(3, count=1000), OK, b"3\n")
    expect(d, "count-1000-kids-3.pdf", tree_pdf(3, count=1000), OK, b"3\n1000\n", args=("-claimed",), what="count-1000-kids-3 -claimed")
    # An over-claim below the object count: pdftotext still emits only the
    # real pages (pdftoppm would emit blank images up to /Count).
    objs = Objects()
    root = objs.alloc()
    kids = [objs.add(page_body(root)) for _ in range(3)]
    objs.set(root, "<< /Type /Pages /Kids [%s] /Count 5 >>" % " ".join("%d 0 R" % k for k in kids))
    for _ in range(5):
        objs.add("null")
    path = expect(d, "count-5-kids-3.pdf", objs.pdf(), OK, b"3\n")
    t.check(poppler_rendered_pages(path) == 3, "count-5-kids-3: pdftotext emits the 3 real pages")
    expect(d, "count-missing.pdf", tree_pdf(2, count="/Foo"), OK, b"2\nnone\n", args=("-claimed",), what="count-missing -claimed")

    # --- hostile trees ------------------------------------------------------
    # Cycle through the root.
    objs = Objects()
    root, node = objs.alloc(), objs.alloc()
    leaf = objs.add(page_body(node))
    objs.set(node, "<< /Type /Pages /Parent %d 0 R /Kids [%d 0 R %d 0 R] /Count 1 >>" % (root, leaf, root))
    objs.set(root, "<< /Type /Pages /Kids [%d 0 R] /Count 2 >>" % node)
    expect(d, "cycle-root.pdf", objs.pdf(), MALFORMED)
    # Self-referencing intermediate node.
    objs = Objects()
    root, node = objs.alloc(), objs.alloc()
    objs.set(node, "<< /Type /Pages /Kids [%d 0 R] /Count 1 >>" % node)
    objs.set(root, "<< /Type /Pages /Kids [%d 0 R] /Count 1 >>" % node)
    expect(d, "cycle-self.pdf", objs.pdf(), MALFORMED)

    # Deep chain: 300 nested intermediate nodes.
    def chain_pdf(depth):
        objs = Objects()
        root = objs.alloc()
        nodes = [root] + [objs.alloc() for _ in range(depth - 1)]
        leaf = objs.add(page_body(nodes[-1]))
        for i, num in enumerate(nodes):
            kid = nodes[i + 1] if i + 1 < len(nodes) else leaf
            parent = "/Parent %d 0 R " % nodes[i - 1] if i else ""
            objs.set(num, "<< /Type /Pages %s/Kids [%d 0 R] /Count 1 >>" % (parent, kid))
        return objs.pdf()

    expect(d, "deep-300.pdf", chain_pdf(300), LIMIT)
    expect(d, "deep-300.pdf", chain_pdf(300), OK, b"1\n", args=("-max-depth", 300), what="deep-300 -max-depth 300")
    expect(d, "deep-300.pdf", chain_pdf(300), LIMIT, args=("-max-depth", 299), what="deep-300 -max-depth 299")

    # Page cap.
    expect(d, "pages-120.pdf", tree_pdf(120, fanout=7), LIMIT, args=("-max-pages", 100), what="pages-120 -max-pages 100")
    expect(d, "pages-120.pdf", tree_pdf(120, fanout=7), OK, b"120\n", args=("-max-pages", 120), what="pages-120 -max-pages 120")
    expect(d, "pages-120.pdf", tree_pdf(120, fanout=7), LIMIT, args=("--max-pages", 119), what="pages-120 --max-pages 119")

    # Shared subtrees count every reference (Poppler renders each one).
    objs = Objects()
    root = objs.alloc()
    leaf = objs.add(page_body(root))
    objs.set(root, "<< /Type /Pages /Kids [%s] /Count 3 >>" % " ".join(["%d 0 R" % leaf] * 3))
    path = expect(d, "dup-leaf-3.pdf", objs.pdf(), OK, b"3\n")
    t.check(poppler_rendered_pages(path) == 3, "dup-leaf-3: pdftotext renders the shared page 3 times")

    # Amplification: 1000 refs to a node with 1000 refs to one page is 10^6
    # pages, past the default cap of 100000.
    objs = Objects()
    root, inner = objs.alloc(), objs.alloc()
    leaf = objs.add(page_body(inner))
    objs.set(inner, "<< /Type /Pages /Kids [%s] /Count 1000 >>" % " ".join(["%d 0 R" % leaf] * 1000))
    objs.set(root, "<< /Type /Pages /Kids [%s] /Count 1 >>" % " ".join(["%d 0 R" % inner] * 1000))
    expect(d, "amplify-1e6.pdf", objs.pdf(), LIMIT)

    # Exponential DAG: 64 levels, each node referencing the next twice.
    # Memoization keeps this linear; the count overflows the cap.
    objs = Objects()
    root = objs.alloc()
    nodes = [root] + [objs.alloc() for _ in range(63)]
    leaf = objs.add(page_body(nodes[-1]))
    for i, num in enumerate(nodes):
        kid = nodes[i + 1] if i + 1 < len(nodes) else leaf
        objs.set(num, "<< /Type /Pages /Kids [%d 0 R %d 0 R] /Count 2 >>" % (kid, kid))
    expect(d, "dag-2pow64.pdf", objs.pdf(), LIMIT)
    expect(d, "dag-2pow64.pdf", objs.pdf(), LIMIT, args=("-max-pages", 2**31 - 1), what="dag-2pow64 -max-pages 2^31-1")

    # --- /Type handling (Catalog::cacheSubTree rules) -----------------------
    objs = Objects()
    root, node = objs.alloc(), objs.alloc()
    untyped_leaf = objs.add(page_body(node, typed=False))
    typed_leaf = objs.add(page_body(node))
    objs.set(node, "<< /Parent %d 0 R /Kids [%d 0 R %d 0 R] /Count 2 >>" % (root, untyped_leaf, typed_leaf))
    other = objs.add(page_body(root, typed=False))
    objs.set(root, "<< /Kids [%d 0 R %d 0 R] /Count 3 >>" % (node, other))
    path = expect(d, "missing-type.pdf", objs.pdf(), OK, b"3\n")
    t.check(poppler_rendered_pages(path) == 3, "missing-type: matches pages pdftotext renders")

    # A /Type /Page kid is a page even if it also has /Kids.
    objs = Objects()
    root = objs.alloc()
    leaf = objs.add(page_body(root, "/Kids []"))
    objs.set(root, "<< /Type /Pages /Kids [%d 0 R] /Count 1 >>" % leaf)
    expect(d, "page-with-kids.pdf", objs.pdf(), OK, b"1\n")

    # A wrongly typed name does not make an intermediate node a page.
    objs = Objects()
    root, node = objs.alloc(), objs.alloc()
    leaves = [objs.add(page_body(node)) for _ in range(2)]
    objs.set(node, "<< /Type /%s /Kids [%s] /Count 2 >>" % (MARKER, " ".join("%d 0 R" % k for k in leaves)))
    objs.set(root, "<< /Type /Pages /Kids [%d 0 R] /Count 2 >>" % node)
    expect(d, "odd-type.pdf", objs.pdf(), OK, b"2\n")

    # Kids resolving to non-dictionaries are skipped, as Poppler does.
    objs = Objects()
    root = objs.alloc()
    leaf = objs.add(page_body(root))
    junk = objs.add("(%s)" % MARKER)
    objs.set(root, "<< /Type /Pages /Kids [%d 0 R %d 0 R 999 0 R] /Count 3 >>" % (leaf, junk))
    path = expect(d, "non-dict-kids.pdf", objs.pdf(), OK, b"1\n")
    t.check(poppler_rendered_pages(path) == 1, "non-dict-kids: matches pages pdftotext renders")

    # A lone page as the root, without /Count.
    objs = Objects()
    objs.set(objs.alloc(), "<< /Type /Page /MediaBox [0 0 612 792] >>")
    expect(d, "root-is-page.pdf", objs.pdf(), OK, b"1\n")

    # Structural errors that stop Poppler's page list.
    objs = Objects()
    root = objs.alloc()
    objs.set(root, "<< /Type /Pages /Kids [<< /Type /Page /MediaBox [0 0 612 792] >>] /Count 1 >>")
    expect(d, "direct-kid.pdf", objs.pdf(), MALFORMED)
    objs = Objects()
    objs.set(objs.alloc(), "<< /Type /Pages /Kids 5 /Count 1 >>")
    expect(d, "kids-not-array.pdf", objs.pdf(), MALFORMED)
    expect(d, "bad-contents.pdf", tree_pdf(2, page_extra="/Contents 7"), MALFORMED)
    objs = Objects()
    objs.add("<< /Foo 1 >>")
    objs.bodies[0] = "<< /Type /Catalog >>"
    expect(d, "no-pages.pdf", t.build_pdf(objs.bodies), MALFORMED)

    # --- damaged files ------------------------------------------------------
    good = tree_pdf(50, fanout=8)
    xref_at = good.index(b"\nxref\n") + 1
    # Cut inside the xref table: no trailer survives, so Poppler cannot open it.
    path = expect(d, "truncated-in-xref.pdf", good[: xref_at + 40], OPEN_ERROR)
    t.check(poppler_rendered_pages(path) == 0, "truncated-in-xref: pdftotext cannot open it either")
    # Cut inside the page tree, then a trailer from an appended update:
    # reconstruction recovers the objects that survive.
    half = good[: good.index(b"\n30 0 obj")] + b"\ntrailer\n<< /Root 1 0 R >>\n"
    path = expect(d, "truncated-mid-tree.pdf", half, OK, b"24\n")
    t.check(poppler_rendered_pages(path) == 24, "truncated-mid-tree: count equals pages pdftotext renders")
    expect(d, "truncated-header.pdf", good[:20], OPEN_ERROR)
    broken = re.sub(rb"(\d{10}) 00000 n", lambda m: b"%010d 00000 n" % (int(m.group(1)) + 7), good)
    t.check(broken != good, "broken-xref-offsets: offsets were changed")
    expect(d, "broken-xref-offsets.pdf", broken, OK, b"50\n")
    bad_start = good.replace(b"startxref\n%d\n" % xref_at, b"startxref\n99999999\n")
    t.check(bad_start != good, "broken-startxref: startxref was changed")
    expect(d, "broken-startxref.pdf", bad_start, OK, b"50\n")
    expect(d, "not-a-pdf.pdf", b"hello " + MARKER.encode() + b"\n", OPEN_ERROR)
    rc, stdout, stderr = run("pdfpagecount", os.path.join(d.path, "missing.pdf"))
    t.check(rc == OPEN_ERROR and stdout == b"", "missing file: exit 1", repr((rc, stdout, stderr)))

    # --- encryption ---------------------------------------------------------
    path = expect(d, "encrypted-empty-user.pdf", encrypted_pdf(b""), OK, b"4\n")
    t.check(poppler_rendered_pages(path) == 4, "encrypted-empty-user: Poppler opens it too")
    expect(d, "encrypted-user-pw.pdf", encrypted_pdf(b"secret"), ENCRYPTED)

    # --- command line -------------------------------------------------------
    for args in ((), ("a.pdf", "b.pdf"), ("-max-pages", -1, "x.pdf"), ("-max-depth", 0, "x.pdf"), ("-nope", "x.pdf")):
        rc, stdout, _ = run("pdfpagecount", *args)
        t.check(rc == USAGE and stdout == b"", "usage %r: exit 99" % (args,), repr((rc, stdout)))
    rc, _, _ = run("pdfpagecount", "-h")
    t.check(rc == OK, "-h: exit 0")

t.finish()
