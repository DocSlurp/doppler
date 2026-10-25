"""Helpers for DocSlurp regression tests.

Tests run with ``python3 -I``, so import this module by path:

    import os, sys
    sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR",
                                      os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    import docslurp_testlib as t

Then build a PDF in memory, write it to a temp dir, and run a util against it:

    pdf = t.simple_page_pdf(b"BT /F1 12 Tf 72 700 Td (Hello) Tj ET")
    with t.TempDir() as d:
        path = d.write("hello.pdf", pdf)
        out = t.Utils.from_args().run("pdftotext", path, "-")
        t.check("Hello" in out, "text extracted", out)
    t.finish()
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import zlib


def _b(x):
    return x if isinstance(x, bytes) else x.encode("latin-1")


def stream(dict_body, data, compress=False):
    """Return an indirect-object body for a stream with a correct /Length."""
    data = _b(data)
    dict_body = _b(dict_body)
    if compress:
        data = zlib.compress(data)
        dict_body += b" /Filter /FlateDecode"
    return b"<< " + dict_body + b" /Length %d >>\nstream\n" % len(data) + data + b"\nendstream"


def build_pdf(objects, root=1, trailer_extra=""):
    """Build a complete PDF from object bodies numbered 1..N, with a valid xref table."""
    out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for num, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % num + _b(body) + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root %d 0 R " % (len(objects) + 1, root) + _b(trailer_extra) + b" >>\n"
    out += b"startxref\n%d\n%%%%EOF\n" % xref
    return bytes(out)


def simple_page_pdf(content, resources="<< /Font << /F1 4 0 R >> >>", extra_objects=(), page_extra="", catalog_extra="", mediabox=(0, 0, 612, 792)):
    """One-page PDF. Objects: 1 Catalog, 2 Pages, 3 Page, 4 Helvetica, 5 contents, 6.. extra_objects."""
    box = " ".join(str(v) for v in mediabox)
    objects = [
        "<< /Type /Catalog /Pages 2 0 R %s >>" % catalog_extra,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [%s] /Resources %s /Contents 5 0 R %s >>" % (box, resources, page_extra),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        stream("", content),
    ] + list(extra_objects)
    return build_pdf(objects)


class TempDir:
    """Scratch directory under $TMPDIR (never hardcoded /tmp), removed on exit."""

    def __enter__(self):
        self.path = tempfile.mkdtemp(prefix="docslurp-test.")
        return self

    def __exit__(self, *exc):
        shutil.rmtree(self.path, ignore_errors=True)

    def write(self, name, data):
        path = os.path.join(self.path, name)
        with open(path, "wb") as f:
            f.write(_b(data))
        return path


class Utils:
    """Runs the poppler utils built in this tree."""

    def __init__(self, bindir):
        self.bindir = bindir

    @classmethod
    def from_args(cls, argv=None):
        parser = argparse.ArgumentParser()
        parser.add_argument("--bindir", required=True)
        args, _ = parser.parse_known_args(argv)
        return cls(args.bindir)

    def run(self, util, *args, timeout=60, check=True, binary=False):
        cmd = [os.path.join(self.bindir, util)] + [str(a) for a in args]
        proc = subprocess.run(cmd, capture_output=True, timeout=timeout)
        if check and proc.returncode != 0:
            raise AssertionError("%s exited %d\nstderr:\n%s" % (" ".join(cmd), proc.returncode, proc.stderr.decode(errors="replace")))
        return proc.stdout if binary else proc.stdout.decode("utf-8", errors="replace")


_failures = []


def check(cond, what, detail=""):
    """Record one assertion; print PASS/FAIL with detail so ctest logs are useful."""
    if cond:
        print("PASS: %s" % what)
    else:
        print("FAIL: %s\n%s" % (what, detail))
        _failures.append(what)


def finish():
    """Exit non-zero if any check failed."""
    if _failures:
        print("%d check(s) failed" % len(_failures))
        sys.exit(1)
    print("all checks passed")
    sys.exit(0)
