#!/usr/bin/env python3
"""Tier-0 prototype: pdftohtml -xml (system poppler 26.10.0) -> Markdown. Throwaway heuristic probe, not a product.
usage: xml2md.py file.xml > out.md   (xml from: pdftohtml -xml -stdout -i -zoom 1 -hidden? file.pdf)"""
import sys, re, collections
import xml.etree.ElementTree as ET
root = ET.parse(sys.argv[1]).getroot()
def inline(t):
    s = ''
    def walk(e, b=False, i=False, href=None):
        nonlocal s
        tag = e.tag
        if tag == 'b': b = True
        if tag == 'i': i = True
        if tag == 'a': href = e.get('href')
        txt = e.text or ''
        if txt:
            piece = txt
            core = piece.strip()
            if core:
                lead = piece[:len(piece)-len(piece.lstrip())]; trail = piece[len(piece.rstrip()):]
                if b: core = f'**{core}**'
                if i: core = f'*{core}*'
                if href: core = f'[{core}]({href})'
                piece = lead + core + trail
            s += piece
        for c in e:
            walk(c, b, i, href)
            s += c.tail or ''
    walk(t)
    return s
out = []
for page in root.iter('page'):
    fonts = {f.get('id'): int(f.get('size')) for f in page.iter('fontspec')}
    lines = []
    for t in page.iter('text'):
        lines.append(dict(top=int(t.get('top')), left=int(t.get('left')), w=int(t.get('width')), h=int(t.get('height')),
                          size=fonts.get(t.get('font'), 0), raw=''.join(t.itertext()).strip(), md=inline(t).strip(),
                          bold=t.find('b') is not None and ''.join(t.itertext()).strip() == ''.join(t.find('b').itertext()).strip()))
    if not lines: continue
    cnt = collections.Counter()
    for l in lines: cnt[l['size']] += len(l['raw'])
    body = cnt.most_common(1)[0][0]
    # group into rows by top (tolerance), then rows into tables when >=3 consecutive multi-cell rows with aligned cell lefts
    lines.sort(key=lambda l: (l['top'], l['left']))
    rows = []
    for l in lines:
        if rows and abs(rows[-1][0]['top'] - l['top']) <= max(2, l['h'] // 3): rows[-1].append(l)
        else: rows.append([l])
    def gaps(r):  # multi-cell if big horizontal gap
        r.sort(key=lambda l: l['left'])
        cells = [[r[0]]]
        for l in r[1:]:
            prev = cells[-1][-1]
            if l['left'] - (prev['left'] + prev['w']) > body * 1.2: cells.append([l])
            else: cells[-1].append(l)
        return cells
    CURRENCY = {'$', '€', '£', '¥'}
    def attach_currency(r):
        """Financial tables print "$" apart from its amount, and pdftohtml often glues
        it to the end of the previous value ("4 % $"). Move each sign onto the amount
        to its right, so it neither forms its own column nor shifts its neighbours."""
        r = sorted(r, key=lambda l: l['left']); res = []; pending = ''
        for l in r:
            raw, md = l['raw'], l['md']
            if not raw: continue  # whitespace-only placeholder (where "$" sits on other rows)
            if pending: l = dict(l, raw=pending + raw, md=pending + md); raw, md = l['raw'], l['md']; pending = ''
            if raw in CURRENCY: pending = raw; continue
            m = re.match(r'^(.*\S)\s+([$€£¥])$', raw)
            if m and l is not r[-1]:
                pending = m.group(2); l = dict(l, raw=m.group(1), md=re.sub(r'\s*\**[$€£¥]\**$', '', md))
            res.append(l)
        if pending: res.append(dict(r[-1], raw=pending, md=pending))
        return res
    rows = [attach_currency(r) for r in rows]
    parsed = [gaps(r) for r in rows]
    def merge_currency(cells):  # "$" printed apart from its amount belongs to that amount
        res = []; pending = None
        for n, c in enumerate(cells):
            if pending: c = pending + c; pending = None
            # a currency sign closing a cell (e.g. "4 %" "$") prefixes the next cell's amount
            if len(c) > 1 and c[-1]['raw'] in CURRENCY and n + 1 < len(cells): c, pending = c[:-1], [c[-1]]
            if ' '.join(x['raw'] for x in c).strip() in CURRENCY: pending = c; continue
            res.append(c)
        if pending: res.append(pending)
        return res
    def extent(c): return (min(x['left'] for x in c), max(x['left'] + x['w'] for x in c))
    def overlap(a, b): return max(0, min(a[1], b[1]) - max(a[0], b[0]))
    NUMERIC = re.compile(r'^[(\-−+$€£¥]*\d[\d,.\s]*%?\)?$')
    def emit_table(block):
        """Assign cells to columns by x-position (not by order within a row), repeat
        header text that spans several columns into each of them, and fold leading
        header lines into one GFM header row, so every column keeps its labels
        (table-acceptance.md)."""
        block = [merge_currency(r) for r in block]
        # the most common cell count defines the columns; stray extra cells
        # (footnote markers) and short header rows are placed by position
        counts = collections.Counter(len(r) for r in block if len(r) >= 2)
        ncol = max(counts, key=lambda n: (counts[n], n))
        anchors = [r for r in block if len(r) == ncol]
        cols = [(sum(extent(r[k])[0] for r in anchors) / len(anchors), sum(extent(r[k])[1] for r in anchors) / len(anchors)) for k in range(ncol)]
        grid = []; in_header = True
        for r in block:
            # spanning text is repeated per column only in the leading header lines,
            # never in the body, where it would duplicate values or prose
            in_header = in_header and not any(NUMERIC.match(re.sub(r'[*_\\]', '', x['raw']).strip()) and not re.fullmatch(r'(19|20)\d\d', x['raw'].strip()) for c in r for x in c) and (not grid or not r[0][0]['left'] <= cols[0][1] - body)
            row = [''] * ncol
            for c in r:
                s = extent(c); text = ' '.join(x['md'] for x in c).replace('|', '\\|')
                ov = [overlap(s, col) for col in cols]
                covered = [k for k, o in enumerate(ov) if o > 0.5 * max(1, cols[k][1] - cols[k][0])]
                if in_header and len(r) < ncol and len(covered) >= 2: targets = covered  # spanning (group) title
                elif in_header and len(r) == 1 and not grid and ncol > 2 and len(text) <= 40 and abs((s[0] + s[1]) / 2 - (cols[1][0] + cols[-1][1]) / 2) <= 0.15 * (cols[-1][1] - cols[1][0]) and all(abs(s[0] - col[0]) > body for col in cols):
                    targets = list(range(1, ncol))  # lone centered title above the value columns, e.g. "Years ended"
                elif max(ov) > 0: targets = [ov.index(max(ov))]
                else: targets = [min(range(ncol), key=lambda k: min(abs(s[0] - cols[k][1]), abs(s[1] - cols[k][0])))]
                for k in targets: row[k] = (row[k] + ' ' + text).strip()
            grid.append(row)
        # fold wrapped header lines into one header row: the next line joins while
        # the header still has empty cells, holds no numbers, and is not a lone
        # section label in the row-label column ("Net sales:")
        def plain(v): return re.sub(r'[*_\\]', '', v).strip()
        def numeric(v): return bool(NUMERIC.match(v)) and not re.fullmatch(r'(19|20)\d\d', v)
        nhead = 1
        while nhead < min(3, len(grid) - 1):
            prev, raw = grid[nhead - 1], [plain(v) for v in grid[nhead]]
            filled = [k for k, v in enumerate(raw) if v]
            if all(prev) or not filled or any(numeric(raw[k]) for k in filled) or filled == [0]: break
            nhead += 1
        if nhead > 1:
            header = []
            for k in range(ncol):
                parts = []
                for row in grid[:nhead]:
                    if row[k] and (not parts or parts[-1] != row[k]): parts.append(row[k])
                header.append(' '.join(parts))
            grid = [header] + grid[nhead:]
        out.append('| ' + ' | '.join(grid[0]) + ' |')
        out.append('|' + ' --- |' * ncol)
        for row in grid[1:]: out.append('| ' + ' | '.join(row) + ' |')
        out.append('')
    i = 0; para = []
    def flush():
        global para
        if para: out.append(' '.join(para)); out.append(''); para = []
    prev_bottom = None
    def close(a, b, lines=1.5):  # rows a and b are vertically adjacent lines
        return rows[b][0]['top'] - (rows[a][0]['top'] + rows[a][0]['h']) <= lines * rows[b][0]['h']
    def table_end(i):  # end of the table run starting at row i, or i if none
        if len(parsed[i]) < 2: return i
        j = i
        while j < len(parsed):
            if len(parsed[j]) >= 2: j += 1; continue
            # a one-cell section label ("Net sales:", "Cost of sales:") in the row-label
            # column between rows with the same columns stays in the table; statements
            # often leave a blank line before such labels
            label = ' '.join(x['raw'] for x in parsed[j][0]).strip()
            if (j > i and j + 1 < len(parsed) and len(parsed[j + 1]) >= 2 and len(label) <= 60 and label.endswith(':')
                    and abs(parsed[j][0][0]['left'] - parsed[j + 1][0][0]['left']) <= body * 2
                    and close(j - 1, j, 3) and close(j, j + 1)): j += 1; continue
            break
        return j if j - i >= 2 else i
    def header_line(k, first):  # one-cell line just above a table, away from the left margin ("Years ended")
        return len(parsed[k]) == 1 and len(' '.join(x['raw'] for x in parsed[k][0])) <= 40 and close(k, first) and parsed[k][0][0]['left'] > min(x['left'] for c in parsed[first] for x in c) + body * 4
    while i < len(parsed):
        cells = parsed[i]
        first = i + 1 if len(cells) == 1 and i + 1 < len(parsed) and table_end(i + 1) > i + 1 and header_line(i, i + 1) else i
        j = table_end(first)
        if j > first:
            flush()
            emit_table(parsed[i:j])
            i = j; prev_bottom = None; continue
        l = rows[i][0]; text = ' '.join(x['md'] for c in cells for x in c)
        sz = max(x['size'] for c in cells for x in c)
        isb = all(x['bold'] for c in cells for x in c)
        if sz >= body * 1.5: flush(); out.append('# ' + re.sub(r'\*\*', '', text)); out.append('')
        elif sz >= body * 1.2 or (isb and len(text) < 80 and sz >= body): flush(); out.append('## ' + re.sub(r'\*\*', '', text)); out.append('')
        else:
            if prev_bottom is not None and l['top'] - prev_bottom > l['h'] * 0.8: flush()
            para.append(text)
        prev_bottom = l['top'] + l['h']; i += 1
    flush()
    out.append('\n---\n')
print('\n'.join(out))
