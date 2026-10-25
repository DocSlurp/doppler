#!/usr/bin/env python3
"""Explicit source annotations. Never reads candidate Markdown to construct gold."""
from pathlib import Path
import hashlib
import json

ROOT = Path(__file__).resolve().parent
REVIEW = ROOT.parent
AAPL = Path('/home/dan/.cache/docslurp/pricing-samples-2026-10-21/sec-10k/aapl-10k-fy2025-as-filed.pdf')

def save(name, headers, rows, source, description, spans=None, notes=''):
    path = source if isinstance(source, Path) else REVIEW / source
    obj = dict(schema_version=1, id=name,
        source=dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest(), description=description),
        headers=headers, rows=rows, notes=notes)
    if spans is not None: obj['spans'] = spans
    (ROOT/'references'/f'{name}.json').write_text(json.dumps(obj, ensure_ascii=False, indent=2)+'\n')

save('minimalist-synth', [['Region'],['Q1'],['Q2'],['Total']],
     [['North','10','12','22'],['South','7','9','16']], 'minimalist/fixtures/synth.pdf',
     'Hand annotation verified against minimalist/fixtures/synth.ps table instructions.', [])
for name in ['tagged','untagged']:
    save('architect-'+name, [['Item'],['Qty'],['Price']], [['Apple','10','1.20'],['Pear','4','2.05']],
         f'architect/fixtures/{name}.pdf', 'Verified against architect/src/mkfixtures.py cells array.', [])
save('radical-ruled', [['Region'],['Q1'],['Q2']], [['North','10.5','12.0'],['South','8.1','9.7']],
     'radical/fixtures/synthetic-2p.pdf', 'Page 1 ruled table; verified against radical/src/make_fixture.py.', [])
save('radical-borderless', [['Item'],['Qty'],['Price']], [['Widget','4','3.50'],['Gadget','12','1.25']],
     'radical/fixtures/synthetic-2p.pdf', 'Page 1 borderless table; verified against radical/src/make_fixture.py.', [])
save('radical-span', [['Region'],['Revenue'],['Revenue']], [['North','10.5','12.0'],['South','8.1','9.7']],
     'radical/fixtures/span-table-1p.pdf', 'Header Revenue spans columns 1 and 2; radical/src/make_fixture_span.py.',
     [['header',0,1,1,2]], 'No child period labels exist in this source: do not invent Q1/Q2.')
save('fidelity-borderless', [['Item'],['Units'],['Unit price'],['Total']],
     [['Widget alpha','12','3.50','42.00'],['Widget beta with a long description wrapped','7','10.25','71.75'],['Gadget','100','0.99','99.00']],
     'fidelity/fixtures/borderless.pdf', 'fidelity/src/mkfixtures.py and retained page render; wrapped description belongs to Widget beta.', [])
headers=[['Region'],['2025 revenue (USD m)'],['2025 revenue (USD m)'],['Growth']]
rows=[['EMEA','Q1','1,204.5','+4.1%'],['EMEA','Q2','1,311.0','+8.9%'],
      ['APAC','Q1','980.2 -1.2%','980.2 -1.2%'],['Total (colspan 3)','','+3.9%','+3.9%']]
spans=[['header',0,1,1,2],['body',0,0,2,1],['body',2,2,1,2],['body',3,2,1,2]]
save('fidelity-ruled-drawn',headers,rows,'fidelity/fixtures/ruled_merged.pdf',
     'Drawn-rule interpretation; verified against generator, rule coordinates and coordinator/table-detail.png.',spans,
     'Ambiguous fixture. Vertical x372 ends before APAC. Footer rules at x172/x272 survive white fills. Duplicate paint of Total is one visible label. Span-expanded values repeat in covered slots. Not a universal semantic truth.')
rows[2]=['APAC','Q1','980.2','-1.2%']
save('fidelity-ruled-repaired',headers,rows,'fidelity/fixtures/ruled_merged.pdf',
     'Alternative annotation: APAC text clusters split despite absent ruling; footer follows visible geometry.',
     [s for s in spans if s != ['body',2,2,1,2]],
     'Explicit plausible alternative, not unquestioned gold. Both scores must be shown; do not pick the best after seeing output.')
save('aapl-tax-p28',[[],['2025'],['2024'],['2023']],
     [['Provision for income taxes','$20,719','$29,749','$16,741'],
      ['Effective tax rate','15.6%','24.1%','14.7%'],['Statutory federal income tax rate','21%','21%','21%']],
     AAPL, 'PDF page 28 / printed page25, Provision for Income Taxes. Hand checked against evidence/aapl-page28.png.', [],
     'Currency symbols are part of numeric cells, not standalone columns. The row-label column has no printed header.')
ops=[['Net sales:','','',''], ['Products','$307,003','$294,866','$298,085'],
     ['Services','109,158','96,169','85,200'],['Total net sales','416,161','391,035','383,285'],
     ['Cost of sales:','','',''],['Products','194,116','185,233','189,282'],['Services','26,844','25,119','24,855'],
     ['Total cost of sales','220,960','210,352','214,137'],['Gross margin','195,201','180,683','169,148'],
     ['Operating expenses:','','',''],['Research and development','34,550','31,370','29,915'],
     ['Selling, general and administrative','27,601','26,097','24,932'],['Total operating expenses','62,151','57,467','54,847'],
     ['Operating income','133,050','123,216','114,301'],['Other income/(expense), net','(321)','269','(565)'],
     ['Income before provision for income taxes','132,729','123,485','113,736'],['Provision for income taxes','20,719','29,749','16,741'],
     ['Net income','$112,010','$93,736','$96,995'],['Earnings per share:','','',''],['Basic','$7.49','$6.11','$6.16'],
     ['Diluted','$7.46','$6.08','$6.13'],['Shares used in computing earnings per share:','','',''],
     ['Basic','14,948,500','15,343,783','15,744,231'],['Diluted','15,004,697','15,408,095','15,812,547']]
save('aapl-ops-p32',[[],['Years ended','September 27','2025'],['Years ended','September 28','2024'],['Years ended','September 30','2023']],ops,
     AAPL,'PDF page32 / printed29; hand checked against fidelity/results/images/real_aapl_ops_page.png.',None,
     'Section label rows retained, outside footnote/footer excluded. All years and dates belong to their numeric columns. Units in narrative are outside table-header scoring.')
save('aapl-products-p26',[[],['2025'],['Change'],['2024'],['Change'],['2023']],
     [['iPhone','$209,586','4%','$201,183','—%','$200,583'],['Mac','33,708','12%','29,984','2%','29,357'],
      ['iPad','28,023','5%','26,694','(6)%','28,300'],['Wearables, Home and Accessories','35,686','(4)%','37,005','(7)%','39,845'],
      ['Services (1)','109,158','14%','96,169','13%','85,200'],['Total net sales','$416,161','6%','$391,035','2%','$383,285']],
     AAPL,'PDF page26 / printed23; source layout text evidence/aapl-pages26-28.txt. Expected table presence and dimensions.',None,
     'Candidate is a retained truncated failure excerpt. No table exists there; score is table detection failure, not a full-page content-loss estimate.')

cases=[]
def case(id,ref,path,index=0,notes=''):
    cases.append(dict(id=id,reference=f'references/{ref}.json',candidate='../'+path,table_index=index,notes=notes))
case('minimalist-synthetic','minimalist-synth','minimalist/results/synth.md')
for name,mode in [('tagged','gfm'),('tagged','rich'),('untagged','gfm')]:
    case(f'architect-{name}-{mode}',f'architect-{name}',f'architect/results/{name}.{mode}.md')
for mode in ['strict','rich']:
    for index,name in enumerate(['ruled','borderless']):
        case(f'radical-{name}-{mode}',f'radical-{name}',f'radical/results/synthetic-2p.{mode}.md',index)
    case(f'radical-span-{mode}','radical-span',f'radical/results/span-table-1p.{mode}.md')
case('fidelity-borderless','fidelity-borderless','fidelity/results/borderless.proto.md')
for variant,file in [('rich-repair','ruled_merged.proto.md'),('rich-rules','ruled_merged.proto.norepair.md'),('gfm','ruled_merged.proto.gfm.md')]:
    for interpretation in ['drawn','repaired']:
        case(f'fidelity-{variant}-vs-{interpretation}',f'fidelity-ruled-{interpretation}',f'fidelity/results/{file}',notes='Ambiguous reference sensitivity analysis; not independent trials.')
case('minimalist-aapl-tax','aapl-tax-p28','minimalist/results/aapl-p28.md')
case('fidelity-aapl-operations','aapl-ops-p32','fidelity/results/real_aapl_ops_page.proto.md')
case('performance-aapl-products-excerpt','aapl-products-p26','performance/results/samples/aapl-p26-table-failure.md',notes='Truncated retained excerpt, no detected table; not a full-document evaluation.')
(ROOT/'manifest.json').write_text(json.dumps(dict(schema_version=1,cases=cases),indent=2)+'\n')
print(f'{len(cases)} cases; {len(list((ROOT/"references").glob("*.json")))} source annotations')
