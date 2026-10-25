"""ToUnicode lookups for multi-code-point entries: precedence semantics and O(1) cost.

Part 1 checks that "later CMap entries take precedence" holds for every combination of
single/multi-code-point entries and for non-BMP entries.  Part 2 is a CPU-time guard: a 20k-entry
ToUnicode of 2-code-point mappings used to cost a reverse linear scan of the string table per
glyph (about 5x the single-code-point time on this fixture); it must now cost about the same.
"""

import os
import resource
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import um_fixtures as f  # noqa: E402
import docslurp_testlib as t  # noqa: E402

utils = t.Utils.from_args()

HEAD = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n1 begincodespacerange <0000> <ffff> endcodespacerange\n"
TAIL = b"endcmap end end"


def extract(body):
    content = b"BT /F1 12 Tf 72 700 Td <000100020003000400050006> Tj ET"
    pdf = f.type0_page(content, "Identity-H", HEAD + body + TAIL, ordering="Identity")
    with t.TempDir() as d:
        path = d.write("t.pdf", pdf)
        return utils.run("pdftotext", path, "-").strip()


# code 1: multi then multi (later wins)      -> "CD"
# code 2: multi then single (single wins)    -> "G"
# code 3: single then multi (multi wins)     -> "IJ"
# codes 4,5: multi bfrange                   -> "AB", "AC"
# code 6: non-BMP via surrogate pair          -> U+1D400
body = (
    b"3 beginbfchar <0001> <00410042> <0002> <00450046> <0003> <0048> endbfchar\n"
    b"3 beginbfchar <0001> <00430044> <0002> <0047> <0003> <0049004A> endbfchar\n"
    b"1 beginbfrange <0004> <0005> <00410042> endbfrange\n"
    b"1 beginbfchar <0006> <D835DC00> endbfchar\n"
)
out = extract(body)
t.check(out == "CDGIJABAC\U0001d400", "later CMap entries take precedence; non-BMP entries map", repr(out))

# ---- CPU guard ----
BENCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "bench", "gen_tounicode_bench.py")


def user_cpu(path):
    best = None
    for _ in range(3):
        before = resource.getrusage(resource.RUSAGE_CHILDREN).ru_utime
        subprocess.run([os.path.join(utils.bindir, "pdftotext"), path, os.devnull], check=True, capture_output=True)
        used = resource.getrusage(resource.RUSAGE_CHILDREN).ru_utime - before
        best = used if best is None else min(best, used)
    return best


with t.TempDir() as d:
    times = {}
    for kind in ("single", "multi", "nonbmp", "unmapped"):
        path = os.path.join(d.path, kind + ".pdf")
        subprocess.run([sys.executable, "-I", BENCH, path, "--kind", kind], check=True, env=dict(os.environ))
        times[kind] = user_cpu(path)
    print("user CPU (best of 3): " + ", ".join("%s=%.3fs" % kv for kv in sorted(times.items())))
    # "multi" yields twice as many characters as "single", so it is only printed; "nonbmp" (one
    # character per glyph, stored in the string table) and "unmapped" (every lookup misses while
    # the string table is large) are directly comparable with "single".
    t.check(times["nonbmp"] < 2.2 * times["single"] + 0.03, "non-BMP (string table) entries cost about the same as plain entries", repr(times))
    t.check(times["unmapped"] < 2.2 * times["single"] + 0.03, "unmapped codes with a large string table cost about the same", repr(times))
t.finish()
