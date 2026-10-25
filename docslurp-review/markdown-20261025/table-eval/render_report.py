#!/usr/bin/env python3
"""Render the retained score JSON; all candidate text is HTML escaped."""
from pathlib import Path
import base64
import datetime
import html
import json

ROOT=Path(__file__).resolve().parent
report=json.loads((ROOT/'results.json').read_text())
results=report['results']
stats=report['summary']
summary_text=(f"{stats['cases']} comparisons: {stats['passed']} pass, {stats['failed']} fail, "
              f"{stats['invalid']} invalid")
rows=[]
for r in results:
    if r['status'] != 'scored':
        rows.append(f"| {r['case_id']} | INVALID | — | — | — |")
        continue
    rows.append(f"| {r['case_id']} | {'PASS' if r['passed'] else 'FAIL'} | {r['headers']['recall']:.1%} | {r['data']['f1']:.1%} | {r['semantic_score']:.1%} |")
intro='''# Table extraction evaluation

Implemented scorer: [usage, schema and metric definitions](../../../test/docslurp/table_scoring/README.md).

The retained batch has **SUMMARY_TEXT**. It includes the original outputs and separately labelled header-coverage variants. Paired interpretations of the ambiguous ruled fixture are sensitivity checks, not independent extraction runs. This is a small development set, not a held-out quality estimate.

## What the results mean

- Simple ruled and borderless synthetic outputs pass, including wrapped descriptions, bold headers and repeated labels. The architect rich header text contains literal Markdown emphasis; header words still satisfy semantic coverage, but presentation quality is not scored.
- Radical rich preserves the shared Revenue header through colspan. Strict has identical values but omits Revenue for the last column: header coverage is 2/3. Repeating Revenue in that column is an accepted representation, covered by regression tests and separately labelled coverage outputs. Original outputs remain available for comparison.
- Fidelity's no-repair rich output matches the drawn rules. Its repair output matches the explicitly annotated alternative. Each fails the other interpretation. The fixture cannot support a single unquestioned gold topology, so both results remain visible.
- Fidelity GFM omits the repeated shared title and loses rowspan/merged-cell content when flattened. It fails both interpretations. The lack of HTML spans alone is not a semantic failure.
- Minimalist's real AAPL tax table has seven columns instead of four, with year titles shifted left. Data F1 is 66.7%, but header coverage is zero. This is incorrect column association, not an alignment penalty.
- Fidelity's AAPL operations table merges three numeric columns into one and retains a footnote/footer inside the table. Data F1 is 37.8%; header coverage is 1/9. The header and shape failures remain visible even though many row-label words survive.
- The performance prototype excerpt contains no table syntax. It scores as a missing table. The excerpt is truncated, so this is not a full-page content-retention measurement.

## Results

Header coverage is per-column label recall. Data F1 uses exact normalized values in their aligned row and original column. The semantic summary is the minimum of header F1, data F1, shape similarity and empty-cell accuracy. It is a conservative engineering score, not a calibrated probability. Exact topology is reported separately in JSON and does not gate semantic acceptance.

| Candidate / reference | Gate | Header coverage | Data F1 | Semantic score |
|---|---|---:|---:|---:|
'''
footer='''

## Provenance and reproducibility

- [Batch manifest](manifest.json), [all scores and mismatch details](results.json), [interactive report](report.html).
- [Reference annotations](references/) and [explicit source annotation script](build_references.py). The script contains hand annotations; it never reads candidate Markdown to create gold. Synthetic annotations were checked against retained PDF generators. AAPL tax and operations annotations were checked against page renders; the no-table products excerpt uses retained source layout text.
- Source PDF hashes, candidate/reference hashes and the scorer hash are retained. Original candidate outputs are retained alongside separately labelled coverage variants. Reference ambiguity and missing-table excerpts are explicitly labelled.
- [CTest validation](evidence/ctest.txt): scorer suite passed. The suite contains 33 tests, including 80 deterministic spacing/alignment variants and negative controls for missing headers, copied labels, wrong values, shifted cells, missing/extra rows, limits and invalid input.
- [Fresh tax-table crop](evidence/aapl-tax-closeup.png), [full page](evidence/aapl-page28.png), and [source layout text](evidence/aapl-pages26-28.txt). The source PDF is the cached 2025 AAPL 10-K identified by the annotation hash.

From repository root, with the scorer dependencies installed:

```sh
python test/docslurp/table_scoring/scorer.py \\
  --manifest docslurp-review/markdown-20261025/table-eval/manifest.json \\
  --output docslurp-review/markdown-20261025/table-eval/results.json
# Exit 1 is expected: the saved extraction outputs include quality failures.
python docslurp-review/markdown-20261025/table-eval/render_report.py
python docslurp-review/markdown-20261025/table-eval/check_results.py
```

No model judge, external recognition service or remote compute was used. References have been checked by the coordinator, not independently double-annotated. Further corpus work should include held-out human-labelled tables; these results do not establish production extractor accuracy. The scorer intentionally rejects unsupported nested/visual/CSS-driven table cases instead of assigning misleading quality scores.
'''
(ROOT/'report.md').write_text(intro.replace('SUMMARY_TEXT', summary_text)+'\n'.join(rows)+footer)

def image(path): return 'data:image/png;base64,'+base64.b64encode(path.read_bytes()).decode()
def esc(s): return html.escape(str(s))
def table(head,rs):
    return '<table><thead><tr>'+''.join('<th>'+esc(v)+'</th>' for v in head)+'</tr></thead><tbody>'+''.join('<tr>'+''.join('<td>'+esc(v)+'</td>' for v in row)+'</tr>' for row in rs)+'</tbody></table>'
summary=table(['Case','Header coverage','Data F1','Result'],[
 ['Shared header / rich','3/3','100%','PASS'],['Shared header / strict','2/3','100%','FAIL: missing shared title'],
 ['Synthetic borderless','4/4','100%','PASS'],['AAPL tax / minimalist','0/3','66.7%','FAIL: shifted headers'],
 ['AAPL operations / fidelity','1/9','37.8%','FAIL: merged columns']])
# Actual candidate cells, transcribed verbatim from minimalist/results/aapl-p28.md,
# shown alongside the source; no alignment repair or column renaming.
candidate=table(['2025','2024','2023','','','',''],[
 ['Provision for income taxes','$','20,719','$','29,749','$','16,741'],
 ['Effective tax rate','15.6%','24.1%','14.7%','','',''],
 ['Statutory federal income tax rate','21%','21%','21%','','','']])
allrows=table(['Comparison','Gate','Headers','Data F1','Semantic'],[[r['case_id'],'PASS' if r['passed'] else 'FAIL',f"{r['headers']['recall']:.1%}",f"{r['data']['f1']:.1%}",f"{r['semantic_score']:.1%}"] for r in results])
page='''<!doctype html><html><head><meta charset="utf-8"><style>body{margin:0;font-family:var(--font-sans,system-ui);line-height:1.5;color:var(--foreground,#192230)}h2{font-size:21px;margin:18px 0 8px}p{margin:9px 0}small,.muted{color:var(--muted-foreground,#607080)}table{border-collapse:collapse;width:100%;font-size:12px}td,th{border:1px solid var(--border,#bbc);padding:7px;text-align:left;overflow-wrap:anywhere}img{width:100%;height:auto}.panel{padding:16px;border:1px solid var(--border,#bbc);border-radius:8px;margin:12px 0;overflow:auto}summary{font-weight:600;cursor:pointer}details{margin:18px 0}.status{font-size:20px;font-weight:600}code{overflow-wrap:anywhere}</style></head><body>
<h2>Table scorer: implemented and applied</h2><p class="status">33 regression tests · SUMMARY_TEXT</p><p>Original outputs and labelled header-coverage variants are evaluated together. Paired interpretations of the ambiguous fixture are sensitivity checks. These are development cases, not a broad accuracy estimate.</p>
<p><b>Tolerance is semantic:</b> spacing and alignment differences are ignored; repeated labels and flattened group titles are accepted. Missing headers, shifted data and invented labels still fail. Span topology is scored separately.</p>
'''+summary+'''
<h2>A real failure that spacing tolerance must not hide</h2><div class="panel"><b>Source: AAPL PDF page 28, printed page 25</b><img src="'''+image(ROOT/'evidence/aapl-tax-closeup.png')+'''" alt="Source income tax table with year headers aligned to three numeric columns"><small>Fresh source render inspected 2026-10-25 during this scoring run. Four logical columns.</small></div><div class="panel"><b>Retained minimalist output: seven columns</b>'''+candidate+'''<small>Actual candidate cells, escaped for display. The year titles are one column too far left and dollar symbols occupy separate cells. This is a column-association error.</small></div>
<details><summary>All comparisons and the ambiguous fixture</summary>'''+allrows+'''<p>The ruled synthetic fixture has conflicting rules and text clustering. The rich rules output matches the drawn interpretation; the repair output matches the declared alternative. Neither interpretation is silently selected to favor an output.</p><img src="'''+image(ROOT.parent/'coordinator/table-detail.png')+'''" alt="Historical ruled merged source table closeup"><small>Historical source render, unchanged from the review; retained for interpreting the paired scores.</small></details>
<p><b>Production safeguards:</b> pinned parser, validated references, bounded input and row alignment, deterministic JSON, content hashes, atomic reports, explicit failure codes, and separate unsupported-input status. CLI and batch APIs are documented.</p><p class="muted">Original outputs are retained alongside labelled coverage variants. No model judge or remote compute used. Reporter stopped at completion; model cost telemetry unavailable. Textual grid scoring excludes font/style fidelity, images and CSS layout.</p></body></html>'''
(ROOT/'report.html').write_text(page.replace('SUMMARY_TEXT', esc(summary_text)))
print('Wrote report.md and self-contained report.html')
