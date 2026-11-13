"""Issue 43 parity: page facts' per-grid thin-rule counts and centre-based words inside images
against the layout walker they are meant to replace (DocSlurp/docslurp-oss
worker/scripts/pdf_layout_facts.py, run with python3).

For each page of each input PDF (the walker's own public fixtures, worker/testdata/pdf-fixtures,
and this directory's grid and image fixtures):

* grids: the walker's ruled_grids over this build's `pdftotext -bbox-layout -rules` XHTML gives
  (rows, columns) per grid. From the facts, every vectors.grids[] entry with hRules >= 2 and
  vRules >= 3 (the walker's minimum grid) gives (hRules - 1, vRules - 1). The two multisets must
  be equal.
* words inside images: the walker's layout_facts (this build's `pdfimages -list` and XHTML)
  gives wordsInside per placed image. Each must equal visibleWordsCentredInside +
  invisibleWordsCentredInside of the facts image whose box matches (within 1 pt).

Known differences are listed in KNOWN (fixture, page, kind) with the reason; they are reported
and do not fail, and a KNOWN entry that starts to match fails (so the list stays true).

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
KNOWN = {
    ("grids", 5, "grids"): "white rules: the walker reads rules of any colour, while the facts grid search skips rules lighter than luminance 0.97, so there is no facts grid to carry counts",
    ("routing-table-large-coverage", 1, "grids"): "rows 118 pt apart: the facts grid search groups rules at most 8 body sizes (80 pt) apart, so these rules make no facts grid; the walker joins any rules that touch",
}


def run(cmd):
    return subprocess.run(cmd, capture_output=True, timeout=300)


def walker_side(path, pages):
    with tempfile.TemporaryDirectory() as d:
        out = os.path.join(d, "layout.xhtml")
        p = run([os.path.join(BIN, "pdftotext"), "-bbox-layout", "-rules", "-f", "1", "-l", str(pages), path, out])
        if p.returncode != 0:
            return None
        xhtml = open(out, encoding="utf-8", errors="replace").read()
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


def facts_side(path, pages):
    p = run([os.path.join(BIN, "pdfpagefacts"), "-level", "full", "-f", "1", "-l", str(pages), path])
    out = {}
    for line in p.stdout.decode("ascii").splitlines():
        d = json.loads(line)
        if d.get("type") != "page":
            continue
        v = d.get("vectors") or {}
        grids = v.get("grids") or []
        complete = not v.get("gridsTruncated") and all("hRules" in g for g in grids)
        walkerlike = sorted((g["hRules"] - 1, g["vRules"] - 1) for g in grids if "hRules" in g and g["hRules"] >= 2 and g["vRules"] >= 3)
        items = (d.get("images") or {}).get("items") or []
        out[d["page"]] = {
            "grids": walkerlike,
            "complete": complete,
            "raw": [{k: g.get(k) for k in ("rows", "columns", "hRules", "vRules", "outerSides")} for g in grids],
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
for name in ("grids", "grid_rules", "image_words", "caption_straddle", "vectors", "scans", "images"):
    path = os.path.join(tmp.name, name + ".pdf")
    with open(path, "wb") as f:
        f.write(fx.ALL[name]())
    inputs.append((name, path))
t.check(len(inputs) > 20, "parity inputs: the walker's public fixtures and the grid/image fixtures", repr([n for n, _ in inputs]))

report = []
totals = {"pages": 0, "gridPagesEqual": 0, "gridPages": 0, "walkerGrids": 0, "images": 0, "imagesEqual": 0, "imagesWholeBoxDiffer": 0, "gridPagesTruncated": 0}
for name, path in inputs:
    pages = min(page_count(path), args.max_pages)
    if pages == 0:
        continue
    w = walker_side(path, pages)
    f = facts_side(path, pages)
    if w is None:
        t.check(False, "%s: pdftotext ran" % name)
        continue
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
        same = fp["complete"] and wp["grids"] == fp["grids"]
        key = (name, n, "grids")
        if not fp["complete"]:
            totals["gridPagesTruncated"] += 1
            t.check(True, "%s p%d grids: gridsTruncated (unknown to a reader; not compared)" % (name, n))
        elif key in KNOWN:
            t.check(not same, "%s p%d grids: known difference still differs (%s)" % (name, n, KNOWN[key]), repr(row))
        else:
            t.check(same, "%s p%d grids: facts %r == walker %r" % (name, n, fp["grids"], wp["grids"]), repr(fp["raw"]))
        totals["gridPagesEqual"] += same and bool(wp["grids"] or fp["raw"])
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
if args.json:
    with open(args.json, "w") as out:
        json.dump({"totals": totals, "pages": report}, out, indent=1)
t.finish()
