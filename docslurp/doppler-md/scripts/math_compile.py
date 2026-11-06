"""Do the formulas that `pdftomarkdown --math` writes compile? Typesets every `$...$` and
`$$...$$` of the converter's output with LuaLaTeX (amsmath, amssymb, unicode-math) in a
TeX Live container and counts formulas whose line has an error.

    python3 -I scripts/math_compile.py --manifest M.json --splits S.json --split development \
        --pdftotext PATH --converter target/release/pdftomarkdown [--image texlive/texlive:latest-medium]
        [--keep DIR]

`--keep DIR` keeps the .tex and .pdf (one page per input file) for visual checks.
"""
import argparse, json, os, re, shutil, subprocess, sys, tempfile
from pathlib import Path

MATH = re.compile(r"(?<!\\)\$\$(.+?)(?<!\\)\$\$|(?<!\\)\$((?:\\\$|[^$\n])+?)(?<!\\)\$", re.S)
PREAMBLE = r"""\documentclass[10pt]{article}
\usepackage[margin=1cm,paperwidth=21cm,paperheight=60cm]{geometry}
\usepackage{amsmath,amssymb}
\usepackage{unicode-math}
\setmathfont{STIX Two Math}
\setlength{\parindent}{0pt}
\begin{document}
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--manifest', required=True)
    ap.add_argument('--splits', required=True)
    ap.add_argument('--split', default='development')
    ap.add_argument('--pdftotext', required=True)
    ap.add_argument('--converter', required=True)
    ap.add_argument('--image', default='texlive-math-extra:local')
    ap.add_argument('--keep')
    args = ap.parse_args()
    if args.split not in ('development', 'calibration'):
        sys.exit('only development or calibration splits are run here')
    root = Path(args.manifest).resolve().parent
    files = json.loads(Path(args.manifest).read_text())['files']
    splits = json.loads(Path(args.splits).read_text())['splits']
    env = dict(os.environ, PDFTOTEXT=args.pdftotext)
    work = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix='mathcompile.', dir=os.environ.get('TMPDIR')))
    work.mkdir(parents=True, exist_ok=True)
    index = {}
    for f in files:
        if splits.get(f['id']) != args.split:
            continue
        md = subprocess.run([args.converter, '--math', str(root / f['path'])], capture_output=True, text=True, env=env, timeout=600).stdout
        lines, formulas = [PREAMBLE], []
        for m in MATH.finditer(md):
            display = m.group(1) is not None
            tex = (m.group(1) if display else m.group(2)).replace('\n', ' ')
            formulas.append({'display': display, 'tex': tex})
            lines.append(f'\\noindent[{len(formulas)}] ' + (f'\\[{tex}\\]' if display else f'${tex}$') + '\\par\n')
        lines.append('\\end{document}\n')
        (work / f"{f['id']}.tex").write_text(''.join(lines))
        index[f['id']] = {'producer': f.get('producer', ''), 'formulas': formulas, 'first_line': PREAMBLE.count('\n') + 1}
    run = 'for t in *.tex; do lualatex -interaction=nonstopmode -halt-on-error=false "$t" >/dev/null 2>&1; done'
    subprocess.run(['docker', 'run', '--rm', '--user', f'{os.getuid()}:{os.getgid()}', '-e', 'HOME=/tmp', '-v', f'{work}:/w', '-w', '/w', args.image, 'sh', '-c', run],
                   check=False, timeout=7200, capture_output=True)
    total = bad = 0
    per = {}
    for fid, info in index.items():
        log = (work / f'{fid}.log').read_text(errors='replace') if (work / f'{fid}.log').exists() else ''
        err_lines = {int(n) for n in re.findall(r'^l\.(\d+)', log, re.M)}
        fatal = 'Fatal error' in log or not (work / f'{fid}.pdf').exists()
        n = len(info['formulas'])
        bad_here = [k for k in range(n) if (info['first_line'] + k) in err_lines]
        if fatal and n:
            bad_here = list(range(n))
        total += n
        bad += len(bad_here)
        per[fid] = {'formulas': n, 'errors': len(bad_here), 'producer': info['producer'],
                    'sample': [info['formulas'][k]['tex'][:160] for k in bad_here[:3]]}
    out = {'split': args.split, 'files': len(index), 'formulas': total, 'compileErrors': bad,
           'compileRate': round(1 - bad / total, 4) if total else None, 'perFile': per}
    print(json.dumps({k: v for k, v in out.items() if k != 'perFile'}, indent=1))
    for fid, p in sorted(per.items(), key=lambda kv: -kv[1]['errors'])[:12]:
        if p['errors']:
            print(f"{fid:36} {p['errors']:3}/{p['formulas']:4} {p['sample'][:2]}")
    if not args.keep:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == '__main__':
    main()
