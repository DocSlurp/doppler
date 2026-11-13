"""Score heading-level arms with the v3.6 judge on the 254 judged wild-v2 cases (doppler-md over
the saved golden XHTML), plus two harness-local level measures the judge does not have (its
headingF1 ignores levels): headingLevelF1 (a heading counts when its text and level both match
the authored reference) and headingLevelAcc (of the headings whose text matches, the share at
the reference's level). Usage:
DOPPLER_GOLDEN=DIR score_headings.py DOPPLER_MD OUT.json
Run with ~/.cache/docslurp/raster-eval-venv/bin/python -I."""
import importlib.util, json, multiprocessing, os, subprocess, sys
from collections import Counter
from pathlib import Path
D = Path(__file__).parent
spec = importlib.util.spec_from_file_location('judge36', D.parent / 'html-spans-20261030' / 'judge36.py'); judge = importlib.util.module_from_spec(spec); spec.loader.exec_module(judge)
DM, OUT = sys.argv[1], sys.argv[2]
RAW = Path(os.environ.get('DOPPLER_GOLDEN', '/home/dan/data/docslurp-evals/doppler-md-golden')) / 'raw'
ARMS = {
    'size': [],
    'rank': ['--heading-levels', 'rank'],
    'stream-size': ['--stream'],
    'stream-rank': ['--stream', '--heading-levels', 'rank'],
}
cat = judge.catalog()
ids = sorted(p.stem for p in RAW.glob('*.raw'))
ids = [i for i in ids if i in cat and judge.gold_pages(cat[i])]
assert len(ids) == 254, len(ids)


def level_scores(gold_md, pred_md):
    gold, pred = judge.headings(gold_md), judge.headings(pred_md)
    if not gold:
        return None, None
    hits = sum((Counter(gold) & Counter(pred)).values())
    p = hits / len(pred) if pred else 0
    r = hits / len(gold)
    f1 = 2 * p * r / (p + r) if p + r else 0.0
    by_text = {}
    for lvl, t in gold:
        by_text.setdefault(t, lvl)
    matched = [(by_text[t], lvl) for lvl, t in pred if t in by_text]
    acc = sum(a == b for a, b in matched) / len(matched) if matched else None
    return f1, acc


def one(cid):
    case = cat[cid]; ref = judge.reference(case); rows, memo = {}, {}
    for arm, args in ARMS.items():
        md = subprocess.run([DM, *args, str(RAW / f'{cid}.raw')], capture_output=True, check=True).stdout.decode()
        sel = judge.select_pages(case, md); assert sel is not None, (cid, arm)
        if sel not in memo:
            row = judge.score_case(ref, sel)
            row['levelF1'], row['levelAcc'] = level_scores(ref['expectedMarkdown'], judge.literal_dollars(sel))
            memo[sel] = row
        rows[arm] = dict(memo[sel], case=cid, level=judge.level(cid), changed=sel != next(iter(memo)))
    return rows


def summarize(rows):
    out = judge.summarize(rows)
    for k in ['levelF1', 'levelAcc']:
        vals = [r[k] for r in rows if r.get(k) is not None]
        out[k] = round(sum(vals) / len(vals), 4) if vals else None
        out[k + 'N'] = len(vals)
    return out


if __name__ == '__main__':
    with multiprocessing.get_context('fork').Pool(6) as pool:
        out = list(pool.imap_unordered(one, ids))
    res = {arm: summarize([r[arm] for r in out]) for arm in ARMS}
    keys = ['quality', 'content', 'tables', 'headings', 'levelF1', 'levelAcc', 'halluc']
    print('| arm | ' + ' | '.join(keys) + ' | cases changed vs size |')
    print('|---|' + '---:|' * (len(keys) + 1))
    for arm in ARMS:
        print(f'| {arm} | ' + ' | '.join(str(res[arm][k]) for k in keys) + f" | {sum(r[arm]['changed'] for r in out)} |")
    json.dump({'summary': res, 'rows': out}, open(OUT, 'w'))
