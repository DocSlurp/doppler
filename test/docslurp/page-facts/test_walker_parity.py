"""Issue 43 parity: page facts' per-grid thin-rule counts and centre-based words inside images
against the layout walker they are meant to replace (DocSlurp/docslurp-oss
worker/scripts/pdf_layout_facts.py, run with python3).

Facts and XHTML come from one `pdftotext -bbox-layout -rules -page-facts-fd` run, the pairing
the contract covers. For each page of each input PDF (the walker's own public fixtures, worker/testdata/pdf-fixtures,
and this directory's grid and image fixtures):

* grids: the walker's ruled_grids over this build's `pdftotext -bbox-layout -rules` XHTML gives
  (rows, columns) per grid. The facts state the walker's grids when the page is known: not
  gridsTruncated, unlistedRuledGrids 0, and every grid counted (hRules present). They are then
  the grids with connected true, hRules >= 2 and vRules >= 3, as (hRules - 1, vRules - 1), and
  that multiset must equal the walker's. An unknown page passes (a reader treats it as unknown),
  and is reported. A known page that differs from the walker fails: never known and wrong.
* words inside images: the walker's layout_facts (this build's `pdfimages -list` and XHTML)
  gives wordsInside per placed image. Each must equal visibleWordsCentredInside +
  invisibleWordsCentredInside of the facts image whose box matches (within 1 pt).

Known word-count differences are listed in KNOWN (fixture, page, "words") with the reason;
they are reported and do not fail, and a KNOWN entry that starts to match fails.

Needs the docslurp-oss checkout: --oss DIR or DOCSLURP_OSS_DIR. Without it the test is skipped
(exit 77).
"""

import argparse
import importlib.util
import json
import os
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR", os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import docslurp_testlib as t  # noqa: E402
import pf_fixtures as fx  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("--bindir", required=True)
parser.add_argument("--oss", default=os.environ.get("DOCSLURP_OSS_DIR", ""))
parser.add_argument("--max-pages", type=int, default=8)
parser.add_argument("--json", default="", help="write the per-page comparison here")
args, _ = parser.parse_known_args()
BIN = args.bindir

walker_path = os.path.join(args.oss, "worker", "scripts", "pdf_layout_facts.py") if args.oss else ""
if not walker_path or not os.path.isfile(walker_path):
    print("SKIP: no docslurp-oss checkout (--oss or DOCSLURP_OSS_DIR)")
    sys.exit(77)
spec = importlib.util.spec_from_file_location("pdf_layout_facts", walker_path)
walker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walker)

# (fixture, page, "grids" | "words"): why the facts cannot match the walker there
KNOWN = {}


def run(cmd):
    return subprocess.run(cmd, capture_output=True, timeout=300)


def same_run(path, pages):
    """(XHTML, facts lines) of one `pdftotext -bbox-layout -rules -page-facts-fd` run: the pairing
    the known-grids contract covers (README, "One run or two")."""
    with tempfile.TemporaryDirectory() as d:
        out, facts_path = os.path.join(d, "layout.xhtml"), os.path.join(d, "facts.jsonl")
        fd = os.open(facts_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        try:
            p = subprocess.run([os.path.join(BIN, "pdftotext"), "-q", "-bbox-layout", "-rules", "-page-facts-fd", str(fd), "-page-facts-level", "full", "-f", "1", "-l", str(pages), path, out], capture_output=True, timeout=300, pass_fds=(fd,))
        finally:
            os.close(fd)
        if p.returncode != 0:
            return None, None
        return open(out, encoding="utf-8", errors="replace").read(), open(facts_path, "rb").read()


def walker_side(path, pages, xhtml):
    listing = run([os.path.join(BIN, "pdfimages"), "-list", "-f", "1", "-l", str(pages), path]).stdout.decode("utf-8", "replace")
    rules = walker.page_rules(xhtml, range(pages))
    layout = walker.layout_facts(path, listing, xhtml, None, 1, pages)["pages"]
    out = {}
    for n in range(1, pages + 1):
        grids = sorted((g["rows"], g["columns"]) for g in walker.ruled_grids(rules.get(n - 1, [])))
        entry = layout[n - 1]
        images = [(im["bbox"], im["wordsInside"]) for im in (entry.get("images") or []) if im.get("bbox") and im.get("wordsInside") is not None]
        out[n] = {"grids": grids, "images": images}
    return out


def facts_side(raw):
    out = {}
    for line in raw.decode("ascii").splitlines():
        d = json.loads(line)
        if d.get("type") != "page":
            continue
        v = d.get("vectors") or {}
        grids = v.get("grids") or []
        complete = not v.get("gridsTruncated") and v.get("unlistedRuledGrids") == 0 and all("hRules" in g for g in grids) and "xref-repaired" not in d.get("parseWarnings", [])
        walkerlike = sorted((g["hRules"] - 1, g["vRules"] - 1) for g in grids if g.get("connected") and g["hRules"] >= 2 and g["vRules"] >= 3)
        items = (d.get("images") or {}).get("items") or []
        out[d["page"]] = {
            "grids": walkerlike,
            "complete": complete,
            "raw": [{k: g.get(k) for k in ("rows", "columns", "hRules", "vRules", "outerSides", "connected")} for g in grids],
            "unlisted": v.get("unlistedRuledGrids"), "truncated": v.get("gridsTruncated"),
            "images": [(it["bbox"], it["visibleWordsCentredInside"] + it["invisibleWordsCentredInside"], it["visibleWordsInside"] + it["invisibleWordsInside"]) for it in items],
        }
    return out


def page_count(path):
    p = run([os.path.join(BIN, "pdfinfo"), path])
    for line in p.stdout.decode("utf-8", "replace").splitlines():
        if line.startswith("Pages:"):
            return int(line.split()[1])
    return 0


inputs = []
fixtures_dir = os.path.join(args.oss, "worker", "testdata", "pdf-fixtures")
for name in sorted(os.listdir(fixtures_dir)) if os.path.isdir(fixtures_dir) else []:
    if name.endswith(".pdf"):
        inputs.append((name[:-4], os.path.join(fixtures_dir, name)))
tmp = tempfile.TemporaryDirectory()
for name in ("grids", "grid_rules", "grid_split", "grid_rebuilt", "grid_reopened", "grid_titled", "image_words", "caption_straddle", "vectors", "scans", "images"):
    path = os.path.join(tmp.name, name + ".pdf")
    with open(path, "wb") as f:
        f.write(fx.ALL[name]())
    inputs.append((name, path))
t.check(len(inputs) > 20, "parity inputs: the walker's public fixtures and the grid/image fixtures", repr([n for n, _ in inputs]))

report = []
totals = {"pages": 0, "gridPagesKnownEqual": 0, "gridPagesUnknown": 0, "gridPagesUnknownWalkerGrids": 0, "gridPages": 0, "walkerGrids": 0, "images": 0, "imagesEqual": 0, "imagesWholeBoxDiffer": 0}
for name, path in inputs:
    pages = min(page_count(path), args.max_pages)
    if pages == 0:
        continue
    xhtml, raw = same_run(path, pages)
    if xhtml is None:
        t.check(False, "%s: pdftotext ran" % name)
        continue
    w = walker_side(path, pages, xhtml)
    f = facts_side(raw)
    for n in range(1, pages + 1):
        wp, fp = w[n], f.get(n)
        if fp is None:
            t.check(False, "%s p%d: a facts line" % (name, n))
            continue
        totals["pages"] += 1
        row = {"fixture": name, "page": n, "walkerGrids": wp["grids"], "factsGrids": fp["grids"], "factsRaw": fp["raw"], "images": []}
        if wp["grids"] or fp["grids"] or fp["raw"]:
            totals["gridPages"] += 1
        totals["walkerGrids"] += len(wp["grids"])
        same = wp["grids"] == fp["grids"]
        if not fp["complete"]:
            totals["gridPagesUnknown"] += 1
            totals["gridPagesUnknownWalkerGrids"] += bool(wp["grids"])
            t.check(True, "%s p%d grids: unknown to a reader (truncated %s, unlisted %s); walker %r" % (name, n, fp["truncated"], fp["unlisted"], wp["grids"]))
        else:
            t.check(same, "%s p%d grids: known, facts %r == walker %r" % (name, n, fp["grids"], wp["grids"]), repr(fp["raw"]))
        totals["gridPagesKnownEqual"] += fp["complete"] and same and bool(wp["grids"] or fp["raw"])
        for bbox, inside in wp["images"]:
            match = [it for it in fp["images"] if all(abs(a - b) <= 1.0 for a, b in zip(bbox, it[0]))]
            got = match[0][1] if match else None
            whole = match[0][2] if match else None
            totals["images"] += 1
            totals["imagesEqual"] += got == inside
            totals["imagesWholeBoxDiffer"] += whole is not None and whole != inside
            row["images"].append({"bbox": bbox, "walker": inside, "factsCentred": got, "factsWholeBox": whole})
            key = (name, n, "words")
            if key in KNOWN:
                t.check(got != inside, "%s p%d words: known difference still differs (%s)" % (name, n, KNOWN[key]))
            else:
                t.check(got == inside, "%s p%d image %r: facts centred %r == walker %r (whole-box %r)" % (name, n, bbox, got, inside, whole))
        report.append(row)

print("totals: %s" % json.dumps(totals))
# a regression that made every page unknown would pass the per-page checks: keep a floor
t.check(totals["gridPagesKnownEqual"] >= 23 and totals["imagesEqual"] >= 36, "parity: at least 23 grid pages known and equal, 36 images equal (this test's count)", json.dumps(totals))
if args.json:
    with open(args.json, "w") as out:
        json.dump({"totals": totals, "pages": report}, out, indent=1)
t.finish()
