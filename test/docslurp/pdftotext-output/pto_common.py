"""Shared helpers for the pdftotext-output regression tests (imported by path)."""

import os
import re
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(HERE)))
import docslurp_testlib as t  # noqa: E402,F401

WORD_RE = re.compile(r'<word xMin="([-\d.]+)" yMin="([-\d.]+)" xMax="([-\d.]+)" yMax="([-\d.]+)"([^>]*)>(.*?)</word>')
PAGE_RE = re.compile(r'<page width="([-\d.]+)" height="([-\d.]+)">')


def xml_pages(out):
    """Return [(width, height, [(xMin, yMin, xMax, yMax, extra_attrs, text), ...]), ...]."""
    pages = []
    for chunk in out.split("<page ")[1:]:
        m = PAGE_RE.match("<page " + chunk)
        words = [(float(a), float(b), float(c), float(d), e, txt) for a, b, c, d, e, txt in WORD_RE.findall(chunk)]
        pages.append((float(m.group(1)), float(m.group(2)), words))
    return pages


def tsv_rows(out):
    """Split TSV output into rows of columns (rows are '\\n' separated)."""
    return [line.split("\t") for line in out.split("\n") if line]
