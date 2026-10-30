#!/usr/bin/env python3
"""Score Markdown arms against wild-v2 generator-authored page Markdown with the frozen DocSlurp judge (v3.5).

  score.py cases --split development --levels L0,L1 > ids.txt
  score.py selfcheck --cases ids.txt
  score.py score --cases ids.txt --arm-dir runs/x [--arm-dir runs/y ...] [--json out.json]

Run with ~/.cache/docslurp/raster-eval-venv/bin/python. The judge code is imported read-only from the
scorer archive checkout (5f1cc3b0); nothing here changes its metrics. `headingF1` and `quality` are
harness-local summaries, reported beside the judge's own components.
"""
import argparse
import json
import os
import random
import re
import statistics
import sys
import unicodedata
from pathlib import Path

ARCHIVE = Path('/home/dan/worktrees/docslurp/scorer-html-tables/scripts/raster-eval')
DATASET = Path(os.environ.get('DOCSLURP_EVAL_DATASET', '/home/dan/data/docslurp-evals/pdfust-dev-v1/wild-v2'))
TEXT_LEVELS = ['L0', 'L1', 'L2', 'L8', 'L9']
sys.path.insert(0, str(ARCHIVE))
from preservation import MD, extract_markdown  # noqa: E402
from matrix import score as judge_score  # noqa: E402

COMPONENTS = {'content': 'contentTokenF1', 'text': 'textTokenF1', 'order': 'textOrderedSimilarity',
              'paragraphs': 'paragraphBoundaryF1', 'tables': 'markdownTablePartial', 'math': 'mathPartial',
              'headings': 'headingF1', 'halluc': 'hallucinatedTokenRate'}
QUALITY = ['content', 'order', 'paragraphs', 'tables', 'math', 'headings']


def catalog():
    return {e['id']: e for e in json.loads((DATASET / 'catalog.json').read_text())['entries']}


def level(cid):
    return re.search(r'-(L\d)_', cid).group(1)


def md_tables(markdown):
    tables, rows, row, cell = [], None, None, False
    for token in MD.parse(markdown):
        if token.type == 'table_open':
            rows = []
        elif token.type == 'table_close':
            tables.append(rows); rows = None
        elif token.type == 'tr_open':
            row = []
        elif token.type in ['td_open', 'th_open']:
            cell = True
        elif token.type == 'inline' and cell and row is not None:
            row.append(''.join(c.content if c.type in ['text', 'code_inline'] else ' ' if c.type in ['softbreak', 'hardbreak'] else '' for c in token.children or []).strip())
            cell = False
        elif token.type == 'tr_close' and row is not None and rows is not None:
            rows.append(row); row = None
    return tables


def literal_dollars(markdown):
    # wild-v2 gold carries math as Unicode text and currency as bare `$`. Escape `$` identically in the gold
    # and in every prediction so the judge never reads prices as inline LaTeX (it skips `\$`).
    return re.sub(r'(?<!\\)\$', r'\\$', markdown)


def gold_pages(case):
    """Indices and Markdown of pages whose live gold is the generator's truth (liveSource=truth).

    Other pages carry a PyMuPDF transcription (silver, engine-biased) or no text layer; they are not scored."""
    ref = json.loads((DATASET / case['reference']['path']).read_text())
    return [(i, p.get('liveMarkdown') or '') for i, p in enumerate(ref['pages'])
            if (p.get('variant') or {}).get('liveSource') == 'truth']


def reference(case):
    markdown = literal_dollars('\n\n'.join(md for _, md in gold_pages(case)).strip())
    equations, paragraphs, _ = extract_markdown(markdown)
    tables = md_tables(markdown)
    formulas = [v for v, fmt in equations if fmt == 'latex']
    return {'scorable': True, 'questions': [],
            'text': '\n'.join(paragraphs + [' '.join(c for row in t for c in row) for t in tables]),
            'prose': '\n'.join(paragraphs), 'paragraphs': paragraphs, 'formulas': formulas,
            'tables': [{'page': 1, 'rows': rows} for rows in tables],
            'groundTruth': {'complete': True, 'prosePresent': bool(paragraphs), 'adapter': 'wild-v2-liveMarkdown-truth@v1'},
            'expectedMarkdown': markdown}


def select_pages(case, prediction):
    """Prediction pages aligned with the scored gold pages. Arms separate pages with form feeds."""
    keep = [i for i, _ in gold_pages(case)]
    pages = prediction.split('\f')
    if len(pages) == case['pages'] + 1 and not pages[-1].strip():
        pages.pop()  # pdftotext terminates every page with a form feed
    if len(pages) != case['pages']:
        return None
    return '\n\n'.join(pages[i] for i in keep)


def headings(markdown):
    out = []
    for token in MD.parse(markdown):
        if token.type == 'heading_open':
            out.append([int(token.tag[1:]), None])
        elif token.type == 'inline' and out and out[-1][1] is None:
            out[-1][1] = ' '.join(unicodedata.normalize('NFKC', token.content).casefold().split())
    return [(lvl, t) for lvl, t in out if t]


def heading_f1(gold_md, pred_md):
    gold = [t for _, t in headings(gold_md)]
    pred = [t for _, t in headings(pred_md)]
    if not gold:
        return None
    from collections import Counter
    hits = sum((Counter(gold) & Counter(pred)).values())
    p = hits / len(pred) if pred else 0
    r = hits / len(gold)
    return 2 * p * r / (p + r) if p + r else 0.0


def score_case(ref, markdown):
    markdown = literal_dollars(markdown)
    result = judge_score(ref, {'markdown': markdown, 'text': markdown})
    result['headingF1'] = heading_f1(ref['expectedMarkdown'], markdown)
    row = {k: result.get(v) for k, v in COMPONENTS.items()}
    if not ref['tables']:
        row['tables'] = None
    vals = [row[k] for k in QUALITY if row[k] is not None]
    row['quality'] = statistics.mean(vals) if vals else None
    return row


_CAT = None


def score_file(cid, path):
    global _CAT
    if _CAT is None:
        _CAT = catalog()
    f = Path(path)
    case = _CAT[cid]
    raw = f.read_text() if f.exists() else ''
    md = select_pages(case, raw)
    row = score_case(reference(case), md if md is not None else raw.replace('\f', '\n\n'))
    row.update(case=cid, level=level(cid), cls=case['documentClass'], missing=not f.exists(), unpaged=md is None)
    return row


def summarize(rows):
    out = {}
    for k in list(COMPONENTS) + ['quality']:
        vals = [r[k] for r in rows if r.get(k) is not None]
        out[k] = round(statistics.mean(vals), 4) if vals else None
        out[k + 'N'] = len(vals)
    return out


def boot_ci(diffs, n=2000, seed=7):
    rng = random.Random(seed)
    means = sorted(statistics.mean(rng.choices(diffs, k=len(diffs))) for _ in range(n))
    return means[int(0.025 * n)], means[int(0.975 * n)]


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest='cmd', required=True)
    c = sub.add_parser('cases'); c.add_argument('--split', default='development'); c.add_argument('--levels', default=','.join(TEXT_LEVELS))
    s = sub.add_parser('selfcheck'); s.add_argument('--cases', required=True)
    sc = sub.add_parser('score'); sc.add_argument('--cases', required=True); sc.add_argument('--arm-dir', action='append', required=True)
    sc.add_argument('--json'); sc.add_argument('--by', default='level'); sc.add_argument('--jobs', type=int, default=6)
    args = ap.parse_args()
    cat = catalog()
    if args.cmd == 'cases':
        levels = set(args.levels.split(','))
        for cid, e in sorted(cat.items()):
            if (args.split == 'all' or e['split'] in args.split.split(',')) and level(cid) in levels and gold_pages(e):
                print(cid)
        return
    ids = Path(args.cases).read_text().split()
    if args.cmd == 'selfcheck':
        bad = 0
        for cid in ids:
            ref = reference(cat[cid])
            row = score_case(ref, ref['expectedMarkdown'])
            low = {k: v for k, v in row.items() if v is not None and k != 'halluc' and v < 0.999}
            if low:
                bad += 1
                print(cid, low)
        print(f'selfcheck: {len(ids) - bad}/{len(ids)} cases >= 0.999')
        return
    report = {}
    from concurrent.futures import ProcessPoolExecutor
    with ProcessPoolExecutor(max_workers=args.jobs) as pool:
        jobs = {d: [pool.submit(score_file, cid, str(Path(d) / f'{cid}.md')) for cid in ids] for d in args.arm_dir}
        results = {d: [f.result() for f in fs] for d, fs in jobs.items()}
    for d in args.arm_dir:
        rows = results[d]
        groups = {}
        for r in rows:
            groups.setdefault(r[args.by], []).append(r)
        report[d] = {'all': summarize(rows), 'groups': {g: summarize(v) for g, v in sorted(groups.items())}, 'rows': rows}
    names = list(report)
    keys = ['quality'] + list(COMPONENTS)
    print('arm'.ljust(44) + ''.join(k[:9].rjust(10) for k in keys))
    for d in names:
        a = report[d]['all']
        print(Path(d).name[:43].ljust(44) + ''.join((f'{a[k]:.4f}' if a[k] is not None else '-').rjust(10) for k in keys))
    if len(names) > 1:
        base = report[names[0]]['rows']
        for d in names[1:]:
            diffs = [b['quality'] - a['quality'] for a, b in zip(base, report[d]['rows']) if a['quality'] is not None and b['quality'] is not None]
            lo, hi = boot_ci(diffs)
            print(f'  {Path(d).name} - {Path(names[0]).name}: quality {statistics.mean(diffs):+.4f} [95% CI {lo:+.4f}, {hi:+.4f}] n={len(diffs)}')
    if args.json:
        Path(args.json).write_text(json.dumps(report, indent=1))


if __name__ == '__main__':
    main()
