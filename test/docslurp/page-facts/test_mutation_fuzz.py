"""Mutation fuzz of the page-facts path: seeded byte mutations of the page-facts fixtures.

pdfpagefacts must never die by a signal, must exit 0, 1, 2 or 3, and every line it writes
must be JSON valid against the schema (a doc line first, a run line last when it finished).
Every 10th case also runs pdftotext -page-facts-fd, which must write the same lines and the same
XHTML as pdftotext without facts (issue 45).
Set PAGE_FACTS_FUZZ_CASES to run more (default 320); the seed is fixed.
"""

import argparse
import concurrent.futures
import json
import os
import random
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402
import pf_fixtures as fx  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("--bindir", required=True)
args, _ = parser.parse_known_args()
BIN = args.bindir
HERE = os.path.dirname(os.path.abspath(__file__))
SCHEMA = json.load(open(os.path.join(HERE, "..", "..", "..", "docslurp", "page-facts", "doppler-page-facts-1.schema.json")))
CASES = int(os.environ.get("PAGE_FACTS_FUZZ_CASES", "320"))


def mutate(data, rng):
    data = bytearray(data)
    kind = rng.randrange(7)
    if kind == 0:  # flip bytes
        for _ in range(rng.randint(1, 24)):
            data[rng.randrange(len(data))] = rng.randrange(256)
    elif kind == 1:  # truncate
        del data[rng.randrange(len(data)) :]
    elif kind == 2:  # insert junk
        at = rng.randrange(len(data))
        data[at:at] = bytes(rng.randrange(256) for _ in range(rng.randint(1, 64)))
    elif kind == 3:  # damage numbers (offsets, lengths, counts, coordinates)
        digits = [i for i, b in enumerate(data) if 0x30 <= b <= 0x39]
        for _ in range(rng.randint(1, 12)):
            if digits:
                data[rng.choice(digits)] = rng.choice(b"0123456789-.")
    elif kind == 4:  # delete a range
        a = rng.randrange(len(data))
        del data[a : a + rng.randint(1, 200)]
    elif kind == 5:  # duplicate a range (nested objects, repeated operators)
        a = rng.randrange(len(data))
        chunk = data[a : a + rng.randint(1, 400)]
        b = rng.randrange(len(data))
        data[b:b] = chunk * rng.randint(1, 8)
    else:  # swap PDF keywords and operators
        words = [b"obj", b"endobj", b"stream", b"endstream", b"BT", b"ET", b"Tj", b"Do", b"BDC", b"EMC", b"/Type", b"/Kids", b"/Parent", b"R", b"q", b"Q", b"cm", b"re", b"f", b"W", b"n", b"/Filter", b"/OC", b"/Annots", b"/AcroForm"]
        for _ in range(rng.randint(1, 6)):
            w = rng.choice(words)
            at = bytes(data).find(w, rng.randrange(len(data)))
            if at >= 0:
                data[at : at + len(w)] = rng.choice(words)
    return bytes(data)


def check_lines(out):
    errs = []
    text = out.decode("ascii", errors="replace")
    ls = text.splitlines()
    if out and not out.endswith(b"\n"):
        errs.append("output does not end with a newline")
    if not out.isascii():
        errs.append("output is not ASCII")
    parsed = []
    for i, line in enumerate(ls):
        try:
            obj = json.loads(line)
        except ValueError as e:
            errs.append("line %d not JSON: %s" % (i + 1, e))
            continue
        parsed.append(obj)
        errs += ["line %d: %s" % (i + 1, e) for e in t.schema_errors(obj, SCHEMA)]
    if parsed and parsed[0].get("type") != "doc":
        errs.append("first line is not the doc line")
    return errs, parsed


def run_case(case):
    i, name, data, path = case
    p = subprocess.run([os.path.join(BIN, "pdfpagefacts"), "-level", "full", "-max-memory", "2048", "-l", "8", path], capture_output=True, timeout=120)
    problems = []
    if p.returncode < 0:
        problems.append("killed by signal %d" % -p.returncode)
    elif p.returncode not in (0, 1, 2, 3):
        problems.append("exit %d" % p.returncode)
    errs, parsed = check_lines(p.stdout)
    problems += errs
    if p.returncode == 0 and (not parsed or parsed[-1].get("type") != "run"):
        problems.append("exit 0 without a run line")
    if p.stderr and not all(l.startswith(b"pdfpagefacts: ") for l in p.stderr.splitlines()):
        problems.append("stderr not fixed strings: %r" % p.stderr[:200])
    if i % 10 == 0 and p.returncode == 0:
        r, w = os.pipe()
        flags = [os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-wordmeta", "-rules", "-compact-layout", "-images", "-l", "8"]
        out_path = path + ".xhtml"
        q = subprocess.Popen(
            flags + ["-page-facts-fd", str(w), "-page-facts-level", "full", path, out_path],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            pass_fds=(w,),
        )
        os.close(w)
        got = b""
        while True:
            b = os.read(r, 65536)
            if not b:
                break
            got += b
        os.close(r)
        q.wait(timeout=120)
        if q.returncode < 0:
            problems.append("pdftotext killed by signal %d" % -q.returncode)
        elif got != p.stdout:
            problems.append("pdftotext -page-facts-fd differs from pdfpagefacts")
        else:
            # issue 45: the facts' reads never change what the pages resolve
            plain = subprocess.run(flags + [path, "-"], capture_output=True, timeout=120).stdout
            with open(out_path, "rb") as f:
                if f.read() != plain:
                    problems.append("pdftotext XHTML with facts differs from the run without")
    return i, name, p.returncode, problems


rng = random.Random(20261112)
seeds = {name: make() for name, make in fx.ALL.items() if name != "vectors"}
seeds["vectors_small"] = fx.doc([{"content": "0.5 w 72 600 m 472 600 l S 72 560 m 472 560 l S 100 400 m 100 600 l S /Sh1 sh\n" + fx.words(20)}], extras=[])
names = sorted(seeds)
with t.TempDir() as d:
    cases = []
    for i in range(CASES):
        name = names[i % len(names)]
        data = mutate(seeds[name], rng)
        cases.append((i, name, data, d.write("m%04d.pdf" % i, data)))
    exits = {}
    failures = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for i, name, rc, problems in pool.map(run_case, cases):
            exits[rc] = exits.get(rc, 0) + 1
            if problems:
                failures.append("case %d (%s, exit %d): %s" % (i, name, rc, "; ".join(problems[:3])))
    print("exit codes: %s" % sorted(exits.items()))
    t.check(not failures, "%d mutated PDFs: no signal, exit in {0,1,2,3}, every line valid JSON + schema, pdftotext agrees" % CASES, "\n".join(failures[:20]))
    t.check(exits.get(0, 0) > CASES // 3, "most mutations still produce facts", repr(exits))
t.finish()
