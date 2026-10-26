"""Write the synthetic test fixtures in tests/fixtures: <name>.xhtml and the reference output
<name>.md (pages joined by form feeds) from doppler_md.py.

    python3 -I scripts/make_fixtures.py /path/to/doppler_md.py

Every fixture is generated here from made-up text; none is copied from a real document.
"""
import importlib.util
import pathlib
import sys
from xml.sax.saxutils import escape, quoteattr

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / 'tests' / 'fixtures'


class Doc:
    def __init__(self):
        self.pages = []

    def page(self, width=612, height=792):
        self.pages.append({'w': width, 'h': height, 'flows': [], 'rules': []})
        return self

    def rule(self, x0, y0, x1, y1, color='#000000', box=False):
        self.pages[-1]['rules'].append((x0, y0, x1, y1, color, box))
        return self

    def block(self, lines):
        """lines: [[(x0, y0, x1, y1, text, size, font, color, space)]]; one flow per block."""
        self.pages[-1]['flows'].append(lines)
        return self

    def xhtml(self):
        out = ['<?xml version="1.0" encoding="UTF-8"?>', '<html><body><doc>']
        for p in self.pages:
            out.append(f'  <page width="{p["w"]:.6f}" height="{p["h"]:.6f}">')
            for x0, y0, x1, y1, color, box in p['rules']:
                tag = 'box' if box else 'rule'
                out.append(f'    <{tag} xMin="{x0:.2f}" yMin="{y0:.2f}" xMax="{x1:.2f}" yMax="{y1:.2f}" color="{color}"/>')
            for lines in p['flows']:
                out.append('    <flow>')
                out.append('      <block>')
                for words in lines:
                    out.append('        <line>')
                    for x0, y0, x1, y1, text, size, font, color, space in words:
                        base = y1 - 0.2 * size
                        out.append(f'          <word xMin="{x0:.6f}" yMin="{y0:.6f}" xMax="{x1:.6f}" yMax="{y1:.6f}" '
                                   f'fontName={quoteattr(font)} fontSize="{size:.6f}" spaceAfter="{int(space)}" '
                                   f'color="{color}" baseline="{base:.6f}">{escape(text, {chr(34): "&quot;"})}</word>')
                    out.append('        </line>')
                out.append('      </block>')
                out.append('    </flow>')
            out.append('  </page>')
        out.append('</doc></body></html>')
        return '\n'.join(out) + '\n'


def words(x, y, text, size=10.0, font='Serif', color='#000000', gap=None, adv=None):
    """A line of words starting at x with baseline-ish top y; adv is the per-character advance."""
    adv = adv or size * 0.5
    gap = size * 0.3 if gap is None else gap
    out = []
    for t in text.split(' '):
        w = len(t) * adv
        out.append((x, y, x + w, y + size, t, size, font, color, True))
        x += w + gap
    return out


def cells(y, spec, size=10.0, font='Serif'):
    """One visual line from [(x, text)] cells (each cell may have several words)."""
    out = []
    for x, text in spec:
        out += words(x, y, text, size, font)
    return out


PROSE = ('alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho '
         'sigma tau upsilon phi chi psi omega').split()


def prose_lines(x, y, n, width_words=9, size=10.0, start=0, pitch=12.0, last_short=True):
    lines = []
    k = start
    for i in range(n):
        count = width_words if (i < n - 1 or not last_short) else 4
        text = ' '.join(PROSE[(k + j) % len(PROSE)] for j in range(count))
        k += count
        if i == n - 1:
            text += '.'
        lines.append(words(x, y + i * pitch, text, size))
    return lines


def two_columns():
    d = Doc().page()
    d.block([words(72, 60, 'Columns and Gutters', 20.0, 'Sans-Bold')])
    d.block(prose_lines(72, 100, 6, width_words=6))
    d.block(prose_lines(330, 100, 6, width_words=6, start=7))
    # a left-column line reaching a little into the gutter narrows it
    d.block([words(72, 200, 'tail of the left column reaches', 10.0)])
    d.block(prose_lines(330, 200, 2, width_words=6, start=3))
    d.block(prose_lines(72, 260, 2, width_words=20, start=11))
    return d


def rtl():
    d = Doc().page()
    he = ['שלום', 'עולם', 'מספר']
    line = words(300, 100, he[0]) + words(330, 100, '2026') + words(360, 100, he[1])
    d.block([line])
    d.block([words(300, 130, ',28') + words(330, 130, he[2])])
    d.block([words(72, 160, 'Mixed English line with ' + 'שלום')])
    return d


def ruled_table():
    d = Doc().page()
    d.block([words(72, 60, 'Quarterly Figures', 16.0, 'Sans-Bold')])
    top, pitch, xs = 100.0, 20.0, [72, 200, 330, 460]
    rows = [('Region', 'Units', 'Revenue'), ('North', '1,204', '$3.5m'), ('South', '980', '$2.9m'), ('East', '1,511', '$4.1m')]
    for i in range(len(rows) + 1):
        d.rule(xs[0], top + i * pitch - 0.5, xs[-1], top + i * pitch + 0.5)
    for x in xs:
        d.rule(x - 0.5, top, x + 0.5, top + len(rows) * pitch)
    for i, row in enumerate(rows):
        d.block([cells(top + i * pitch + 5, [(xs[k] + 4, t) for k, t in enumerate(row)], font='Sans-Bold' if i == 0 else 'Serif')])
    d.block(prose_lines(72, 220, 3))
    return d


def gutter_table():
    d = Doc().page()
    d.block(prose_lines(72, 60, 2))
    rows = [('Item', 'Qty', 'Price'), ('Widget', '12', '4.50'), ('Gadget', '7', '11.25'), ('Sprocket', '140', '0.75'), ('Flange', '3', '19.00')]
    for i, row in enumerate(rows):
        d.block([cells(100 + i * 14, [(72, row[0]), (240, row[1]), (360, row[2])])])
    d.block(prose_lines(72, 200, 2, start=5))
    return d


def lists_headings():
    d = Doc().page()
    d.block([words(72, 50, 'Main Title', 24.0, 'Sans-Bold')])
    d.block([words(72, 90, 'Section Heading', 14.0, 'Sans-Bold')])
    d.block(prose_lines(72, 110, 3))
    d.block([words(72, 160, '•', 10.0) + words(84, 160, 'first bullet item with text')])
    d.block([words(72, 174, '•second bullet glued to its marker')])
    d.block([words(72, 188, '1.', 10.0) + words(90, 188, 'numbered item one')])
    d.block([words(72, 202, '2.') ])
    d.block([words(90, 202, 'numbered item two on the same baseline')])
    d.block([words(72, 230, 'Minor bold lead', 10.0, 'Serif-Bold')])
    d.block(prose_lines(72, 244, 3, start=4))
    d.block([words(72, 300, 'A coloured note', 10.0, 'Serif', '#cc0000')])
    d.block(prose_lines(72, 314, 2, start=9))
    return d


def joins():
    d = Doc().page()
    d.block([words(72, 100, 'a hyphen- ated word and an exam-'), words(72, 112, 'ple of dehyphenation and Upper-'),
              words(72, 124, 'Case that keeps its hyphen.')])
    d.block([words(72, 160, '日本語の文章は'), words(72, 172, '空白なしで続きます。')])
    d.block([words(72, 200, 'soft­'), words(72, 212, 'hyphen &amp; entities <ok> "quoted"')])
    return d


def furniture():
    d = Doc()
    for i in range(3):
        d.page()
        d.block([words(72, 20, 'Running Header Text')])
        d.block(prose_lines(72, 100, 3, start=i * 5))
        d.block([words(300, 760, str(i + 1))])
    return d


FIXTURES = {
    'two_columns': two_columns, 'rtl': rtl, 'ruled_table': ruled_table, 'gutter_table': gutter_table,
    'lists_headings': lists_headings, 'joins': joins, 'furniture': furniture,
}


def main():
    spec = importlib.util.spec_from_file_location('doppler_md', sys.argv[1])
    mod = importlib.util.module_from_spec(spec)
    sys.modules["doppler_md"] = mod
    spec.loader.exec_module(mod)
    OUT.mkdir(parents=True, exist_ok=True)
    for name, make in FIXTURES.items():
        x = make().xhtml()
        (OUT / f'{name}.xhtml').write_text(x, encoding='utf-8')
        (OUT / f'{name}.md').write_text('\f'.join(mod.to_markdown(x)), encoding='utf-8')
        print(name)


if __name__ == '__main__':
    main()
