"""Write small synthetic PDFs for the rotated and borderless table tests, then (optionally) their
doppler XHTML in tests/fixtures/producer.

    python3 -I scripts/make_table_pdfs.py OUT_DIR [--pdftotext PATH]

With --pdftotext, each PDF is converted with `-bbox-layout -wordmeta -rules` (the strict form
the golden set uses) into tests/fixtures/producer/<name>.xhtml. Needs pycairo. All text and
numbers are made up here; the layouts (sizes, column counts, row pitch, rotations) are chosen to
cover the shapes the converter must handle, not copied from any document.
"""
import math
import pathlib
import subprocess
import sys

import cairo

ROOT = pathlib.Path(__file__).resolve().parent.parent
FIXTURES = ROOT / 'tests' / 'fixtures' / 'producer'

STATIONS = [
    ('Station', 'Depth (m)', 'Flow', 'Change'),
    ('Alder Creek', '12.5', '1,204', '(37.2)'),
    ('Birch Hollow', '7.25', '88', '4.1'),
    ('Cedar Run', '104.0', '15,730', '(0.6)'),
    ('Dunmore Lake', '3.75', '612', '12.9'),
    ('Elm Fork', '66.1', '9', '(118.4)'),
]

SHIPMENTS = [
    ('Item', 'Crates', 'Weight', 'Balance'),
    ('Copper wire', '14', '2,310.5', '(45)'),
    ('Glass panes', '3', '87.25', '1,020'),
    ('Oak boards', '220', '14,002.0', '(7)'),
    ('Wool bales', '41', '506.75', '333'),
]


def text(cr, x, y, s, size=9.0, angle=0.0, align='left'):
    """Draw s with its baseline origin at (x, y) in a frame rotated by angle (radians)."""
    cr.save()
    cr.translate(x, y)
    cr.rotate(angle)
    cr.set_font_size(size)
    w = cr.text_extents(s).x_advance
    dx = {'left': 0.0, 'right': -w, 'center': -w / 2}[align]
    cr.move_to(dx, 0)
    cr.show_text(s)
    cr.restore()


def table(cr, rows, x, y, widths, pitch, size=9.0, angle=0.0, rules=False, aligns=None):
    """A table in a frame rotated by angle about (x, y): rows run along the frame's y axis."""
    cr.save()
    cr.translate(x, y)
    cr.rotate(angle)
    aligns = aligns or ['left'] + ['right'] * (len(widths) - 1)
    total = sum(widths)
    for r, row in enumerate(rows):
        cx = 0.0
        for c, cell in enumerate(row):
            w = widths[c]
            tx = {'left': cx + 4, 'right': cx + w - 4, 'center': cx + w / 2}[aligns[c]]
            text(cr, tx, r * pitch + pitch * 0.7, cell, size, 0.0, aligns[c])
            cx += w
    if rules:
        cr.set_line_width(0.6)
        for r in range(len(rows) + 1):
            cr.move_to(0, r * pitch)
            cr.line_to(total, r * pitch)
        cx = 0.0
        for w in [0] + widths:
            cx += w
            cr.move_to(cx, 0)
            cr.line_to(cx, len(rows) * pitch)
        cr.stroke()
    cr.restore()


def prose(cr, x, y, lines, size=10.0, pitch=13.0):
    for i, s in enumerate(lines):
        text(cr, x, y + i * pitch, s, size)


def surface(path, w, h):
    s = cairo.PDFSurface(str(path), w, h)
    cr = cairo.Context(s)
    cr.select_font_face('Sans', cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
    cr.set_source_rgb(0, 0, 0)
    return s, cr


INTRO = [
    'Gauging notes for the spring survey. Readings were taken at dawn on',
    'each site, and the change column compares them with the autumn run.',
]
OUTRO = [
    'Negative changes are shown in parentheses. Sites with ice cover were',
    'skipped and are listed in the appendix with the reason for each one.',
]


def rot90_ruled(out):
    # whole portrait page turned: the table reads top to bottom (rotation 1), ruled grid
    s, cr = surface(out, 612, 792)
    text(cr, 560, 60, 'Spring gauging summary', 14, math.pi / 2)
    table(cr, STATIONS, 520, 60, [120, 80, 80, 80], 22, 9, math.pi / 2, rules=True)
    s.finish()


def rot270_borderless(out):
    # whole page turned the other way (rotation 3), no rules, wide-spaced rows
    s, cr = surface(out, 612, 792)
    text(cr, 60, 740, 'Warehouse shipments by item', 14, -math.pi / 2)
    table(cr, SHIPMENTS, 90, 740, [130, 70, 90, 90], 30, 9, -math.pi / 2)
    s.finish()


def mixed_pages(out):
    # page 1: upright prose, then a rotated (90) ruled table in the middle, then upright prose;
    # page 2: an ordinary upright ruled table
    s, cr = surface(out, 612, 792)
    text(cr, 72, 60, 'Field report', 16)
    prose(cr, 72, 90, INTRO)
    table(cr, STATIONS, 420, 140, [110, 70, 70, 70], 24, 8.5, math.pi / 2, rules=True)
    prose(cr, 72, 520, OUTRO)
    s.show_page()
    text(cr, 72, 60, 'Shipments', 16)
    table(cr, SHIPMENTS, 72, 90, [130, 70, 90, 90], 18, 9, 0.0, rules=True)
    s.finish()


def borderless_sparse(out):
    # upright, no rules, rows about 3.3 font sizes apart, right-aligned ragged numbers,
    # negatives in parentheses, a text header row, prose above and below
    s, cr = surface(out, 612, 792)
    text(cr, 72, 60, 'Survey readings', 16)
    prose(cr, 72, 90, INTRO)
    table(cr, STATIONS, 72, 130, [140, 90, 90, 90], 30, 9)
    prose(cr, 72, 340, OUTRO)
    s.finish()


def borderless_decimal(out):
    # decimal-aligned numbers (the point lines up, not the edges) and a centred header
    s, cr = surface(out, 612, 792)
    text(cr, 72, 60, 'Batch weights', 16)
    rows = [('Lot', 'Mass', 'Spread'), ('North', '1.5', '0.125'), ('South', '128.25', '12.5'),
            ('East', '64.0', '(3.75)'), ('West', '7.125', '100.0')]
    pitch, x = 32, 72
    for r, row in enumerate(rows):
        y = 110 + r * pitch
        text(cr, x, y, row[0], 10)
        for c, cell in enumerate(row[1:]):
            point = x + 180 + c * 110
            if r == 0:
                text(cr, point, y, cell, 10, 0.0, 'center')
                continue
            whole = cell.split('.')[0]
            cr.set_font_size(10)
            wpx = cr.text_extents(whole).x_advance
            text(cr, point - wpx, y, cell, 10)
    s.finish()


def negatives(out):
    # nothing here is a table: two-column prose, a double-spaced numbered list and a column
    # of numbers inside running text
    s, cr = surface(out, 612, 792)
    left = ['The river rose slowly through', 'the night and crested near the', 'old mill before noon, when the',
            'crews began clearing debris', 'from the lower footbridge and', 'reopened the towpath again.']
    right = ['Volunteers logged every gauge', 'by hand because the telemetry', 'link was down for most of the',
             'week; the logs were typed up', 'later and checked against the', 'automatic station records.']
    for i, (a, b) in enumerate(zip(left, right)):
        text(cr, 72, 80 + i * 30, a, 10)
        text(cr, 320, 80 + i * 30, b, 10)
    items = ['Check the gauge', 'Record the level', 'Photograph the bank', 'Sign the sheet']
    for i, it in enumerate(items):
        text(cr, 72, 300 + i * 30, f'{i + 1}.', 10)
        text(cr, 100, 300 + i * 30, it, 10)
    prose(cr, 72, 450, ['The totals for the four gauges were:'])
    for i, v in enumerate(['1,204', '88', '15,730', '612']):
        text(cr, 72, 476 + i * 26, v, 10)
    prose(cr, 72, 590, ['and no gauge was read twice on the same day.'])
    s.finish()


CASES = [rot90_ruled, rot270_borderless, mixed_pages, borderless_sparse, borderless_decimal, negatives]


def main():
    args = sys.argv[1:]
    pdftotext = None
    if '--pdftotext' in args:
        i = args.index('--pdftotext')
        pdftotext = args[i + 1]
        del args[i:i + 2]
    out = pathlib.Path(args[0])
    out.mkdir(parents=True, exist_ok=True)
    for case in CASES:
        pdf = out / f'{case.__name__}.pdf'
        case(pdf)
        if pdftotext:
            FIXTURES.mkdir(parents=True, exist_ok=True)
            dst = FIXTURES / f'{case.__name__}.xhtml'
            subprocess.run([pdftotext, '-q', '-bbox-layout', '-wordmeta', '-rules', str(pdf), str(dst)], check=True)
        print(pdf)


if __name__ == '__main__':
    main()
