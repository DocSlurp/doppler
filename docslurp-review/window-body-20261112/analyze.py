"""Metrics for the window/body sweep. Usage: analyze.py XHTML_DIR OUT_DIR > table.
(a) line disagreement with the whole-document run; (b) leaked running heads/footers found by
geometry in the XHTML (not by the converter's rule); body sizes; CPU and peak RSS."""
import html, json, re, statistics, sys, unicodedata
from collections import Counter, defaultdict
from pathlib import Path
XD, OUT = Path(sys.argv[1]), Path(sys.argv[2])
PAGE = re.compile(r'<page\b[^>]*?height="([^"]+)"')
LINE = re.compile(r'<line\b[^>]*?yMin="([^"]+)"[^>]*?yMax="([^"]+)"[^>]*>(.*?)</line>', re.S)
WORD = re.compile(r'<word\b[^>]*>(.*?)</word>', re.S)
BAND = 0.12   # wider than the converter's 0.09: an independent band
SPAN = 5      # repeated on >= 3 band lines of the 11 pages around
def norm(t):
    t = unicodedata.normalize('NFKC', html.unescape(t)).lower()
    t = re.sub(r'\d+', '#', t)
    t = re.sub(r'[^\w#]+', ' ', t)
    return ' '.join(t.split())
def furniture_truth(xhtml, SPAN):
    """Per page: set of normalised keys of band lines repeated nearby (running heads/footers)."""
    text = xhtml.read_text(errors='replace')
    starts = [m.start() for m in PAGE.finditer(text)] + [len(text)]
    heights = [float(m.group(1)) for m in PAGE.finditer(text)]
    band = []
    for i in range(len(heights)):
        keys = set()
        for m in LINE.finditer(text, starts[i], starts[i + 1]):
            mid = (float(m.group(1)) + float(m.group(2))) / 2
            if mid < BAND * heights[i] or mid > (1 - BAND) * heights[i]:
                k = norm(' '.join(WORD.findall(m.group(3))))
                # page numbers alone are not counted: every arm drops them by pattern
                if len(k.replace('#', '').replace(' ', '')) >= 3:
                    keys.add(k)
        band.append(keys)
    truth = []
    for i, keys in enumerate(band):
        near = Counter(k for j in range(max(0, i - SPAN), min(len(band), i + SPAN + 1)) for k in band[j])
        truth.append({k for k in keys if near[k] >= 3})
    return truth
def md_lines(path):
    return [p.split('\n') for p in path.read_text(errors='replace').split('\f')]
def leaks(pages, truth):
    n = 0
    for lines, keys in zip(pages, truth):
        if not keys:
            continue
        got = {norm(re.sub(r'^[#>*\-\s|]+', '', l)) for l in lines}
        n += sum(1 for k in keys if k in got)
    return n
def tokens(lines):
    return Counter(t for l in lines for t in norm(re.sub(r'<[^>]+>', ' ', l)).split())
def lost_words(a, b, truth):
    """Words of the whole run missing from the arm, not counting the page's furniture keys."""
    lost = tot = 0
    for pa, pb, keys in zip(a, b, truth):
        ta, tb = tokens(pa), tokens(pb)
        gone = ta - tb
        gone -= Counter(t for k in keys for t in k.split())
        lost += sum(gone.values()); tot += sum(ta.values())
    return lost, tot
def disagree(a, b):
    tot = diff = pages = 0
    for pa, pb in zip(a, b):
        ca, cb = Counter(l for l in pa if l.strip()), Counter(l for l in pb if l.strip())
        m = max(sum(ca.values()), sum(cb.values()))
        inter = sum((ca & cb).values())
        tot += m; diff += m - inter; pages += (ca != cb)
    return diff, tot, pages
rows = defaultdict(dict)
for d in sorted(p for p in OUT.iterdir() if p.is_dir()):
    truth = furniture_truth(XD / f'{d.name}.xhtml', 5)
    truth16 = furniture_truth(XD / f'{d.name}.xhtml', 16)
    whole = md_lines(d / 'whole.md')
    for md in sorted(d.glob('*.md')):
        arm = md.stem
        pages = md_lines(md)
        diff, tot, pdiff = disagree(whole, pages)
        u, s, rss = (d / f'{arm}.cpu').read_text().split()
        rep = json.loads((d / f'{arm}.rep').read_text().splitlines()[0])
        lost, words = lost_words(whole, pages, truth16)
        rows[arm][d.name] = dict(diff=diff, tot=tot, pdiff=pdiff, pages=len(pages), leaks=leaks(pages, truth),
                                 leaks16=leaks(pages, truth16), truth16=sum(map(len, truth16)), lost=lost, words=words,
                                 truth=sum(map(len, truth)), cpu=float(u) + float(s), rss=int(rss) / 1024,
                                 body=rep['body_size'], wbody=None)
json.dump(rows, open(OUT / 'metrics.json', 'w'))
docs = sorted(rows['whole'])
for arm in rows:
    for doc in docs:
        rows[arm][doc]['wbody'] = rows['whole'][doc]['body']
def agg(arm):
    r = rows[arm].values()
    diff = sum(x['diff'] for x in r); tot = sum(x['tot'] for x in r)
    per_doc = [x['diff'] / max(x['tot'], 1) for x in r]
    return dict(lines_diff=100 * diff / tot, worst_doc=100 * max(per_doc), docs_over_5=sum(p > 0.05 for p in per_doc),
                pages_diff=sum(x['pdiff'] for x in r), leaks=sum(x['leaks'] for x in r), leaks16=sum(x['leaks16'] for x in r),
                lost=100 * sum(x['lost'] for x in r) / sum(x['words'] for x in r),
                body_wrong=sum(x['body'] != x['wbody'] for x in r),
                cpu=sum(x['cpu'] for x in r), rss_max=max(x['rss'] for x in r), rss_med=statistics.median(x['rss'] for x in r))
print(f"docs {len(docs)}, pages {sum(rows['whole'][d]['pages'] for d in docs)}, furniture lines (±5 / ±16) {sum(rows['whole'][d]['truth'] for d in docs)} / {sum(rows['whole'][d]['truth16'] for d in docs)}")
print('| arm | lines differing % | worst doc % | docs >5% | pages differing | leaked heads/footers ±5 / ±16 | whole-run words lost % | body size != whole | CPU s | peak RSS max / median MiB |')
print('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|')
for arm in rows:
    a = agg(arm)
    print(f"| {arm} | {a['lines_diff']:.2f} | {a['worst_doc']:.1f} | {a['docs_over_5']} | {a['pages_diff']} | {a['leaks']} / {a['leaks16']} | {a['lost']:.3f} | {a['body_wrong']} | {a['cpu']:.2f} | {a['rss_max']:.0f} / {a['rss_med']:.0f} |")
