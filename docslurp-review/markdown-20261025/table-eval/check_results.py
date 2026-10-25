#!/usr/bin/env python3
"""Check expected development outcomes and reproducibility hashes."""
import hashlib
import json
from pathlib import Path
root=Path(__file__).resolve().parent
repo=root.parents[2]
report=json.loads((root/'results.json').read_text())
assert report['summary']==dict(cases=26,passed=17,failed=9,invalid=0),report['summary']
expected_failures={'radical-span-strict','fidelity-rich-repair-vs-drawn','fidelity-rich-rules-vs-repaired',
 'fidelity-gfm-vs-drawn','fidelity-gfm-vs-repaired','minimalist-aapl-tax','fidelity-aapl-operations',
 'performance-aapl-products-excerpt',
 'fidelity-gfm-coverage-vs-drawn'}  # data differs only by the ambiguous fixture's drawn rules
assert {r['case_id'] for r in report['results'] if not r['passed']}==expected_failures
assert hashlib.sha256((repo/'test/docslurp/table_scoring/scorer.py').read_bytes()).hexdigest()==report['scorer_sha256']
for r in report['results']:
 for key in ['candidate','reference']:
  p=Path(r[key]);p=p if p.is_absolute() else repo/p
  assert hashlib.sha256(p.read_bytes()).hexdigest()==r[key+'_sha256'],(r['case_id'],key)
 ref=json.loads((repo/r['reference']).read_text())
 src=Path(ref['source']['path'])
 if src.exists():assert hashlib.sha256(src.read_bytes()).hexdigest()==ref['source']['sha256']
byid={r['case_id']:r for r in report['results']}
assert byid['radical-span-strict']['headers']['recall']==2/3
assert byid['radical-span-strict']['data']['f1']==1
assert byid['radical-span-rich']['passed']
assert byid['minimalist-aapl-tax']['actual_shape']==[3,7]
assert byid['fidelity-aapl-operations']['actual_shape']==[27,2]
# header-coverage exporter changes: every column keeps its source header labels
for cid in ['radical-span-strict-coverage','fidelity-gfm-coverage-vs-drawn','fidelity-gfm-coverage-vs-repaired',
            'minimalist-aapl-tax-coverage','minimalist-aapl-ops-coverage','minimalist-aapl-products-coverage']:
 assert byid[cid]['headers']['recall']==1 and byid[cid]['headers']['precision']==1,cid
assert byid['minimalist-aapl-ops-coverage']['actual_shape']==[24,4]
print('All 26 expected outcomes and retained hashes verified.')
