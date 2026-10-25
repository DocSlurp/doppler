"""Fix 4: unknown and PDF 2.0 structure types keep their children."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tagged_pdf as tp  # noqa: E402

t = tp.t
utils = t.Utils.from_args()


def mc(text, mcid):
    return b"/P <</MCID %d>> BDC BT /F1 12 Tf 72 %d Td (%s) Tj ET EMC\n" % (mcid, 750 - 20 * mcid, text.encode())


def struct(pdf, *opts):
    with t.TempDir() as d:
        path = d.write("s.pdf", pdf)
        return utils.run("pdfinfo", *opts, path)


def lines(out):
    return [line.strip() for line in out.splitlines()]


# 1. PDF 2.0 types and a custom type, all wrapping content.
content = b"".join(mc("text %d" % i, i) for i in range(10))
types = ["FENote", "Title", "Aside", "Em", "Strong", "Sub", "DocumentFragment", "Artifact", "MyCustomThing", "H7"]
root = tp.Elem("Document", [tp.Elem(ty, [tp.Elem("P", [tp.MC(0, i)], pg=0)]) for i, ty in enumerate(types)])
pdf = tp.build_tagged([content], root)
out = struct(pdf, "-struct-text")
ls = lines(out)
for i, ty in enumerate(types):
    t.check('"text %d"' % i in ls, "content under /%s is kept" % ty, out)
    t.check(ty in ls or any(l.startswith(ty + " ") for l in ls), "/%s name is preserved in the structure dump" % ty, out)
t.check("Unknown" not in out, "no element reported as Unknown", out)

# 2. A RoleMap that resolves a custom type still wins over the generic fallback.
root = tp.Elem("Document", [tp.Elem("Chapter", [tp.Elem("P", [tp.MC(0, 0)], pg=0)])])
out = struct(tp.build_tagged([content], root, role_map="/Chapter /Sect"), "-struct-text")
ls = lines(out)
t.check("Sect (block)" in ls or "Sect" in ls, "RoleMap-resolved type is reported by its standard name", out)
t.check('"text 0"' in ls, "content under RoleMap-resolved type is kept", out)

# 3. Deeply nested content below an unknown element is reachable.
deep = tp.Elem("Document", [tp.Elem("Mystery", [tp.Elem("Div", [tp.Elem("Div", [tp.Elem("P", [tp.MC(0, 3)], pg=0)])])])])
out = struct(tp.build_tagged([content], deep), "-struct-text")
t.check('"text 3"' in lines(out), "grandchildren of an unknown element are kept", out)

# 4. An element that is really invalid (no /S) is dropped together with its
#    children. Its descendants must not stay registered in the ParentTree:
#    findParentElement() returned pointers to freed elements.
bad = tp.Elem("Document", [tp.Elem("P", [tp.MC(0, 1)], pg=0), tp.Elem("BadP", [tp.Elem("Span", [tp.MC(0, 2)], pg=0)])])
pdf = tp.build_tagged([content], bad).replace(b"/S /BadP", b"/X /BadP", 1)
out = struct(pdf, "-struct-text")
t.check('"text 1"' in lines(out) and '"text 2"' not in lines(out), "valid sibling kept, invalid element dropped", out)

probe = None
for i, a in enumerate(sys.argv):
    if a == "--probe":
        probe = sys.argv[i + 1]
if probe:
    import subprocess

    with t.TempDir() as d:
        path = d.write("bad.pdf", pdf)
        res = subprocess.run([probe, path, "0", "4"], capture_output=True, text=True, timeout=60)
        t.check(res.returncode == 0, "probe runs", res.stderr)
        got = res.stdout.splitlines()
        t.check(got == ["mcid 0: none", "mcid 1: found", "mcid 2: none", "mcid 3: none"], "ParentTree has no entry for the dropped element", got)
        vg = None
        for cand in ("/usr/bin/valgrind",):
            if os.path.exists(cand):
                vg = cand
        if vg:
            res = subprocess.run([vg, "--error-exitcode=9", "-q", probe, "--deref", path, "0", "4"], capture_output=True, text=True, timeout=300)
            t.check(res.returncode == 0, "valgrind: reading ParentTree elements is clean", res.stderr[-2000:])

t.finish()
