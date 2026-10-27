import argparse
import concurrent.futures
import importlib.util
import json
import os
from pathlib import Path
import random
import re
import resource
import statistics
import subprocess
import time
import xml.etree.ElementTree as ET

ROOT = Path('/home/dan/code/oss/poppler')
OUT = ROOT / 'docslurp-review/architecture-20261027'
BASE_CPP = '/home/dan/worktrees/poppler/bbox-logical-rtl/build/utils/pdftotext'
NEW_CPP = str(ROOT / 'build/utils/pdftotext')
BASE_RUST = '/home/dan/worktrees/poppler/doppler-md/docslurp/doppler-md/target/release/doppler-md'
NEW_RUST = str(ROOT / 'docslurp/doppler-md/target/release/doppler-md')
GOLDEN = Path('/home/dan/data/docslurp-evals/doppler-md-golden/raw')


def run(cmd, data=None):
    p = subprocess.run(cmd, input=data, capture_output=True, timeout=90)
    return p.returncode, p.stdout, p.stderr


def corpus():
    files = json.loads((OUT / 'baseline.json').read_text())['corpus']
    modes = {'text': [], 'layout': ['-layout'], 'raw': ['-raw'], 'bbox': ['-bbox'],
             'bbox-layout': ['-bbox-layout'], 'tsv': ['-tsv'],
             'bbox-meta': ['-bbox', '-wordmeta'],
             'bbox-layout-meta': ['-bbox-layout', '-wordmeta'],
             'tsv-meta': ['-tsv', '-wordmeta'],
             'bbox-layout-rules': ['-bbox-layout', '-wordmeta', '-rules']}
    def one(job):
        mode, flags, name = job
        a = run([BASE_CPP, *flags, name, '-'])
        b = run([NEW_CPP, *flags, name, '-'])
        result = {'mode': mode, 'file': name, 'baseline_exit': a[0], 'candidate_exit': b[0]}
        if a[0] != b[0]:
            result['status'] = 'unexpected_exit'
        elif a[0] != 0:
            result['status'] = 'matched_failure'
        elif a[1] == b[1]:
            result['status'] = 'identical'
        elif mode.startswith('bbox-layout'):
            # Only line bounds may change. Check all other bytes and independently
            # verify candidate line boxes contain exactly the union of their words.
            strip = lambda s: re.sub(rb'<line\b[^>]*>', b'<line>', s)
            if strip(a[1]) != strip(b[1]):
                result['status'] = 'unexpected_content'
            else:
                root = ET.fromstring(b[1])
                valid = True
                for line in root.iter():
                    if line.tag.rsplit('}', 1)[-1] != 'line':
                        continue
                    words = [w for w in line if w.tag.rsplit('}', 1)[-1] == 'word']
                    if not words:
                        continue
                    for attr, choose in [('xMin', min), ('yMin', min), ('xMax', max), ('yMax', max)]:
                        want = choose(float(w.attrib[attr]) for w in words)
                        valid &= abs(float(line.attrib[attr]) - want) < 0.000002
                result['status'] = 'corrected_line_bounds' if valid else 'unexpected_geometry'
        else:
            result['status'] = 'unexpected_content'
        return result
    jobs = [(mode, flags, name) for mode, flags in modes.items() for name in files]
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        results = list(pool.map(one, jobs))
    summary = {}
    for r in results:
        counts = summary.setdefault(r['mode'], {})
        counts[r['status']] = counts.get(r['status'], 0) + 1
    (OUT / 'corpus-results.json').write_text(json.dumps({'summary': summary, 'results': results}, indent=2)+'\n')
    print(json.dumps(summary, indent=2), flush=True)
    assert not any(r['status'].startswith('unexpected') for r in results), 'unexpected corpus differences'


def fuzz():
    source = ROOT / 'docslurp/doppler-md/scripts/make_adversarial.py'
    spec = importlib.util.spec_from_file_location('adversarial', source)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    rng = random.Random(271027)
    cases, mismatches = 1000, []
    for i in range(cases):
        data = mod.fuzz_doc(rng).encode()
        params = [{} , {'max_page_work': 50}, {'max_doc_work': 100},
                  {'max_page_lines': 15}, {'max_page_rules': 3}][i % 5]
        args = ['--report', '--params', json.dumps(params)]
        a, b = run([BASE_RUST, *args], data), run([NEW_RUST, *args], data)
        ar, br = json.loads(a[2]), json.loads(b[2])
        added = br.pop('rules_truncated_pages', None)
        if a[:2] != b[:2] or ar != br or added != []:
            mismatches.append(i)
            (OUT / f'fuzz-mismatch-{i}.raw').write_bytes(data)
            (OUT / f'fuzz-mismatch-{i}.json').write_text(json.dumps({'params': params,'before':ar,'after':br,'added':added},indent=2))
        if (i+1) % 250 == 0:
            print(f'fuzz {i+1}/{cases}, mismatches={len(mismatches)}', flush=True)
    result = {'seed':271027,'cases':cases,'matching':cases-len(mismatches),'mismatches':mismatches,
              'comparison':'exit status, Markdown bytes, all previous report fields; new truncation field must be empty'}
    (OUT / 'fuzz-results.json').write_text(json.dumps(result,indent=2)+'\n')
    assert not mismatches, 'differential mismatches'


def perf():
    raws = sorted(GOLDEN.glob('*.raw'))
    assert len(raws) == 258
    pdf = '/usr/share/doc/ghostscript/Ghostscript.pdf'
    workloads = {
        'rust_258_golden_documents': ([BASE_RUST], [NEW_RUST], [str(p) for p in raws], []),
        'cpp_bbox_layout_wordmeta_rules_477_pages': ([BASE_CPP,'-bbox-layout','-wordmeta','-rules'],
          [NEW_CPP,'-bbox-layout','-wordmeta','-rules'], [pdf], ['-']),
        'cpp_tsv_wordmeta_477_pages': ([BASE_CPP,'-tsv','-wordmeta'],[NEW_CPP,'-tsv','-wordmeta'],[pdf],['-'])}
    result={'observed_at':time.strftime('%Y-%m-%d %H:%M:%S %Z'), 'rounds':5,
            'method':'warm both, alternating order, serial child launches, stdout /dev/null; median user and total CPU',
            'load_start':os.getloadavg(),'workloads':{}}
    def batch(prefix,files,suffix):
        a=resource.getrusage(resource.RUSAGE_CHILDREN)
        for f in files:
            subprocess.run([*prefix,f,*suffix],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True,timeout=90)
        b=resource.getrusage(resource.RUSAGE_CHILDREN)
        return {'user_s':b.ru_utime-a.ru_utime,'system_s':b.ru_stime-a.ru_stime,
                'total_s':b.ru_utime+b.ru_stime-a.ru_utime-a.ru_stime}
    for name,(base,new,files,suffix) in workloads.items():
        batch(base,files,suffix);batch(new,files,suffix)
        rounds={'baseline':[],'candidate':[]}
        for i in range(5):
            for label,prefix in ([('baseline',base),('candidate',new)] if i%2==0 else [('candidate',new),('baseline',base)]):
                rounds[label].append(batch(prefix,files,suffix))
        stats={label:{k:statistics.median(x[k] for x in values) for k in values[0]} for label,values in rounds.items()}
        result['workloads'][name]={'runs':rounds,'median':stats,
          'user_ratio':stats['candidate']['user_s']/stats['baseline']['user_s'],
          'total_ratio':stats['candidate']['total_s']/stats['baseline']['total_s']}
        print(name,json.dumps(result['workloads'][name]),flush=True)
    result['load_end']=os.getloadavg()
    (OUT/'performance.json').write_text(json.dumps(result,indent=2)+'\n')


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('phase',choices=['corpus','fuzz','perf'])
    args=parser.parse_args()
    {'corpus':corpus,'fuzz':fuzz,'perf':perf}[args.phase]()
