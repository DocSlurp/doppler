#!/usr/bin/env python3
"""Deterministic reference-based Markdown/HTML table evaluation. No model calls.

See README.md for the supported dialect, schema, limits and metric definitions.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import hashlib
from html.parser import HTMLParser
import importlib.metadata
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import unicodedata

VERSION = "1.0.0"
MAX_BYTES = 8 * 1024 * 1024
MAX_CELLS = 100_000
MAX_ROWS = 10_000
MAX_COLS = 256
MAX_TABLES = 256
MAX_ALIGNMENT = 2_000_000


class InputError(ValueError):
    """Malformed, unsupported, or over-budget input; never a quality score."""


def normalize(text: str) -> str:
    """Normalize ordinary whitespace, not signs, decimal marks or case."""
    text = " ".join(unicodedata.normalize("NFC", text).split())
    text = re.sub(r"([$€£¥]) +(?=[+−-]?\d)", r"\1", text)
    # "4 %", "(6) %" (negative) and "— %" (nil) are spacing variants of a percentage.
    return re.sub(r"(?<=[\d)—–−-]) +(?=%)", "", text)


def header_tokens(text: str) -> tuple[str, ...]:
    # Header separators and presentation punctuation are not associations.
    # Underscore is a separator too: flattened multi-level titles are often
    # joined as `Revenue_Q1`. Reference labels are tokenized the same way.
    return tuple(re.findall(r"[^\W_]+|[%$€£¥]", normalize(text).casefold()))


@dataclass
class Cell:
    text: str
    header: bool = False
    rowspan: int = 1
    colspan: int = 1


@dataclass
class Table:
    rows: list[list[Cell]] = field(default_factory=list)
    grid: list[list[str | None]] = field(default_factory=list)
    header_rows: int = 0
    spans: list[tuple[int, int, int, int]] = field(default_factory=list)

    def expand(self) -> None:
        occupied: dict[tuple[int, int], str] = {}
        width = 0
        leading_headers = True
        for r, row in enumerate(self.rows):
            if leading_headers and row and all(c.header for c in row):
                self.header_rows += 1
            else:
                leading_headers = False
            col = 0
            for cell in row:
                while (r, col) in occupied:
                    col += 1
                if col + cell.colspan > MAX_COLS or r + cell.rowspan > len(self.rows):
                    raise InputError("span exceeds table bounds")
                if cell.rowspan > 1 or cell.colspan > 1:
                    self.spans.append((r, col, cell.rowspan, cell.colspan))
                for rr in range(r, r + cell.rowspan):
                    for cc in range(col, col + cell.colspan):
                        if (rr, cc) in occupied:
                            raise InputError("overlapping table spans")
                        occupied[rr, cc] = normalize(cell.text)
                        if len(occupied) > MAX_CELLS:
                            raise InputError("expanded table cell limit exceeded")
                col += cell.colspan
                width = max(width, col)
        if width * len(self.rows) > MAX_CELLS:
            raise InputError("rectangular table cell limit exceeded")
        self.grid = [[occupied.get((r, c)) for c in range(width)] for r in range(len(self.rows))]
        if any(v is None for row in self.grid for v in row):
            raise InputError("ragged HTML table: missing cells must be explicit")
        if not self.grid or not width:
            raise InputError("empty table")


class TableParser(HTMLParser):
    """Non-executing bounded parser for the structural HTML table subset."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.tables: list[Table] = []
        self.table: Table | None = None
        self.row: list[Cell] | None = None
        self.cell: Cell | None = None
        self.in_head = False
        self.hidden: list[str] = []
        self.cell_count = 0
        self.expanded_count = 0
        self.in_code = False

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        a = dict(attrs)
        if tag in {"script", "style", "template"}:
            self.hidden.append(tag)
            return
        if self.hidden:
            return
        if self.table is not None or tag == "table":
            style = (a.get("style") or "").lower()
            if "hidden" in a or re.search(r"display\s*:|visibility\s*:", style):
                raise InputError("CSS/hidden table content requires a browser reference")
        if tag == "table":
            if self.table is not None:
                raise InputError("nested tables are unsupported")
            if len(self.tables) >= MAX_TABLES:
                raise InputError("table count limit exceeded")
            self.table = Table()
        elif self.table is not None:
            if tag == "thead":
                self.in_head = True
            elif tag == "tr":
                self.cell = None  # HTML permits omitted cell/row closing tags.
                self.row = []
                self.table.rows.append(self.row)
                if len(self.table.rows) > MAX_ROWS:
                    raise InputError("table row limit exceeded")
            elif tag in {"td", "th"}:
                if self.row is None:
                    raise InputError("cell outside a table row")
                spans = []
                for key in ("rowspan", "colspan"):
                    value = a.get(key, "1")
                    if value is None or not re.fullmatch(r"[1-9][0-9]{0,4}", value):
                        raise InputError(f"invalid {key}: {value!r}")
                    spans.append(int(value))
                is_header = (tag == "th" and a.get("scope") not in {"row", "rowgroup"}) or self.in_head
                self.cell = Cell("", is_header, *spans)
                self.row.append(self.cell)
                self.cell_count += 1
                if self.cell_count > MAX_CELLS:
                    raise InputError("document cell limit exceeded")
            elif self.cell is not None and tag in {"br", "p", "div", "li"}:
                self.cell.text += " "
            elif self.cell is not None and tag == "img":
                raise InputError("image-valued table cells require a text reference adapter")
            elif self.cell is not None and tag in {"code", "pre"}:
                self.in_code = True

    def handle_endtag(self, tag: str) -> None:
        if self.hidden:
            if tag == self.hidden[-1]:
                self.hidden.pop()
            return
        if tag == "table" and self.table is not None:
            self.table.expand()
            self.expanded_count += len(self.table.grid) * len(self.table.grid[0])
            if self.expanded_count > MAX_CELLS:
                raise InputError("document expanded cell limit exceeded")
            self.tables.append(self.table)
            self.table = None
            self.row = None
            self.cell = None
            self.in_head = False
        elif tag == "thead":
            self.in_head = False
        elif tag == "tr":
            self.row = None
            self.cell = None
        elif tag in {"td", "th"}:
            self.cell = None
            self.in_code = False
        elif tag in {"code", "pre"}:
            self.in_code = False
        elif self.cell is not None and tag in {"p", "div", "li"}:
            self.cell.text += " "

    def handle_data(self, data: str) -> None:
        if self.cell is not None and not self.hidden:
            if self.in_code and re.search(r"\s{2,}|[\n\t]", data):
                raise InputError("whitespace-sensitive code cells require a verbatim reference adapter")
            self.cell.text += data


def parse_tables(document: str, fmt: str = "markdown") -> list[Table]:
    if len(document.encode("utf-8")) > MAX_BYTES:
        raise InputError("document byte limit exceeded")
    if fmt == "markdown":
        try:
            from markdown_it import MarkdownIt
            from markdown_it.rules_block.table import escapedSplit
        except ImportError as exc:
            raise InputError("install table_scoring/requirements.txt") from exc
        md = MarkdownIt("commonmark", {"html": True}).enable(["table", "strikethrough"])
        tokens = md.parse(document)
        lines = document.splitlines()
        for token in tokens:
            if token.type != "table_open" or token.map is None:
                continue
            start, end = token.map
            def count(line: str) -> int:
                line = line.strip()
                if line.startswith("|"):
                    line = line[1:]
                if line.endswith("|") and not line.endswith("\\|"):
                    line = line[:-1]
                return len(escapedSplit(line))
            width = count(lines[start])
            if width > MAX_COLS or end - start > MAX_ROWS + 1:
                raise InputError("Markdown table dimension limit exceeded")
            # GFM silently discards excess body cells. Report invalid input rather
            # than giving a perfect score to a candidate with hidden extra data.
            if any(count(line) > width for line in lines[start + 2:end]):
                raise InputError("Markdown row has excess cells that rendering would discard")
        document = md.renderer.render(tokens, md.options, {})
    elif fmt != "html":
        raise InputError("format must be markdown or html")
    parser = TableParser()
    parser.feed(document)
    parser.close()
    if parser.table is not None:
        raise InputError("unclosed HTML table")
    return parser.tables


def validate_reference(ref: dict) -> dict:
    if not isinstance(ref, dict) or type(ref.get("schema_version")) is not int or ref["schema_version"] != 1:
        raise InputError("reference schema_version must be 1")
    allowed = {"schema_version", "id", "source", "headers", "rows", "spans", "notes"}
    if set(ref) - allowed:
        raise InputError(f"unknown reference fields: {sorted(set(ref) - allowed)}")
    if not isinstance(ref.get("id"), str) or not ref["id"]:
        raise InputError("reference requires a nonempty id")
    if (not isinstance(ref.get("source"), dict)
            or not isinstance(ref["source"].get("description"), str) or not ref["source"]["description"]):
        raise InputError("reference requires source.description provenance")
    headers, rows = ref.get("headers"), ref.get("rows")
    if not isinstance(headers, list) or not 1 <= len(headers) <= MAX_COLS:
        raise InputError("reference headers must define 1..256 columns")
    for labels in headers:
        if not isinstance(labels, list) or any(not isinstance(s, str) or not header_tokens(s) for s in labels):
            raise InputError("each column header must be a list of nonempty labels")
        if len({header_tokens(s) for s in labels}) != len(labels):
            raise InputError("duplicate reference header association")
    if not isinstance(rows, list) or not 1 <= len(rows) <= MAX_ROWS:
        raise InputError("reference requires 1..10000 data rows")
    if len(rows) * len(headers) > MAX_CELLS:
        raise InputError("reference cell limit exceeded")
    for row in rows:
        if not isinstance(row, list) or len(row) != len(headers) or any(not isinstance(v, str) for v in row):
            raise InputError("reference rows must be rectangular strings (use empty strings for empty cells)")
    if "spans" in ref:
        if not isinstance(ref["spans"], list):
            raise InputError("spans must be a list")
        for span in ref["spans"]:
            if (not isinstance(span, list) or len(span) != 5 or span[0] not in {"header", "body"}
                    or any(type(v) is not int for v in span[1:]) or min(span[1:3]) < 0
                    or min(span[3:]) < 1 or span[2] + span[4] > len(headers) or span[1] + span[3] > MAX_ROWS
                    or (span[0] == "body" and span[1] + span[3] > len(rows))):
                raise InputError("invalid reference span [section, row, column, rowspan, colspan]")
        if len({tuple(s) for s in ref["spans"]}) != len(ref["spans"]):
            raise InputError("duplicate reference span")
        occupied = set()
        for section, r, c, rs, cs in ref["spans"]:
            if rs * cs + len(occupied) > MAX_CELLS:
                raise InputError("reference span expansion limit exceeded")
            for rr in range(r, r + rs):
                for cc in range(c, c + cs):
                    if (section, rr, cc) in occupied:
                        raise InputError("overlapping reference spans")
                    occupied.add((section, rr, cc))
    return ref


def metric(matched: int, expected: int, actual: int) -> dict:
    precision = matched / actual if actual else (1.0 if expected == 0 else 0.0)
    recall = matched / expected if expected else 1.0
    f1 = 2 * matched / (expected + actual) if expected + actual else 1.0
    return dict(matched=matched, expected=expected, actual=actual,
                precision=precision, recall=recall, f1=f1)


def contains(tokens: tuple[str, ...], phrase: tuple[str, ...]) -> bool:
    return any(tokens[i:i + len(phrase)] == phrase for i in range(len(tokens) - len(phrase) + 1))


def associations(text: str, vocabulary: set[tuple[str, ...]]) -> tuple[set[tuple[str, ...]], set[str]]:
    tokens = header_tokens(text)
    covered: set[int] = set()
    labels = set()
    for label in vocabulary:
        for i in range(len(tokens) - len(label) + 1):
            if tokens[i:i + len(label)] == label:
                labels.add(label)
                covered.update(range(i, i + len(label)))
    return labels, {token for i, token in enumerate(tokens) if i not in covered}


def align_rows(expected: list[list[str]], actual: list[list[str]]) -> list[tuple[int | None, int | None]]:
    """Order-preserving edit alignment. Never permute columns or sort rows."""
    n, m = len(expected), len(actual)
    if expected == actual:
        return [(i, i) for i in range(n)]
    if not actual:
        return [(i, None) for i in range(n)]
    if n * m > MAX_ALIGNMENT:
        raise InputError("row alignment budget exceeded; evaluate smaller explicit table regions")
    width = max(len(expected[0]), len(actual[0]) if actual else 0)
    if n * m * width > 20_000_000:
        raise InputError("row alignment cell-comparison budget exceeded")
    # Cost includes empty slots, so missing/shifted blank cells remain errors.
    prev = [j * width for j in range(m + 1)]
    trace = [bytearray(m + 1) for _ in range(n + 1)]
    for j in range(1, m + 1):
        trace[0][j] = 2
    for i in range(1, n + 1):
        cur = [i * width] + [0] * m
        trace[i][0] = 1
        for j in range(1, m + 1):
            eq = sum(a == b for a, b in zip(expected[i - 1], actual[j - 1]))
            choices = (prev[j - 1] + width - eq, prev[j] + width, cur[j - 1] + width)
            op = min(range(3), key=choices.__getitem__)
            cur[j], trace[i][j] = choices[op], op
        prev = cur
    pairs = []
    i, j = n, m
    while i or j:
        op = trace[i][j]
        if op == 0:
            pairs.append((i - 1, j - 1)); i -= 1; j -= 1
        elif op == 1:
            pairs.append((i - 1, None)); i -= 1
        else:
            pairs.append((None, j - 1)); j -= 1
    return pairs[::-1]


def score_table(reference: dict, table: Table | None) -> dict:
    ref = validate_reference(reference)
    wanted = [{header_tokens(s) for s in labels} for labels in ref["headers"]]
    vocabulary = set().union(*wanted)
    expected = [[normalize(v) for v in row] for row in ref["rows"]]
    width = len(wanted)
    grid = table.grid if table is not None else []
    actual_width = len(grid[0]) if grid else 0
    header_rows = table.header_rows if table is not None else 0
    header_text = ["" for _ in range(max(width, actual_width))]
    dropped = []
    body = []
    body_source_rows = []
    for r, row in enumerate(grid):
        # Repeated/multi-level header rows must consist solely of labels valid
        # for their OWN columns. Reference-identical data rows cannot be dropped.
        recognized = actual_width == width and row not in expected and any(row)
        for c, text in enumerate(row):
            labels, unknown = associations(text or "", vocabulary)
            if unknown or not labels.issubset(wanted[c] if c < width else set()):
                recognized = False
        if r < header_rows or recognized:
            for c, text in enumerate(row):
                header_text[c] += " " + (text or "")
            if r >= header_rows:
                dropped.append(r)
        else:
            body.append(row)
            body_source_rows.append(r)
    missing = []
    extra = []
    matched = 0
    for c, text in enumerate(header_text):
        labels, unknown = associations(text, vocabulary)
        allowed = wanted[c] if c < width else set()
        matched += len(allowed & labels)
        missing.extend({"column": c, "label": " ".join(label)} for label in sorted(allowed - labels))
        # A shorter vocabulary phrase inside a valid longer title isn't spam.
        unexpected = {label for label in labels - allowed
                      if not any(contains(valid, label) for valid in labels & allowed)}
        extra.extend({"column": c, "label": " ".join(label)} for label in sorted(unexpected))
        extra.extend({"column": c, "unknown_token": token} for token in sorted(unknown))
    header_metric = metric(matched, sum(map(len, wanted)), matched + len(extra))
    pairs = align_rows(expected, body)
    correct = 0
    mismatches = []
    empty_expected = empty_correct = 0
    for er, ar in pairs:
        e = expected[er] if er is not None else []
        a = body[ar] if ar is not None else []
        for c in range(max(len(e), len(a))):
            ev = e[c] if c < len(e) else None
            av = a[c] if c < len(a) else None
            if ev is not None and ev == av:
                if ev:
                    correct += 1
                else:
                    empty_correct += 1
            elif len(mismatches) < 200:
                mismatches.append(dict(reference_row=er, candidate_row=ar, column=c,
                                       expected=ev, actual=av))
            if ev == "":
                empty_expected += 1
    expected_count = sum(bool(v) for row in expected for v in row)
    actual_count = sum(bool(v) for row in body for v in row)
    data_metric = metric(correct, expected_count, actual_count)
    exact = (len(body) == len(expected) and actual_width == width and body == expected)
    shape = min(len(body), len(expected)) / max(len(body), len(expected))
    shape *= min(width, actual_width) / max(width, actual_width)
    empty_accuracy = empty_correct / empty_expected if empty_expected else 1.0
    # Bottleneck score has no learned weights; its dimensions remain visible.
    semantic = min(header_metric["f1"], data_metric["f1"], shape, empty_accuracy)
    topology = None
    if "spans" in ref:
        expected_spans = {tuple(s) for s in ref["spans"]}
        actual_spans = set()
        for r, c, rs, cs in table.spans if table else []:
            if r < header_rows:
                actual_spans.add(("header", r, c, rs, cs))
            elif r in body_source_rows:
                actual_spans.add(("body", body_source_rows.index(r), c, rs, cs))
        topology = metric(len(expected_spans & actual_spans), len(expected_spans), len(actual_spans))
    return dict(reference_id=ref["id"], status="scored", table_found=table is not None,
                semantic_score=semantic, passed=exact and not missing and not extra,
                headers=header_metric, data=data_metric, shape_score=shape,
                empty_cell_accuracy=empty_accuracy, topology=topology,
                missing_headers=missing, unexpected_headers=extra,
                expected_shape=[len(expected), width], actual_shape=[len(body), actual_width],
                repeated_or_supplemental_header_rows=dropped,
                mismatches=mismatches, mismatch_details_truncated=len(mismatches) == 200)


def read_text(path: Path) -> str:
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise InputError(f"input exceeds {MAX_BYTES} bytes: {path}")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise InputError(f"input is not UTF-8: {path}") from exc


def load_json(path: Path) -> dict:
    def unique(pairs):
        result = {}
        for k, v in pairs:
            if k in result:
                raise InputError(f"duplicate JSON key: {k}")
            result[k] = v
        return result
    try:
        return json.loads(read_text(path), object_pairs_hook=unique,
                          parse_constant=lambda v: (_ for _ in ()).throw(InputError(f"nonfinite JSON: {v}")))
    except (json.JSONDecodeError, RecursionError) as exc:
        raise InputError(f"invalid JSON: {path}: {exc}") from exc


def write_report(path: Path, text: str) -> None:
    """Publish complete JSON atomically, preserving an older report on failure."""
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent,
                                         prefix=f".{path.name}.", delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def evaluate(ref_path: Path, candidate_path: Path, index: int = 0, fmt: str = "markdown") -> dict:
    ref = validate_reference(load_json(ref_path))
    text = read_text(candidate_path)
    tables = parse_tables(text, fmt)
    if index < 0:
        raise InputError("table index must be nonnegative")
    result = score_table(ref, tables[index] if index < len(tables) else None)
    result.update(candidate=str(candidate_path), reference=str(ref_path), table_index=index,
                  tables_found=len(tables), candidate_sha256=hashlib.sha256(text.encode()).hexdigest(),
                  reference_sha256=hashlib.sha256(ref_path.read_bytes()).hexdigest())
    return result


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--reference", type=Path)
    ap.add_argument("--candidate", type=Path)
    ap.add_argument("--table-index", type=int, default=0)
    ap.add_argument("--format", choices=["markdown", "html"], default="markdown")
    ap.add_argument("--manifest", type=Path, help="batch paths are relative to the manifest")
    ap.add_argument("--output", type=Path, help="JSON report; stdout if omitted")
    args = ap.parse_args(argv)
    results = []
    try:
        if args.manifest:
            if args.reference or args.candidate or args.table_index != 0 or args.format != "markdown":
                raise InputError("manifest cannot be combined with single-case options")
            manifest = load_json(args.manifest)
            if (not isinstance(manifest, dict) or type(manifest.get("schema_version")) is not int or manifest["schema_version"] != 1
                    or set(manifest) != {"schema_version", "cases"}
                    or not isinstance(manifest["cases"], list) or not 1 <= len(manifest["cases"]) <= 1000):
                raise InputError("invalid manifest (schema_version=1 and 1..1000 cases required)")
            seen = set()
            for case in manifest["cases"]:
                allowed = {"id", "reference", "candidate", "table_index", "format", "notes"}
                if (not isinstance(case, dict) or set(case) - allowed
                        or any(not isinstance(case.get(k), str) or not case[k] for k in ("id", "reference", "candidate"))
                        or type(case.get("table_index", 0)) is not int or case.get("table_index", 0) < 0
                        or case.get("format", "markdown") not in {"markdown", "html"} or case["id"] in seen):
                    raise InputError("invalid or duplicate manifest case")
                seen.add(case["id"])
                try:
                    result = evaluate(args.manifest.parent / case["reference"], args.manifest.parent / case["candidate"],
                                      case.get("table_index", 0), case.get("format", "markdown"))
                    result["case_id"] = case["id"]
                    result["notes"] = case.get("notes", "")
                except (InputError, OSError) as exc:
                    result = dict(case_id=case["id"], status="invalid", error=str(exc))
                results.append(result)
        else:
            if not args.reference or not args.candidate:
                raise InputError("provide --reference and --candidate, or --manifest")
            results.append(evaluate(args.reference, args.candidate, args.table_index, args.format))
    except (InputError, OSError) as exc:
        print(f"table-score: {exc}", file=sys.stderr)
        return 2
    try:
        parser_version = importlib.metadata.version("markdown-it-py")
    except importlib.metadata.PackageNotFoundError:
        parser_version = None
    report = dict(schema_version=1, scorer_version=VERSION, parser_version=parser_version,
                  scorer_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  python_version=sys.version.split()[0],
                  normalization="NFC + whitespace + currency/percent spacing; case-sensitive data; tokenized case-insensitive headers",
                  results=results, summary=dict(cases=len(results),
                    passed=sum(r.get("passed", False) for r in results),
                    failed=sum(r["status"] == "scored" and not r["passed"] for r in results),
                    invalid=sum(r["status"] == "invalid" for r in results)))
    encoded = json.dumps(report, ensure_ascii=False, indent=2, allow_nan=False) + "\n"
    try:
        if args.output:
            write_report(args.output, encoded)
        else:
            sys.stdout.write(encoded)
    except OSError as exc:
        print(f"table-score: cannot write report: {exc}", file=sys.stderr)
        return 2
    return 2 if report["summary"]["invalid"] else (1 if report["summary"]["failed"] else 0)


if __name__ == "__main__":
    raise SystemExit(main())
