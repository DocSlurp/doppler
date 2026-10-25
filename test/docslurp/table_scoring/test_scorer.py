"""Regression and metamorphic tests for semantic table scoring."""

import json
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import scorer


def reference():
    return dict(schema_version=1, id="sales", source={"description": "authored test"},
                headers=[["Region"], ["Revenue", "Q1"], ["Revenue", "Q2"]],
                rows=[["North", "10.5", "12.0"], ["South", "8.1", "9.7"]],
                spans=[["header", 0, 1, 1, 2]])


BASE = """| Region | Revenue — Q1 | Revenue / Q2 |
| :--- | ---: | :---: |
| North | 10.5 | 12.0 |
| South | 8.1 | 9.7 |
"""


def score(text=BASE, ref=None):
    tables = scorer.parse_tables(text)
    return scorer.score_table(ref or reference(), tables[0] if tables else None)


class ScorerTests(unittest.TestCase):
    def test_flattening_semantics_not_topology(self):
        result = score()
        self.assertTrue(result["passed"])
        self.assertEqual(result["semantic_score"], 1)
        self.assertEqual(result["topology"]["recall"], 0)

    def test_html_multirow_spans(self):
        text = """<table><thead><tr><th rowspan=2>Region</th><th colspan=2>Revenue</th></tr>
        <tr><th>Q1</th><th>Q2</th></tr></thead><tbody>
        <tr><th scope=row>North</th><td>10.5</td><td>12.0</td></tr>
        <tr><th scope=row>South</th><td>8.1</td><td>9.7</td></tr></tbody></table>"""
        self.assertTrue(score(text)["passed"])

    def test_random_spacing_alignment(self):
        rng = random.Random(7301)
        for _ in range(80):
            rows = [["Region", "Revenue Q1", "Revenue Q2"],
                    [rng.choice(["---", ":----", "---:", ":---:"]) for _ in range(3)],
                    ["North", "10.5", "12.0"], ["South", "8.1", "9.7"]]
            text = "\n".join("|" + "|".join(" " * rng.randrange(1, 9) + x + " " * rng.randrange(1, 9)
                                                 for x in row) + "|" for row in rows)
            self.assertTrue(score(text)["passed"], text)

    def test_repeated_labels_not_penalized_or_rewarded(self):
        repeated = BASE.replace("Revenue — Q1", "Revenue Revenue Q1 Q1")
        self.assertEqual(score(repeated)["headers"], score()["headers"])
        self.assertTrue(score(repeated)["passed"])

    def test_repeated_page_header(self):
        text = BASE.replace("| South", "| Region | Revenue Q1 | Revenue Q2 |\n| South")
        result = score(text)
        self.assertTrue(result["passed"])
        self.assertEqual(result["repeated_or_supplemental_header_rows"], [2])

    def test_supplemental_header_row(self):
        text = BASE.replace("Revenue — Q1", "Revenue").replace("Revenue / Q2", "Revenue")
        text = text.replace("| North", "| | Q1 | Q2 |\n| North")
        self.assertTrue(score(text)["passed"])

    def test_missing_group_header(self):
        result = score(BASE.replace("Revenue / Q2", "Q2"))
        self.assertFalse(result["passed"])
        self.assertEqual(result["headers"]["recall"], 4 / 5)
        self.assertEqual(result["missing_headers"], [{"column": 2, "label": "revenue"}])

    def test_prose_cannot_supply_headers(self):
        result = score("Revenue Q2\n\n" + BASE.replace("Revenue / Q2", ""))
        self.assertEqual(result["headers"]["recall"], 3 / 5)

    def test_spam_and_invented_headers(self):
        for text in (BASE.replace("Region", "Region Revenue Q1 Q2"),
                     BASE.replace("Revenue / Q2", "Revenue Q2 Invented")):
            result = score(text)
            self.assertFalse(result["passed"])
            self.assertTrue(result["unexpected_headers"])

    def test_data_misassignments_and_inventions(self):
        for text in (BASE.replace("10.5 | 12.0", "12.0 | 10.5"),
                     BASE.replace("10.5", "-10.5"), BASE.replace("10.5", "10,5"),
                     BASE.replace("10.5", "105"), BASE.replace("North", "NORTH"),
                     BASE + "| Fake | 10.5 | 12.0 |\n"):
            result = score(text)
            self.assertFalse(result["passed"], text)
            self.assertLess(result["semantic_score"], 1)

    def test_row_order(self):
        text = BASE.replace("| North | 10.5 | 12.0 |\n| South | 8.1 | 9.7 |",
                            "| South | 8.1 | 9.7 |\n| North | 10.5 | 12.0 |")
        self.assertFalse(score(text)["passed"])

    def test_duplicate_body_rows_penalized(self):
        self.assertFalse(score(BASE + "| South | 8.1 | 9.7 |\n")["passed"])

    def test_missing_row_does_not_shift_every_later_row(self):
        ref = reference()
        ref["rows"].insert(1, ["East", "15", "20"])
        result = score(ref=ref)
        self.assertEqual(result["data"]["matched"], 6)
        self.assertEqual(result["data"]["expected"], 9)
        self.assertFalse(result["passed"])

    def test_empty_cells_are_not_dropped(self):
        ref = reference()
        ref["rows"][0][1] = ""
        self.assertTrue(score(BASE.replace("10.5", ""), ref)["passed"])
        result = score(BASE.replace("10.5 | 12.0", "12.0 |"), ref)
        self.assertFalse(result["passed"])
        self.assertEqual(result["empty_cell_accuracy"], 0)

    def test_body_rowspan_equivalent_to_repeated_labels(self):
        ref = reference()
        ref["rows"][1][0] = "North"
        text = """<table><tr><th>Region<th>Revenue Q1<th>Revenue Q2
        <tr><td rowspan=2>North<td>10.5<td>12.0
        <tr><td>8.1<td>9.7</table>"""
        self.assertTrue(score(text, ref)["passed"])
        self.assertTrue(score(BASE.replace("South", "North"), ref)["passed"])

    def test_markdown_inline_escapes_entities_and_wraps(self):
        ref = reference()
        ref["rows"][0][0] = "North | West & East"
        text = BASE.replace("North", "[North \\| West &amp; East](https://example.org)")
        self.assertTrue(score(text, ref)["passed"])
        text = BASE.replace("North", "**North**<br>West")
        ref["rows"][0][0] = "North West"
        self.assertTrue(score(text, ref)["passed"])

    def test_escaped_code_pipe(self):
        ref = reference(); ref["rows"][0][0] = "a|b"
        self.assertTrue(score(BASE.replace("North", "`a\\|b`"), ref)["passed"])

    def test_unicode_normalization(self):
        ref = reference(); ref["rows"][0][0] = "Café East"
        self.assertTrue(score(BASE.replace("North", "Cafe\u0301\u00a0East"), ref)["passed"])

    def test_numeric_spacing_without_joining_numbers(self):
        self.assertEqual(scorer.normalize("$ 12.50"), scorer.normalize("$12.50"))
        self.assertEqual(scorer.normalize("12.5 %"), scorer.normalize("12.5%"))
        self.assertNotEqual(scorer.normalize("12 50"), scorer.normalize("1250"))

    def test_no_table_and_code_fence(self):
        for text in ("North 10.5 12.0", "```markdown\n" + BASE + "```\n"):
            result = score(text)
            self.assertFalse(result["table_found"])
            self.assertEqual(result["semantic_score"], 0)

    def test_literal_emphasis_in_html_is_not_interpreted(self):
        text = "<table><tr><th>Region<th>Revenue Q1<th>Revenue Q2<tr><td>**North**<td>10.5<td>12.0<tr><td>South<td>8.1<td>9.7</table>"
        self.assertFalse(score(text)["passed"])

    def test_no_headers_required_for_unlabelled_column(self):
        ref = reference(); ref["headers"][0] = []
        self.assertTrue(score(BASE.replace("Region", ""), ref)["passed"])

    def test_all_header_like_data_is_not_deduplicated(self):
        ref = reference(); ref["rows"] = [["Region", "Revenue Q1", "Revenue Q2"]]
        text = "\n".join(BASE.splitlines()[:2]) + "\n| Region | Revenue Q1 | Revenue Q2 |"
        self.assertTrue(score(text, ref)["passed"])

    def test_row_headers_are_data(self):
        ref = reference(); ref["headers"] = [["Region"]]; ref["rows"] = [["North"], ["South"]]; ref.pop("spans")
        text = '<table><tr><th>Region<tr><th scope="row">North<tr><th scope="row">South</table>'
        self.assertTrue(score(text, ref)["passed"])

    def test_large_exact_table_has_linear_fast_path(self):
        rows = [[str(i)] for i in range(2000)]
        self.assertEqual(len(scorer.align_rows(rows, rows)), 2000)

    def test_bounded_alignment(self):
        with self.assertRaises(scorer.InputError):
            scorer.align_rows([["x"]] * 1500, [["y"]] * 1500)

    def test_invalid_html_and_resource_limits(self):
        cases = ["<table><tr><td rowspan=0>x</td></tr></table>",
                 "<table><tr><td colspan=99999>x</td></tr></table>",
                 "<table><tr><td rowspan=2>x</td></tr></table>",
                 "<table><tr><td>x</tr><tr><td>x<td>y</tr></table>",
                 "<table><tr><td>x<table></table></td></tr></table>",
                 "<table><tr><td>x</td></tr>",
                 "<table><tr><td style='display:none'>x</td></tr></table>",
                 "<table hidden><tr><td>x</td></tr></table>",
                 "<table><tr><td><img src='https://example.org/x'></td></tr></table>",
                 "<table><tr><td><code>a  b</code></td></tr></table>",
                 "<table><tr><td>a<td rowspan=2>b<tr><td colspan=2>c</table>",
                 BASE.replace("10.5 | 12.0", "10.5 | 12.0 | SURPLUS")]
        for text in cases:
            with self.subTest(text=text), self.assertRaises(scorer.InputError):
                scorer.parse_tables(text)

    def test_reference_validation(self):
        for key, value in [("schema_version", 2), ("headers", []), ("rows", [[1, 2, 3]]),
                           ("rows", [["a"]]), ("source", {}), ("spans", [["body", 0, 2, 1, 2]])]:
            ref = reference(); ref[key] = value
            with self.subTest(key=key), self.assertRaises(scorer.InputError):
                scorer.validate_reference(ref)
        ref = reference(); ref["unexpected"] = 1
        with self.assertRaises(scorer.InputError):
            scorer.validate_reference(ref)

    def test_multiple_tables_and_explicit_selection(self):
        tables = scorer.parse_tables(BASE + "\n\n" + BASE.replace("10.5", "999"))
        self.assertEqual(len(tables), 2)
        self.assertTrue(scorer.score_table(reference(), tables[0])["passed"])
        self.assertFalse(scorer.score_table(reference(), tables[1])["passed"])

    def test_atomic_report(self):
        with tempfile.TemporaryDirectory(prefix="table-score-") as d:
            target = Path(d) / "report.json"
            scorer.write_report(target, '{"ok": true}\n')
            scorer.write_report(target, '{"ok": false}\n')
            self.assertEqual(json.loads(target.read_text()), {"ok": False})
            self.assertEqual(list(Path(d).iterdir()), [target])

    def test_script_text_is_not_evaluated_or_used(self):
        text = "<table><tr><th>Region<th>Revenue Q1<th>Revenue Q2<tr><td>North<script>evil()</script><td>10.5<td>12.0<tr><td>South<td>8.1<td>9.7</table>"
        self.assertTrue(score(text)["passed"])

    def test_determinism(self):
        self.assertEqual(json.dumps(score(), sort_keys=True), json.dumps(score(), sort_keys=True))

    def test_cli_exit_codes_and_batch_invalid(self):
        with tempfile.TemporaryDirectory(prefix="table-score-") as d:
            root = Path(d); ref = root / "ref.json"; md = root / "table.md"
            ref.write_text(json.dumps(reference())); md.write_text(BASE)
            cmd = [sys.executable, str(Path(scorer.__file__)), "--reference", str(ref), "--candidate", str(md)]
            run = subprocess.run(cmd, capture_output=True, text=True)
            self.assertEqual(run.returncode, 0, run.stderr)
            md.write_text(BASE.replace("12.0", "999"))
            self.assertEqual(subprocess.run(cmd, capture_output=True).returncode, 1)
            ref.write_text('{"schema_version":1,"schema_version":1}')
            self.assertEqual(subprocess.run(cmd, capture_output=True).returncode, 2)
            manifest = root / "suite.json"
            manifest.write_text(json.dumps({"schema_version": 1, "cases": [
                {"id": "bad", "reference": "ref.json", "candidate": "table.md"}]}))
            run = subprocess.run([sys.executable, str(Path(scorer.__file__)), "--manifest", str(manifest)],
                                 capture_output=True, text=True)
            self.assertEqual(run.returncode, 2)
            self.assertEqual(json.loads(run.stdout)["summary"]["invalid"], 1)


    def test_joined_column_titles(self):
        # Flattened multi-level headers may join group and leaf titles with
        # any separator; each still covers both labels of its own column.
        for sep in ("_", "/", ".", " > ", " → ", "::", " - ", " "):
            text = BASE.replace("Revenue — Q1", f"Revenue{sep}Q1").replace("Revenue / Q2", f"Revenue{sep}Q2")
            self.assertTrue(score(text)["passed"], repr(sep))

    def test_header_presentation_variants(self):
        rows = "| North | 10.5 | 12.0 |\n| South | 8.1 | 9.7 |\n"
        variants = [
            "Region | Revenue Q1 | Revenue Q2\n--- | --- | ---\n" + rows.replace("| ", "", 1),
            "| Region | Revenue<br>Q1 | Revenue<br/>Q2 |\n|---|---|---|\n" + rows,
            "| **REGION** | *Revenue* (Q1) | Q2 revenue |\n|-|-:|:-:|\n" + rows,
            "| Region | Revenue | Revenue |\n|---|---|---|\n| Region | Q1 | Q2 |\n" + rows,
        ]
        for text in variants:
            self.assertTrue(score(text)["passed"], text)

    def test_percent_spacing_variants(self):
        ref = reference()
        ref["rows"] = [["North", "(6)%", "—%"], ["South", "4%", "−2%"]]
        text = BASE.replace("10.5", "(6) %").replace("12.0", "— %").replace("8.1", "4 %").replace("9.7", "−2 %")
        self.assertTrue(score(text, ref)["passed"])
        # a space before % never merges distinct values, but a changed value still fails
        self.assertFalse(score(text.replace("(6) %", "(5) %"), ref)["passed"])

    def test_underscore_labels_still_match_themselves(self):
        ref = reference()
        ref["headers"] = [["Region"], ["rev_q1"], ["rev_q2"]]
        text = BASE.replace("Revenue — Q1", "rev_q1").replace("Revenue / Q2", "rev q2")
        self.assertTrue(score(text, ref)["passed"])


if __name__ == "__main__":
    # The repository test helper supplies --bindir to all Python tests.
    if "--bindir" in sys.argv:
        i = sys.argv.index("--bindir"); del sys.argv[i:i + 2]
    unittest.main()
