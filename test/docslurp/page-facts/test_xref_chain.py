"""Issue 45: the /Prev chain read at open, and read-only lookups, under hostile cross-reference
tables. The generators in xref_hostile/ come from the issue's security council.

For every file:
- pdftotext and pdfpagefacts finish within the CPU and memory bounds;
- pdftotext writes the same XHTML with and without -page-facts-fd, with and without -rules
  -images;
- pdftotext -page-facts-fd writes the same lines as pdfpagefacts;
- three separate runs are byte-identical.

Plus the per-case expectations below.
"""

import argparse
import json
import os
import subprocess
import sys

sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("--bindir", required=True)
args, _ = parser.parse_known_args()
BIN = args.bindir
GEN = os.path.join(os.path.dirname(os.path.abspath(__file__)), "xref_hostile")
CPU_LIMIT = 10.0  # seconds, per process; master took 2.7 to 155 s on the worst of these
RSS_LIMIT = 400 * 1024  # KiB


def run(cmd, pass_fds=()):
    """(stdout, cpu seconds, peak RSS KiB, exit code)."""
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, pass_fds=pass_fds)
    out = p.stdout.read()
    _, status, ru = os.wait4(p.pid, 0)
    return out, ru.ru_utime + ru.ru_stime, ru.ru_maxrss, os.waitstatus_to_exitcode(status)


def with_facts(path, flags):
    r, w = os.pipe()
    p = subprocess.Popen([os.path.join(BIN, "pdftotext"), "-q"] + flags + ["-page-facts-fd", str(w), "-page-facts-level", "full", path, "-"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, pass_fds=(w,))
    os.close(w)
    import threading

    chunks = []

    def drain():
        while True:
            b = os.read(r, 65536)
            if not b:
                break
            chunks.append(b)

    th = threading.Thread(target=drain)
    th.start()
    xhtml = p.communicate(timeout=300)[0]
    th.join()
    os.close(r)
    return xhtml, b"".join(chunks)


def doc_line(facts):
    return json.loads(facts.split(b"\n", 1)[0])


with t.TempDir() as d:
    base = d.path
    subprocess.run([sys.executable, "-I", os.path.join(GEN, "chains.py"), base], check=True)
    cases = {
        "h-loop": None,
        "h-deep": None,
        "h-bomb": None,
        "h-bigfirst": None,
        "h-overlap": None,
        "prevstr": ("prevstr.py", ["300", "1000000"]),
        "trailers": ("trailers.py", ["2000", "100000"]),
        "zerow": ("zerow.py", ["65536", "1500", "1"]),
        "wide_pred": ("wide_pred.py", ["4800"]),
        "xrefstm_nest": ("xrefstm_nest.py", ["4000", "1000000"]),
        "xrefstm_nest_main": ("xrefstm_nest.py", ["400", "1000000", "main"]),
        "predchain": ("predchain.py", ["50", "1"]),
        "fullchain": ("fullchain.py", ["1100000", "10"]),
        "stale_objstm": ("stale_objstm.py", []),
        "stale_ocmd": ("stale_ocmd.py", []),
        "stale_objstm_hybrid": ("stale_objstm_hybrid.py", []),
        "stale_ocmd_rebuild": ("stale_ocmd_rebuild.py", []),
        "selfprev": ("selfprev.py", ["eof"]),
        "manysec999": ("manysec.py", ["999"]),
        "manysec1000": ("manysec.py", ["1000"]),
        "tail_eof": ("tailprev.py", ["eof"]),
        "widepred_old": ("widepred_old.py", ["4800"]),
        "objstm_depth": ("objstm_es.py", ["496"]),
        "streamparam_depth": ("sp_depth_inv.py", ["497"]),
        "xrefstm_reuse": ("stmreuse.py", ["999", "65536", "3"]),
        "paeth_chain": ("wide30m_paeth.py", ["999", "30000", "3"]),
    }
    # argument order of each generator: "out" first or last
    OUT_FIRST = {"selfprev.py", "manysec.py", "tailprev.py", "objstm_es.py", "sp_depth_inv.py", "stmreuse.py", "wide30m_paeth.py"}
    OUT_SECOND = {"manysec.py", "widepred_old.py"}
    paths = {}
    for name, gen in cases.items():
        path = os.path.join(base, name + ".pdf")
        if gen:
            script, extra = gen
            if script in OUT_SECOND:
                cmd = [sys.executable, "-I", os.path.join(GEN, script)] + extra + [path]
            elif script in OUT_FIRST:
                cmd = [sys.executable, "-I", os.path.join(GEN, script), path] + extra
            else:
                cmd = [sys.executable, "-I", os.path.join(GEN, script)] + extra + [path]
            subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL)
        else:
            path = os.path.join(base, name + ".pdf")
        paths[name] = path

    for name, path in paths.items():
        outs = []
        bounded = True
        for _ in range(3):
            out, cpu, rss, rc = run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-l", "3", path, "-"])
            outs.append(out)
            bounded = bounded and cpu < CPU_LIMIT and rss < RSS_LIMIT and rc >= 0
            if not bounded:
                break
        t.check(bounded, "%s: pdftotext within %g s CPU and %d MiB" % (name, CPU_LIMIT, RSS_LIMIT // 1024), "cpu %.2f rss %d rc %d" % (cpu, rss, rc))
        t.check(len(set(outs)) == 1, "%s: three runs byte-identical" % name)
        pf, cpu, rss, rc = run([os.path.join(BIN, "pdfpagefacts"), "-level", "full", "-l", "3", path])
        t.check(cpu < CPU_LIMIT and rss < RSS_LIMIT and rc in (0, 1, 2, 3), "%s: pdfpagefacts within bounds" % name, "cpu %.2f rss %d rc %d" % (cpu, rss, rc))
        for flags in (["-bbox-layout", "-l", "3"], ["-bbox-layout", "-wordmeta", "-rules", "-compact-layout", "-images", "-l", "3"]):
            plain = run([os.path.join(BIN, "pdftotext"), "-q"] + flags + [path, "-"])[0]
            xhtml, facts = with_facts(path, flags)
            t.check(xhtml == plain, "%s %s: XHTML with facts == without" % (name, " ".join(flags[:-2])))
            if rc == 0:
                t.check(facts == pf, "%s %s: pdftotext -page-facts-fd == pdfpagefacts" % (name, " ".join(flags[:-2])), "%r\n%r" % (facts[:300], pf[:300]))
        # files upstream read without a rebuild, and that stay within the bounds or have nothing
        # missing: not repaired, no page flagged, the text drawn
        if name in ("fullchain", "selfprev", "h-bomb", "prevstr", "xrefstm_nest", "wide_pred", "widepred_old", "tail_eof", "manysec999") and rc == 0:
            dl = doc_line(pf)
            t.check(dl.get("xref") == "ok" and b"xref-repaired" not in pf and b'xrefRebuilt="1"' not in outs[0], "%s: not repaired, no page flagged" % name, repr((dl.get("xref"), pf[-400:])))
        if name in ("manysec999", "manysec1000", "tail_eof"):
            t.check(b">Record<" in outs[0] or b"Record" in outs[0], "%s: the page content (listed only by the oldest section) is drawn" % name)
        if name == "manysec1000":
            t.check(b'xrefRebuilt="1"' in run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-rules", path, "-"])[0], "manysec1000: past the 999-section cap the content is found by a rebuild, flagged")
        unite = os.path.join(BIN, "pdfunite")
        if name in ("manysec1000", "tail_eof", "manysec999") and os.path.exists(unite):
            merged = os.path.join(base, name + "-united.pdf")
            subprocess.run([unite, path, path, merged], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
            text = run([os.path.join(BIN, "pdftotext"), "-q", merged, "-"])[0]
            t.check(text.count(b"Record") == 2, "%s: pdfunite keeps both pages' content (a writer resumes the chain)" % name, repr(text[:200]))
t.finish()
