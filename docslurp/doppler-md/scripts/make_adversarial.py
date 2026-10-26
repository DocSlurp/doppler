#!/usr/bin/env python3
"""Synthetic hostile doppler XHTML pages for super-linear behaviour (see PERF-ALGO.md).

    python3 -I scripts/make_adversarial.py OUT_DIR [SCALE]
    python3 -I scripts/make_adversarial.py --fuzz OUT_DIR SEED COUNT

SCALE (default 1.0) multiplies every page's element count. --fuzz writes COUNT random
documents (snapped coordinates, shared baselines, rules, styles, extreme values) for
differential runs of two converter builds.
"""
import os
import random
import sys


def word(x0, y0, x1, y1, text, size=10.0):
    return (f'<word xMin="{x0:.6f}" yMin="{y0:.6f}" xMax="{x1:.6f}" yMax="{y1:.6f}" fontName="Serif" '
            f'fontSize="{size:.6f}" spaceAfter="1" color="#000000" baseline="{y1 - 2:.6f}">{text}</word>')


def line(words):
    return '<flow><block><line>' + ''.join(words) + '</line></block></flow>\n'


def page(w, h, body, rules=''):
    return f'<page width="{w:.6f}" height="{h:.6f}">\n{body}{rules}</page>\n'


def doc(pages):
    return '<?xml version="1.0" encoding="UTF-8"?>\n<html><body><doc>\n' + ''.join(pages) + '</doc></body></html>\n'


def twocol(n):
    """n rows of a short key/value list in two separate lines per row: a table-like gutter that
    every band re-follows to the bottom of the page."""
    out = []
    for i in range(n):
        y = 50 + 12 * i
        out.append(line([word(50, y, 70, y + 10, f'k{i}')]))
        out.append(line([word(300, y, 330, y + 10, f'v{i}')]))
    return doc([page(612, 12 * n + 100, ''.join(out))])


def furniture(n):
    """n page-number-like lines inside the header band, on distinct baselines."""
    h = 12 * n * 20
    out = [line([word(300, 20 + 12 * i, 310, 30 + 12 * i, str(i % 90 + 1))]) for i in range(n)]
    out.append(line([word(72, h / 2, 200, h / 2 + 10, 'body text here')]))
    return doc([page(612, h, ''.join(out))])


def grid(rows, cols):
    """rows x cols short cells, each its own line: many gutters in every band."""
    out = []
    for r in range(rows):
        y = 50 + 12 * r
        for c in range(cols):
            x = 20 + 40 * c
            out.append(line([word(x, y, x + 15, y + 10, f'{r % 10}{c % 10}')]))
    return doc([page(40 * cols + 40, 12 * rows + 100, ''.join(out))])


def rules(n):
    """a ruled table of n rows (horizontal rules every 14pt, three vertical rules) with text, plus
    n isolated short rules spread down a second page."""
    out, rs = [], []
    h = 14 * n + 100
    for i in range(n):
        y = 50 + 14 * i
        rs.append(f'<rule xMin="50" yMin="{y:.2f}" xMax="550" yMax="{y + 0.5:.2f}" color="#000000"/>\n')
        if i + 1 < n:
            out.append(line([word(60, y + 2, 90, y + 12, f'a{i}')]))
            out.append(line([word(260, y + 2, 290, y + 12, f'{i}.5')]))
    for x in (50, 250, 550):
        rs.append(f'<rule xMin="{x:.2f}" yMin="50" xMax="{x + 0.5:.2f}" yMax="{50 + 14 * (n - 1):.2f}" color="#000000"/>\n')
    p1 = page(612, h, ''.join(out), ''.join(rs))
    rs2 = [f'<rule xMin="{50 + i % 7:.2f}" yMin="{50 + 100 * i:.2f}" xMax="{350 + i % 7:.2f}" yMax="{50.5 + 100 * i:.2f}" color="#000000"/>\n'
           for i in range(n)]
    p2 = page(612, 100 * n + 100, line([word(72, 40, 200, 50, 'spread rules')]), ''.join(rs2))
    return doc([p1, p2])


def ruled_many(n):
    """n small ruled tables (three rules, one vertical) with a line of text between them."""
    out, rs = [], []
    for i in range(n):
        y = 50 + 120 * i
        for k in range(3):
            rs.append(f'<rule xMin="50" yMin="{y + 14 * k}" xMax="550" yMax="{y + 14 * k + 0.5}" color="#000000"/>\n')
        rs.append(f'<rule xMin="300" yMin="{y}" xMax="300.5" yMax="{y + 28}" color="#000000"/>\n')
        for k in range(2):
            out.append(line([word(60, y + 14 * k + 2, 90, y + 14 * k + 12, f'a{i}')]))
            out.append(line([word(310, y + 14 * k + 2, 340, y + 14 * k + 12, f'b{i}')]))
        out.append(line([word(60, y + 60, 200, y + 70, 'between tables text')]))
    return doc([page(612, 120 * n + 100, ''.join(out), ''.join(rs))])


def caption_rules(n):
    """n full-width rules with one single-cell line between each pair and a vertical rule that
    crosses none of them: every rule gap is tested as a caption band."""
    out, rs = [], []
    for i in range(n):
        y = 50 + 14 * i
        rs.append(f'<rule xMin="50" yMin="{y}" xMax="550" yMax="{y + 0.5}" color="#000000"/>\n')
        out.append(line([word(60, y + 2, 90, y + 12, f'a{i}')]))
    rs.append('<rule xMin="600" yMin="50" xMax="600.5" yMax="80" color="#000000"/>\n')
    return doc([page(612, 14 * n + 100, ''.join(out), ''.join(rs))])


def sameline(n):
    """n one-word lines on one baseline, each just right of the last."""
    out = [line([word(10 + 4 * i, 50, 13 + 4 * i, 60, 'x')]) for i in range(n)]
    return doc([page(4 * n + 100, 792, ''.join(out))])


def fuzz_doc(rng):
    pages = []
    for _ in range(rng.randint(1, 4)):
        w, h = 612.0, rng.choice([792.0, 792.0, 3000.0])
        cols = [rng.choice([40, 60, 72, 200, 300, 320, 400]) for _ in range(rng.randint(1, 5))]
        pitch = rng.choice([10, 12, 14, 24])
        body = []
        for _ in range(rng.randint(0, 120)):
            x = rng.choice(cols) + rng.choice([0, 0, 0, 0.5, 3, 12])
            y = rng.randint(0, int(h / pitch)) * pitch + rng.choice([0, 0, 0, 1, 4])
            size = rng.choice([10.0, 10.0, 10.0, 8.0, 12.0, 14.0, 20.0, 0.0])
            ws = []
            for k in range(rng.randint(1, 6)):
                t = rng.choice(['a', 'word', 'Text.', '12', '3.4', '•', '-', '1.', '(a)', '$5', 'x' * 30, 'Page 3',
                                'AB', 'ok;', 'سلام', '中文', '&amp;', 'é'])
                x1 = x + max(1, len(t)) * size * 0.5
                font = rng.choice(['Serif', 'Serif-Bold', 'Sans-Italic'])
                color = rng.choice(['#000000', '#000000', '#ff0000', '#777777', ''])
                base = y + size * 0.8
                if rng.random() < 0.02:
                    x1, base = rng.choice([(1e308, base), (x1, 1.7e308), (x1, -0.0)])
                ws.append(f'<word xMin="{x}" yMin="{y}" xMax="{x1}" yMax="{y + max(size, 1)}" fontName="{font}" '
                          f'fontSize="{size}" spaceAfter="{rng.choice([0, 1, 1])}" color="{color}" '
                          f'baseline="{base}">{t}</word>')
                x = x1 + rng.choice([1, 2, 3, 3, 10, 30])
            body.append(f'<flow><block><line>{"".join(ws)}</line></block></flow>\n'
                        if rng.random() < 0.7 else f'<block><line>{"".join(ws)}</line></block>\n')
        rules = []
        if rng.random() < 0.6:
            x0, x1 = rng.choice(cols), rng.choice(cols) + 250
            ys = sorted(rng.randint(0, int(h / pitch)) * pitch - 1 for _ in range(rng.randint(2, 30)))
            for y in ys:
                tag = 'box' if rng.random() < 0.1 else 'rule'
                rules.append(f'<{tag} xMin="{x0}" yMin="{y}" xMax="{x1}" yMax="{y + 0.5}" color="#000000"/>\n')
            for x in rng.sample(cols + [x1], k=min(len(cols) + 1, rng.randint(0, 3))):
                rules.append(f'<rule xMin="{x - 2}" yMin="{ys[0]}" xMax="{x - 1.5}" yMax="{ys[-1]}" color="#000000"/>\n')
        pages.append(page(w, h, ''.join(body), ''.join(rules)))
    return doc(pages)


def main():
    if sys.argv[1] == '--fuzz':
        out, seed, count = sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
        os.makedirs(out, exist_ok=True)
        rng = random.Random(seed)
        for i in range(count):
            with open(os.path.join(out, f'fuzz-{seed}-{i}.raw'), 'w', encoding='utf-8') as f:
                f.write(fuzz_doc(rng))
        return
    out = sys.argv[1]
    s = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
    os.makedirs(out, exist_ok=True)
    pages = {
        'adv-twocol': twocol(int(10000 * s)),
        'adv-furniture': furniture(int(20000 * s)),
        'adv-grid': grid(int(300 * s), 30),
        'adv-rules': rules(int(3000 * s)),
        'adv-sameline': sameline(int(20000 * s)),
        'adv-ruled-many': ruled_many(int(3000 * s)),
        'adv-caption-rules': caption_rules(int(20000 * s)),
    }
    for name, x in pages.items():
        with open(os.path.join(out, name + '.raw'), 'w', encoding='utf-8') as f:
            f.write(x)


if __name__ == '__main__':
    main()
