"""Original vector PDFs exercising chart/table geometry without copied content."""
import json
from pathlib import Path
import sys

from pf_fixtures import doc


def chart(*, vertical=False, labels=True, grid="behind", values=(180, 70, 130, 205),
          widths=(20, 20, 20, 20), ticks=(0, 25, 50, 75, 100), table=False, mixed=False,
          dividers=False, displaced_ticks=False, stray_tick=False, baseline=True):
    def point(value, lane):
        return (140 + lane, 330 + value) if vertical else (140 + value, 330 + lane)

    def stroke(v0, l0, v1, l1):
        x0, y0 = point(v0, l0)
        x1, y1 = point(v1, l1)
        return f"{x0} {y0} m {x1} {y1} l S\n"

    def rectangle(value, lane, width):
        x, y = point(0, lane)
        w, h = (width, value) if vertical else (value, width)
        return f"{x} {y} {w} {h} re f\n"

    def word(value, lane, content):
        x, y = point(value, lane)
        return f"BT /F1 9 Tf {x} {y} Td ({content}) Tj ET\n"

    lanes = (10, 45, 80, 115)
    grid_lines = "0.65 G 0.5 w\n" + "".join(stroke(x, 0, x, 145) for x in (25, 50, 100, 150, 200))
    if grid == "broken":
        grid_lines = "0.65 G 0.5 w\n" + "".join(
            stroke(x, lo, x, hi) for x in (25, 50, 100, 150, 200)
            for lo, hi in ((0, 9), (31, 44), (66, 79), (101, 114), (136, 145))
        )
    parts = ["0 G 0.5 w\n", stroke(0, 0, 210, 0)]
    if baseline:
        parts.append(stroke(0, 0, 0, 145))
    if grid in ("behind", "broken"):
        parts.append(grid_lines)
    parts.append("0.20 0.55 0.70 rg\n")
    parts.extend(rectangle(value, lane, width) for value, lane, width in zip(values, lanes, widths))
    if grid == "front":
        parts.append(grid_lines)
    if table or dividers:
        parts += ["0 G 0.5 w\n"]
        parts.extend(stroke(-90 if table else 0, y, 270 if table else 210, y) for y in (0, 37, 72, 107, 145))
    if table:
        parts.extend(stroke(x, 0, x, 145) for x in (-90, 100, 210, 270))
    parts.append("0 g\n")
    if ticks:
        parts.extend(word(i * 50 - (4 if not vertical else 2), -27 - i * 8 * displaced_ticks, str(tick))
                     for i, tick in enumerate(ticks))
    if stray_tick:
        parts.append(word(65, -57, "999"))
    for i, lane in enumerate(lanes):
        location = lane + (3 if vertical else 6)
        if labels:
            parts.append(word(14, location, ("Alder", "Birch", "Cedar", "Elm")[i]))
        if table:
            parts.append(word(-73, location, ("North", "West", "South", "East")[i]))
            parts.append(word(225, location, str(10 * (i + 1))))
    if mixed:
        parts += ["0 G 0.5 w\n"]
        parts.extend(f"400 {400 + i * 20} m 560 {400 + i * 20} l S\n" for i in range(6))
        parts.extend(f"{x} 400 m {x} 500 l S\n" for x in (400, 490, 560))
        for i in range(5):
            parts.append(f"BT /F1 9 Tf 410 {405 + i * 20} Td (Sensor {i+1}) Tj 90 0 Td ({30+i}) Tj ET\n")
    return "".join(parts)


def edge_column_table():
    parts = [chart(labels=False), "0 G 0.5 w\n"]
    parts.extend(f"140 {y} m 560 {y} l S\n" for y in (330, 367, 402, 437, 475))
    parts.extend(f"{x} 330 m {x} 475 l S\n" for x in (140, 350, 453, 560))
    for i, y in enumerate((346, 381, 416, 451)):
        parts.append(f"BT /F1 9 Tf 370 {y} Td (Quarter {i+1}) Tj 105 0 Td ({20+10*i}) Tj ET\n")
    return "".join(parts)


def one_sided_labels():
    return chart(dividers=True) + "".join(
        f"BT /F1 9 Tf 94 {y} Td (Lane {i+1}) Tj ET\n"
        for i, y in enumerate((346, 381, 416, 451))
    )


CASES = {
    "data-bars-in-edge-column": (False, edge_column_table()),
    "chart-with-one-sided-labels": (True, one_sided_labels()),
    "labeled-crossings-behind": (True, chart()),
    "labeled-crossings-front": (True, chart(grid="front")),
    "vertical-labeled": (True, chart(vertical=True)),
    "inferred-baseline": (True, chart(baseline=False)),
    "gridless-decoration-near-ruler": (False, chart(grid="none", values=(190, 98, 162, 205))),
    "rule-fragment-decoration": (False, chart(grid="broken", values=(175, 94, 151, 207))),
    "chart-with-category-dividers": (True, chart(dividers=True)),
    "nearby-grid-in-table": (False, chart(dividers=True, labels=False, table=True)),
    "irregular-tick-values": (True, chart(ticks=(0, 10, 40, 60, 100))),
    "off-axis-numeric-decoration": (False, chart(ticks=(10, 20, 30, 40, 50), displaced_ticks=True)),
    "thin-mark-decoration": (False, chart(widths=(2, 20, 2, 20))),
    "equal-status-boxes": (False, chart(values=(180,) * 4, labels=False)),
    "single-rule-forms": (False, chart(values=(28, 33, 39, 205), widths=(20, 20, 20, 16), dividers=True, table=True)),
    "sparse-numeric-decoration": (False, chart(ticks=(0, 25), labels=False)),
    "rounded-staircase": (True, chart(values=(50, 100, 150, 200))),
    "mixed-separate-table": (True, chart(mixed=True)),
    "one-crossing": (True, chart(values=(28, 33, 39, 205))),
    "varying-thickness": (True, chart(widths=(15, 23, 9, 18))),
    "gridless-labeled": (True, chart(grid="none")),
    "broken-grid": (True, chart(grid="broken")),
    "scale-free-labeled": (True, chart(ticks=())),
    "two-ticks": (True, chart(ticks=(0, 25))),
    "equal-length": (True, chart(values=(180,) * 4)),
    "scale-contaminated": (True, chart(stray_tick=True)),
    "plain-chart-control": (True, chart(labels=False, grid="none")),
    "in-cell-bars-with-crossings": (False, chart(table=True)),
    "in-cell-bars-no-crossings": (False, chart(table=True, grid="none")),
    "unlabeled-in-cell-bars": (False, chart(table=True, labels=False)),
    "shaded-cell-boundaries": (False, chart(table=True, labels=False, grid="none", values=(100, 210, 100, 210))),
    "progress-strip-no-scale": (False, chart(labels=False, ticks=(), grid="none")),
    "grid-decoration-no-scale": (False, chart(labels=False, ticks=())),
}


if __name__ == "__main__":
    output = Path(sys.argv[1])
    output.mkdir(parents=True, exist_ok=True)
    for name, (_, content) in CASES.items():
        (output / f"{name}.pdf").write_bytes(doc([{"content": content}]))
    (output / "manifest.json").write_text(json.dumps({name: expected for name, (expected, _) in CASES.items()}, indent=2) + "\n")
