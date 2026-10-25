"""-bbox, -bbox-layout and -tsv must look up the output encoding once per run,
not once per word (GlobalParams::getTextEncoding() takes a global mutex and
copies a std::string). Counted with callgrind; skipped (77) without valgrind."""

import os
import re
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pto_common import t  # noqa: E402

utils = t.Utils.from_args()
valgrind = shutil.which("valgrind")
if not valgrind:
    print("SKIP: valgrind not installed")
    sys.exit(77)

WORDS = 300
ops = b" ".join(b"1 0 0 1 %d %d Tm (w%d) Tj" % (40 + 50 * (i % 10), 750 - 12 * (i // 10), i) for i in range(WORDS))
pdf = t.simple_page_pdf(b"BT /F1 8 Tf " + ops + b" ET")


def lookups(path, outdir, mode):
    outfile = os.path.join(outdir, "cg.out")
    cmd = [valgrind, "--tool=callgrind", "--compress-strings=no", "--callgrind-out-file=" + outfile, os.path.join(utils.bindir, "pdftotext"), mode, path, os.path.join(outdir, "out.txt")]
    proc = subprocess.run(cmd, capture_output=True, timeout=600)
    if proc.returncode != 0:
        raise AssertionError("valgrind failed: " + proc.stderr.decode(errors="replace"))
    total = 0
    callee = ""
    for line in open(outfile, errors="replace"):
        if line.startswith("cfn="):
            callee = line[4:]
        elif line.startswith("calls=") and "getTextEncodingName" in callee:
            total += int(line.split()[0][6:])
    words = len(re.findall(r"<word |^5\t", open(os.path.join(outdir, "out.txt"), errors="replace").read(), re.M))
    return total, words


with t.TempDir() as d:
    path = d.write("many.pdf", pdf)
    for mode in ("-bbox", "-bbox-layout", "-tsv"):
        n, words = lookups(path, d.path, mode)
        t.check(words == WORDS, "%s: all %d words are in the output" % (mode, WORDS), "got %d" % words)
        t.check(n <= 5, "%s: output encoding looked up %d times for %d words (want a constant, <= 5)" % (mode, n, WORDS), "")
t.finish()
