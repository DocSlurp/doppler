#!/usr/bin/env python3
"""Seeded differential fuzz of the release doppler-md binary against doppler_md.py, aimed at
Unicode, entities, malformed numbers and the work limits.

    python3 -I scripts/difffuzz.py --py PATH/doppler_md.py [--bin target/release/doppler-md]
                                   [--seed 1] [--count 1000] [--keep DIR]

Each document mixes Turkish dotted/dotless i, Greek final sigma, ligatures, Arabic and Hebrew,
combining marks, NBSP, line and paragraph separators, astral characters, Unicode 17 letters,
numeric and named entities (valid, invalid, unterminated), backslashes and pipes, Unicode case in
font names and repeated header lines, malformed and non-finite numbers, missing attributes,
truncation and invalid UTF-8. (Huge finite values such as 1e308 are left out: the reference raises
OverflowError on them where the port clamps, see make_adversarial.py --fuzz.) About a fifth of the documents run with tight work limits so the
plain-line fallback is compared too. Markdown must be byte-identical, and the parse counts and
degraded pages of the two reports must agree. Run with `python3 -I` (the reference is loaded
from --py by path, never from the current directory).
"""
import argparse
import importlib.util
import json
import os
import random
import subprocess
import sys

TEXTS = [
    'İstanbul', 'ISTANBUL', 'ıi', 'İı', 'ΟΔΥΣΣΕΥΣ', 'ΑΣ', "ΑΣ'", 'Σ.', 'ΣΑ', 'σς', 'ﬁnance', 'ﬂow', 'ﬃ', 'ﬆ',
    'سلام', 'مرحبا', '١٢٣', 'שלום', 'עולם', '٣٫٥', 'é', 'ǟ', 'ͅ', 'ño',
    'a b', ' ', 'x y', ' ', 'p\u0085q', ' ', '𝐀𝐁', '😀', '𞤀𞤢', '𐐀', '\U0010fffd',
    '꟎꟏', 'Ƛ', 'ǅ', 'ẞ', 'K', 'Ω', 'Å', '&amp;', '&nbsp;', '&notin;', '&not', '&notit;',
    '&NotEqualTilde;', '&amp', '&#10;', '&#13;', '&#9;', '&#x2028;', '&#133;', '&#128;', '&#0;', '&#xD800;',
    '&#1114112;', '&#x1F600;', '&#99999999;', '&#x;', '&#;', '&lt;b&gt;', '<b>', 'a\\', '\\|', '|', 'a|b',
    '\\\\', '`code`', '[x](javascript:y)', 'word', 'Text.', '12', '3.4', '•', '-', '1.', '(a)', '$5',
    'Page 3', 'PAGE ٣', 'page 7 of 9', 'ok;', '中文', 'x' * 20, 'é', 'Ⅻ', 'ⅰ', '½', '²',
]
FONTS = ['BLAC\u212a', 'SEMIBOLD', 'Serif', 'Serif-Bold', 'Sans-Italic', 'İTALIC', 'Serif-İtalic', 'ΣBOLD', 'BOLDΣ', 'Ｂｏｌｄ', 'SERIF,B',
         'Serif,I', 'Times-Oblique', 'Arial-Black', 'Heavyİ', 'semiKold', '']
NUMBERS = ['nan', 'inf', '-inf', 'Infinity', '1e999', '', 'abc', ' 5 ', '1_0', '1__0', '_1', '١٢', '0x10',
           '1e5', '.5', '5.', '+3', '-0.0', ' 5']
# header lines repeated across pages in case variants: furniture keys are str.lower()ed, so these
# pairs exercise full case mappings, final sigma, and code points whose case differs between
# Unicode 16 (Python 3.14) and 17 (Rust 1.99)
HEADER_FAMILIES = [
    ['Annual Report 2024', 'ANNUAL REPORT 2025', 'Annual report 2026'],
    ['İSTANBUL REVIEW', 'istanbul review', 'i\u0307stanbul review', 'ıstanbul review'],
    ['ΟΔΥΣΣΕΥΣ', 'οδυσσευς', 'οδυσσευσ', 'ΟΔΥΣΣΕΥΣ\u0301'],
    ['\ua7ce HEADER', '\ua7cf header', '\ua7dc HEADER', '\u019b header'],
    ['STRASSE 1', 'Straße 2', 'strasse 3', 'STRAẞE 4'],
    ['ǅ title', 'ǆ TITLE', 'Ǆ title'],
    ['\U0001e900 page', '\U0001e922 PAGE'],
]


def num(rng, v):
    return rng.choice(NUMBERS) if rng.random() < 0.03 else f'{v:g}'


def word(rng, x0, y0, x1, y1, text, size, font, base):
    attrs = [('xMin', num(rng, x0)), ('yMin', num(rng, y0)), ('xMax', num(rng, x1)), ('yMax', num(rng, y1)),
             ('fontName', font), ('fontSize', num(rng, size)), ('spaceAfter', rng.choice(['0', '1', '1'])),
             ('color', rng.choice(['#000000', '#000000', '#ff0000', '#777777', ''])), ('baseline', num(rng, base))]
    if rng.random() < 0.02:
        attrs.pop(rng.randrange(4))  # a missing coordinate
    return '<word ' + ' '.join(f'{k}="{v}"' for k, v in attrs) + f'>{text}</word>'


def line_words(rng, x, y, size, n, table):
    ws = []
    for _ in range(n):
        t = ''.join(rng.choice(TEXTS) for _ in range(rng.choice([1, 1, 1, 2, 3])))
        x1 = x + max(1, len(t)) * size * 0.5
        ws.append(word(rng, x, y, x1, y + max(size, 1), t, size, rng.choice(FONTS), y + size * 0.8))
        x = x1 + (rng.choice([12, 30, 60]) if table else rng.choice([1, 2, 3, 3, 10]))
    return ''.join(ws)


def doc(rng):
    pages = []
    family = rng.choice(HEADER_FAMILIES)
    for pi in range(rng.randint(1, 4)):
        h = rng.choice([792.0, 792.0, 3000.0])
        cols = [rng.choice([40, 60, 72, 200, 300, 320, 400]) for _ in range(rng.randint(1, 4))]
        pitch = rng.choice([10, 12, 14])
        body = []
        if rng.random() < 0.7:  # a header line repeated with case variants (furniture keys use lower())
            t = rng.choice(family)
            body.append(f'<flow><block><line>{word(rng, 72, 20, 300, 30, t.replace(" ", "&#32;"), 10.0, "Serif", 28)}</line></block></flow>\n')
        table = rng.random() < 0.4
        for _ in range(rng.randint(0, 60)):
            x = rng.choice(cols) + rng.choice([0, 0, 0, 0.5, 3])
            y = 60 + rng.randint(0, int((h - 120) / pitch)) * pitch + rng.choice([0, 0, 0, 1, 4])
            size = rng.choice([10.0, 10.0, 10.0, 8.0, 12.0, 14.0, 20.0, 0.0])
            ws = line_words(rng, x, y, size, rng.randint(1, 5), table)
            r = rng.random()
            if r < 0.6:
                body.append(f'<flow><block><line>{ws}</line></block></flow>\n')
            elif r < 0.9:
                body.append(f'<block><line>{ws}</line></block>\n')
            elif r < 0.95:
                body.append(f'{ws}\n')  # words outside any line
            else:
                body.append(f'<line>{ws}\n')  # a line that never closes
        rules = []
        if rng.random() < 0.3:
            for _ in range(rng.randint(1, 8)):
                y = rng.randint(60, int(h) - 60)
                rules.append(f'<rule xMin="{num(rng, 40)}" yMin="{num(rng, y)}" xMax="{num(rng, 560)}" '
                             f'yMax="{num(rng, y + 0.5)}" color="#000000"/>\n')
        dims = f'width="{num(rng, 612)}" height="{num(rng, h)}"'
        pages.append(f'<page {dims}>\n{"".join(body)}{"".join(rules)}' + ('</page>\n' if rng.random() < 0.95 else ''))
    x = '<?xml version="1.0" encoding="UTF-8"?>\n<html><body><doc>\n' + ''.join(pages) + '</doc></body></html>\n'
    data = x.encode('utf-8')
    r = rng.random()
    if r < 0.05:
        data = data[:rng.randrange(len(data) + 1)]  # truncated
    elif r < 0.08:
        k = rng.randrange(len(data) + 1)
        data = data[:k] + rng.choice([b'\xff', b'\xc3', b'\xe2\x80', b'\xed\xa0\x80', b'\xf4\x90\x80\x80']) + data[k:]
    elif r < 0.09:
        data = rng.choice([b'', b'plain text', b'<html><p>not doppler</p></html>'])
    return data


def tight_params(rng):
    if rng.random() >= 0.2:
        return {}
    return rng.choice([{'max_page_work': rng.randint(1, 400)}, {'max_doc_work': rng.randint(1, 600)},
                       {'max_page_lines': rng.randint(0, 30)}, {'max_page_rules': rng.randint(0, 3)}])


def rust_counts(j):
    s = j['words_skipped']
    return {'pages': j['pages'], 'lines': j['lines'], 'words': j['words'], 'missing_coord': s['missing_coord'],
            'bad_number': s['bad_number'], 'outside_line': s['outside_line'], 'unclosed_line': s['unclosed_line'],
            'eof': j['unclosed_line_at_eof'], 'rules_skipped': j['rules_skipped'], 'unclosed_pages': j['unclosed_pages'],
            'degraded': [(d['page'] - 1, d['reason']) for d in j['degraded']]}


def py_counts(r):
    return {'pages': r['pages'], 'lines': r['lines'], 'words': r['words'], 'missing_coord': r['words_missing_coord'],
            'bad_number': r['words_bad_number'], 'outside_line': r['words_outside_line'],
            'unclosed_line': r['words_unclosed_line'], 'eof': r['unclosed_line_at_eof'],
            'rules_skipped': r['rules_skipped'], 'unclosed_pages': max(0, r['pages'] - r['page_closes']),
            'degraded': [tuple(d) for d in r['degraded']]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--py', required=True)
    ap.add_argument('--bin', default=os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'target', 'release', 'doppler-md'))
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--count', type=int, default=1000)
    ap.add_argument('--keep', help='write failing inputs here')
    a = ap.parse_args()
    spec = importlib.util.spec_from_file_location('doppler_md_ref', a.py)
    ref = importlib.util.module_from_spec(spec)
    sys.modules['doppler_md_ref'] = ref
    spec.loader.exec_module(ref)
    rng = random.Random(a.seed)
    ok = degraded = nopage = 0
    fails = []
    for i in range(a.count):
        data, params = doc(rng), None
        params = tight_params(rng)
        args = [a.bin, '--report'] + (['--params', json.dumps(params)] if params else [])
        p = subprocess.run(args, input=data, capture_output=True)
        try:
            pages, report = ref.to_markdown_report(data.decode('utf-8', errors='replace'), params)
            want = '\f'.join(pages).encode('utf-8')
        except Exception as e:  # the reference must not raise on any input
            fails.append((i, f'python raised {type(e).__name__}: {e}', data))
            continue
        err = p.stderr.decode('utf-8', errors='replace').splitlines()
        try:
            got_counts = rust_counts(json.loads(err[0]))
        except (IndexError, ValueError) as e:
            fails.append((i, f'no report ({e}): rc {p.returncode} {err[:2]}', data))
            continue
        want_rc = 0 if report['pages'] else 2
        problems = []
        if p.returncode != want_rc:
            problems.append(f'rc {p.returncode} want {want_rc}')
        if p.stdout != want:
            problems.append('markdown differs')
        if got_counts != py_counts(report):
            problems.append(f'report {got_counts} != {py_counts(report)}')
        if problems:
            fails.append((i, '; '.join(problems), data))
            continue
        ok += 1
        degraded += bool(report['degraded'])
        nopage += not report['pages']
    print(f'difffuzz seed {a.seed}: {ok}/{a.count} identical ({degraded} with degraded pages, {nopage} without pages)')
    for i, why, data in fails[:20]:
        print(f'  FAIL doc {i}: {why}')
        if a.keep:
            os.makedirs(a.keep, exist_ok=True)
            with open(os.path.join(a.keep, f'difffuzz-{a.seed}-{i}.raw'), 'wb') as f:
                f.write(data)
    sys.exit(1 if fails else 0)


if __name__ == '__main__':
    main()
