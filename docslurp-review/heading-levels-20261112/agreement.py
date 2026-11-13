"""Heading agreement with the whole-document run on long PDFs: for each heading mode (size,
rank), the whole run (no window) is the reference, and each arm (--stream, and --stream with a
deliberately short body sample) is compared with it page by page. A reference heading agrees
when the arm's same page has a heading with the same text at the same level; `found` counts
the same text at any level. Usage:
agreement.py DOPPLER_MD XHTML_DIR OUT.json
over saved `pdftotext -bbox-layout -wordmeta -rules -compact-layout` XHTML (a local corpus,
not committed; only aggregates are recorded)."""
import json, re, subprocess, sys
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

DM, XD, OUT = sys.argv[1], Path(sys.argv[2]), sys.argv[3]
MODES = {'size': [], 'rank': ['--heading-levels', 'rank']}
ARMS = {'stream': ['--stream'], 'stream-bp2': ['--stream', '--body-pages', '2']}
H = re.compile(r'^(#{1,6}) (.+)$')


def run(args, f):
    p = subprocess.run([DM, '--report', *args, str(f)], capture_output=True, check=True)
    rep = json.loads(p.stderr.decode().strip().splitlines()[-1])
    return p.stdout.decode().split('\f'), rep


def heads(page):
    out = []
    for line in page.split('\n'):
        m = H.match(line)
        if m:
            out.append((len(m.group(1)), ' '.join(m.group(2).casefold().split())))
    return out


def compare(ref, arm):
    n = same = found = extra = 0
    for rp, ap in zip(ref, arm):
        r, a = heads(rp), heads(ap)
        n += len(r)
        same += sum((Counter(r) & Counter(a)).values())
        found += sum((Counter(t for _, t in r) & Counter(t for _, t in a)).values())
        extra += max(0, len(a) - sum((Counter(t for _, t in r) & Counter(t for _, t in a)).values()))
    return dict(ref_headings=n, same_level=same, same_text=found, arm_extra=extra)


def one(f):
    row = {'doc': f.stem}
    for mode, margs in MODES.items():
        ref, rrep = run(margs, f)
        row[mode] = {'body': rrep['body_size'], 'levels': dict(Counter(l for p in ref for l, _ in heads(p)))}
        for arm, aargs in ARMS.items():
            out, arep = run(margs + aargs, f)
            row[mode][arm] = dict(compare(ref, out), body=arep['body_size'])
    return row


if __name__ == '__main__':
    files = sorted(XD.glob('*.xhtml'))
    with ThreadPoolExecutor(4) as ex:
        rows = list(ex.map(one, files))
    rows = [r for r in rows if r['size']['stream']['ref_headings'] or r['rank']['stream']['ref_headings']]
    print('| document | body whole / bp2 | arm | size: same level / text / headings | rank: same level / text / headings |')
    print('|---|---|---|---:|---:|')
    tot = {}
    for r in rows:
        for arm in ARMS:
            cells = []
            for mode in MODES:
                c = r[mode][arm]
                t = tot.setdefault((mode, arm), Counter()); t.update({k: c[k] for k in ['ref_headings', 'same_level', 'same_text', 'arm_extra']})
                pct = lambda k: f"{100 * c[k] / c['ref_headings']:.1f}%" if c['ref_headings'] else '-'
                cells.append(f"{pct('same_level')} / {pct('same_text')} / {c['ref_headings']}")
            print(f"| {r['doc'][:12]} | {r['size']['body']} / {r['size']['stream-bp2']['body']} | {arm} | " + ' | '.join(cells) + ' |')
    print()
    for (mode, arm), t in sorted(tot.items()):
        print(f"{mode} {arm}: same level {100 * t['same_level'] / t['ref_headings']:.2f}%  same text {100 * t['same_text'] / t['ref_headings']:.2f}%  of {t['ref_headings']}  arm extra {t['arm_extra']}")
    json.dump({'rows': rows, 'totals': {f'{m} {a}': dict(t) for (m, a), t in tot.items()}}, open(OUT, 'w'))
