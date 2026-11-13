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
parser.add_argument("--info-probe", default="")
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
        "xrefstm_nest_main": ("xrefstm_nest.py", ["400", "1000000", "{out}", "main"]),
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
        "lenstack": ("lenstack.py", []),
        "missing_single": ("missing.py", ["single"]),
        "missing_prev": ("missing.py", ["prev"]),
        "scopebomb": ("scopebomb.py", ["200", "20000000"]),
        "gapprev": ("gapprev.py", []),
        "ocbudget": ("ocbudget.py", ["1000", "250"]),
        "ownparse": ("ownparse.py", []),
        "shapedbomb": ("shapedbomb.py", ["200", "20000000"]),
        "depthpred": ("depthpred.py", ["249"]),
        "deepoc": ("deepoc.py", ["600"]),
        "repairbomb": ("repairbomb2.py", ["200", "20000000"]),
        "shapedbomb_decodeparms": ("shapedbomb_decodeparms.py", ["200", "20000000"]),
        "dp_reuse": ("dp_reuse.py", ["249"]),
        "notokbomb": ("notokbomb.py", ["200", "20000000"]),
        "failpred": ("failpred.py", ["249"]),
    }
    # argument order of each generator: "out" first or last
    OUT_FIRST = {"selfprev.py", "manysec.py", "tailprev.py", "objstm_es.py", "sp_depth_inv.py", "stmreuse.py", "wide30m_paeth.py", "missing.py", "scopebomb.py", "gapprev.py", "ocbudget.py", "ownparse.py", "shapedbomb.py", "depthpred.py", "deepoc.py", "repairbomb2.py", "shapedbomb_decodeparms.py", "dp_reuse.py", "notokbomb.py", "failpred.py"} - {"manysec.py"}
    OUT_SECOND = {"manysec.py", "widepred_old.py"}
    paths = {}
    for name, gen in cases.items():
        path = os.path.join(base, name + ".pdf")
        if gen:
            script, extra = gen
            if "{out}" in extra:
                cmd = [sys.executable, "-I", os.path.join(GEN, script)] + [path if a == "{out}" else a for a in extra]
            elif script in OUT_SECOND:
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
        if name == "deepoc":
            # an intact file whose object stream holds a member nested past the parser's limit:
            # not refused (the cut is the stream's own, not the route's), the hidden image stays out
            t.check(b"<image " not in run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-images", path, "-"])[0], "deepoc: the hidden image is not reported (a member's own depth cut is not a route-dependent parse)")
        t.check(os.path.getsize(path) > 0 and rc in (0, 1, 2, 3), "%s: generated and opened" % name, repr((os.path.exists(path), rc)))
        if name in ("fullchain", "selfprev", "h-bomb", "prevstr", "xrefstm_nest", "wide_pred", "widepred_old", "tail_eof", "manysec999") and rc == 0:
            dl = doc_line(pf)
            t.check(dl.get("xref") == "ok" and b"xref-repaired" not in pf and b'xrefRebuilt="1"' not in outs[0], "%s: not repaired, no page flagged" % name, repr((dl.get("xref"), pf[-400:])))
        attach = os.path.join(BIN, "pdfattach")
        detach = os.path.join(BIN, "pdfdetach")
        if name == "gapprev" and os.path.exists(attach) and os.path.exists(detach):
            # a writer's added objects survive the rebuild a missing entry makes
            note = os.path.join(base, "note.txt")
            with open(note, "wb") as f:
                f.write(b"quarterly beat note\n")
            attached = os.path.join(base, "gapprev-attached.pdf")
            subprocess.run([attach, path, note, attached], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
            out_dir = os.path.join(base, "detached")
            os.makedirs(out_dir, exist_ok=True)
            subprocess.run([detach, "-saveall", "-o", out_dir, attached], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
            saved = os.path.join(out_dir, "note.txt")
            t.check(os.path.exists(saved) and open(saved, "rb").read() == b"quarterly beat note\n", "gapprev: pdfattach keeps the attached file's data (edits survive a rebuild)")
        if name == "failpred" and rc == 0:
            p3 = [json.loads(l) for l in pf.splitlines() if json.loads(l).get("page") == 3]
            t.check(p3 and "xref-repaired-earlier" in p3[0].get("parseWarnings", []), "failpred: page 3, whose object stream failed to parse because of an earlier route-dependent refusal, is marked xref-repaired-earlier", repr(p3[:1]))
        if name in ("depthpred", "dp_reuse") and rc == 0:
            # page 2's widget value sits in an object stream page 1 reached at the depth limit:
            # refused on page 2 too, and the page says a run of it alone may differ
            p2 = [json.loads(l) for l in pf.splitlines() if json.loads(l).get("page") == 2]
            t.check(p2 and "xref-repaired-earlier" in p2[0].get("parseWarnings", []), "depthpred: page 2 refused from page 1's finding is marked xref-repaired-earlier", repr(p2[:1]))
        if name == "gapprev" and args.info_probe:
            # a writer's /Info edit survives the rebuild a later missing entry makes
            edited = os.path.join(base, "gapprev-info.pdf")
            rc_probe = subprocess.run([args.info_probe, path, edited], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120).returncode
            info = subprocess.run([os.path.join(BIN, "pdfinfo"), edited], capture_output=True, timeout=60).stdout if os.path.exists(os.path.join(BIN, "pdfinfo")) else b""
            t.check(rc_probe == 0 and b"Record quarter" in info, "gapprev: a writer's /Info edit survives the missing-entry rebuild", repr((rc_probe, info[:200])))
        if name == "lenstack":
            # the XHTML header's /Title (in an object stream both the doc line and the header
            # reach): each guarded scope parses it for itself
            on = with_facts(path, ["-bbox-layout"])[0]
            off = run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", path, "-"])[0]
            title = lambda x: x[x.find(b"<title>") : x.find(b"</title>")]
            t.check(title(on) == title(off), "lenstack: the header's /Title is the same with and without facts (its object stream parses differently by route, so it is left out in both)", repr((title(on), title(off))))
        if name in ("manysec999", "manysec1000", "tail_eof"):
            t.check(b">Record<" in outs[0] or b"Record" in outs[0], "%s: the page content (listed only by the oldest section) is drawn" % name)
        if name == "manysec1000":
            t.check(b'xrefRebuilt="1"' in run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-rules", path, "-"])[0], "manysec1000: past the 999-section cap the content is found by a rebuild, flagged")
        unite = os.path.join(BIN, "pdfunite")
        if name in ("manysec1000", "tail_eof", "manysec999", "missing_single", "missing_prev") and os.path.exists(unite):
            merged = os.path.join(base, name + "-united.pdf")
            subprocess.run([unite, path, path, merged], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
            text = run([os.path.join(BIN, "pdftotext"), "-q", merged, "-"])[0]
            t.check(text.count(b"Record") == 2, "%s: pdfunite keeps both pages' content (a writer resumes the chain)" % name, repr(text[:200]))
t.finish()
