"""Robustness of `pdftomarkdown --math` on real PDFs, against the same converter without it.

    python3 -I scripts/math_eval.py --manifest M.json --splits S.json --split development \
        --pdftotext PATH --converter target/release/pdftomarkdown [--out results.json] [--runs 3]

For each file of the split (manifest `files[].id/path`, splits `splits[id]`), runs the converter
with and without `--math` (`--html` both times) and reports:
- prose words (3+ letters, outside `$...$`, plus words inside `\\text{...}` and operator names)
  lost and duplicated with --math, as a multiset difference against the plain output; with
  `--dict FILE` only dictionary words count, so math garble in the plain output (`xlk`) does
  not;
- formulas inline / display / from ActualText / left uncertain, and the share of math glyphs
  that ended up in a formula (from the `--report` JSON);
- pages listed in mathUncertainPages;
- CPU (user+sys of the converter and its pdftotext) per page, median and p95 over files, with
  and without --math, as the median of `--runs` runs.
"""
import argparse, json, os, re, resource, statistics, subprocess, sys, unicodedata
from collections import Counter
from pathlib import Path

WORD = re.compile(r"[^\W\d_]{3,}")
MATH = re.compile(r"(?<!\\)\$\$.+?(?<!\\)\$\$|(?<!\\)\$(?:\\\$|[^$\n])+?(?<!\\)\$", re.S)
TEXT = re.compile(r"\\text\{([^{}]*)\}|\\(sin|cos|tan|log|exp|max|min|lim|sup|inf|det|arg|ker|deg|dim|gcd|hom|sinh|cosh|tanh|arcsin|arccos|arctan|liminf|limsup)\b")
DICT = None


def prose_words(md: str, math: bool) -> Counter:
    words = Counter()
    keep = (lambda w: w in DICT) if DICT is not None else (lambda w: True)
    if math:
        for m in MATH.finditer(md):
            for t in TEXT.findall(m.group(0)):
                words.update(w.lower() for w in WORD.findall(' '.join(t)) if keep(w.lower()))
        md = MATH.sub(' ', md)
    words.update(w.lower() for w in WORD.findall(unicodedata.normalize('NFC', md)) if keep(w.lower()))
    return words


def run(cmd, env):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    p = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=600)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
    return p, cpu


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--manifest', required=True)
    ap.add_argument('--splits', required=True)
    ap.add_argument('--split', default='development')
    ap.add_argument('--pdftotext', required=True)
    ap.add_argument('--converter', required=True)
    ap.add_argument('--out')
    ap.add_argument('--runs', type=int, default=3)
    ap.add_argument('--dict')
    args = ap.parse_args()
    global DICT
    if args.dict:
        DICT = {w.strip().lower() for w in open(args.dict, encoding='utf-8', errors='replace') if len(w.strip()) >= 3}
    if args.split not in ('development', 'calibration'):
        sys.exit('only development or calibration splits are run here')
    root = Path(args.manifest).resolve().parent
    files = json.loads(Path(args.manifest).read_text())['files']
    splits = json.loads(Path(args.splits).read_text())['splits']
    env = dict(os.environ, PDFTOTEXT=args.pdftotext)
    rows = []
    for f in files:
        if splits.get(f['id']) != args.split:
            continue
        pdf = str(root / f['path'])
        pages = int(f.get('pages_kept') or 1)
        plain_cpu, math_cpu = [], []
        for _ in range(args.runs):
            p0, c0 = run([args.converter, '--html', pdf], env)
            p1, c1 = run([args.converter, '--html', '--math', '--report', pdf], env)
            plain_cpu.append(c0)
            math_cpu.append(c1)
        report = {}
        for line in p1.stderr.splitlines():
            if line.startswith('{'):
                report = json.loads(line)
        m = report.get('math', {})
        a, b = prose_words(p0.stdout, False), prose_words(p1.stdout, True)
        lost, dup = a - b, b - a
        rows.append({
            'id': f['id'], 'producer': f.get('producer', ''), 'pages': pages,
            'inline': m.get('inline', 0), 'display': m.get('display', 0), 'actualText': m.get('actualText', 0),
            'uncertain': m.get('uncertain', 0), 'mathGlyphs': m.get('mathGlyphs', 0), 'glyphsLeft': m.get('glyphsLeft', 0),
            'uncertainPages': report.get('mathUncertainPages', []),
            'proseWords': sum(a.values()), 'lost': sum(lost.values()), 'duplicated': sum(dup.values()),
            'lostSample': [w for w, _ in lost.most_common(8)], 'dupSample': [w for w, _ in dup.most_common(8)],
            'cpuPlainPerPage': statistics.median(plain_cpu) / pages, 'cpuMathPerPage': statistics.median(math_cpu) / pages,
            'exit': [p0.returncode, p1.returncode],
        })
        print(f"{f['id']:40} f={rows[-1]['inline']+rows[-1]['display']:4} unc={rows[-1]['uncertain']:3} lost={rows[-1]['lost']:3} dup={rows[-1]['duplicated']:3}", flush=True)

    def pct(v, q):
        v = sorted(v)
        return v[min(len(v) - 1, int(q * len(v)))] if v else None
    per_page_plain = [r['cpuPlainPerPage'] for r in rows]
    per_page_math = [r['cpuMathPerPage'] for r in rows]
    mg = sum(r['mathGlyphs'] for r in rows)
    summary = {
        'split': args.split, 'files': len(rows), 'pages': sum(r['pages'] for r in rows),
        'formulas': sum(r['inline'] + r['display'] for r in rows), 'inline': sum(r['inline'] for r in rows),
        'display': sum(r['display'] for r in rows), 'actualText': sum(r['actualText'] for r in rows),
        'uncertain': sum(r['uncertain'] for r in rows),
        'mathGlyphShareInFormulas': round(1 - sum(r['glyphsLeft'] for r in rows) / mg, 4) if mg else None,
        'uncertainPages': sum(len(r['uncertainPages']) for r in rows),
        'proseWords': sum(r['proseWords'] for r in rows), 'lost': sum(r['lost'] for r in rows), 'duplicated': sum(r['duplicated'] for r in rows),
        'cpuMsPerPage': {'plain': {'median': round(1000 * pct(per_page_plain, 0.5), 2), 'p95': round(1000 * pct(per_page_plain, 0.95), 2)},
                         'math': {'median': round(1000 * pct(per_page_math, 0.5), 2), 'p95': round(1000 * pct(per_page_math, 0.95), 2)}},
        'nonzeroExit': sum(1 for r in rows if any(r['exit'])),
    }
    print(json.dumps(summary, indent=1))
    if args.out:
        Path(args.out).write_text(json.dumps({'summary': summary, 'rows': rows}, indent=1, ensure_ascii=False))


if __name__ == '__main__':
    main()
