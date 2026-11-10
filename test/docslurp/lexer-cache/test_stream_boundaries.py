"""Cached lexer lookahead must preserve token and stream boundaries."""
import os
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t


def pdf(parts):
    refs = " ".join(f"{5+i} 0 R" for i in range(len(parts)))
    return t.build_pdf([
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents [{refs}] >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        *[t.stream("", part) for part in parts],
    ])


utils = t.Utils.from_args()
prefix = b"BT /F1 12 Tf 72 700 Td "
cases = [
    ("empty-before-after", [b"", prefix + b"(Hello) Tj ET", b""], "Hello"),
    ("command-at-eof", [b"BT /F1 12 Tf", b"72 700 Td (Hello) Tj ET"], "Hello"),
    ("string-across-streams", [prefix + b"(Hel", b"lo) Tj ET"], "Hello"),
    ("escape-across-streams", [prefix + b"(Hello\\", b"(World\\)) Tj ET"], "Hello(World)"),
    ("hex-at-eof", [prefix + b"<48656c6c6f>", b"Tj ET"], "Hello"),
    ("comment-at-eof", [prefix + b"% comment", b"\n(Hello) Tj ET"], "Hello"),
]
with t.TempDir() as directory:
    for name, parts, expected in cases:
        path = directory.write(name + ".pdf", pdf(parts))
        output = utils.run("pdftotext", "-q", path, "-")
        t.check(output.strip("\n\f ") == expected, name, repr(output))
t.finish()
