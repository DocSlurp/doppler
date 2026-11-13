"""Shards with --context and the estimated body vs a whole run, on local PDFs (not committed).
Usage: shard_identity.py PDFTOMARKDOWN SHARD_PAGES CONTEXT PDF... -- WINDOW ARGS..."""
import json, os, subprocess, sys
argv = sys.argv[1:]
cut = argv.index('--')
PM, SHARD, CTX, pdfs, win = argv[0], int(argv[1]), int(argv[2]), argv[3:cut], argv[cut + 1:]
def rusage_children():
    import resource
    r = resource.getrusage(resource.RUSAGE_CHILDREN); return r.ru_utime + r.ru_stime
tot_same = tot_pages = 0
for pdf in pdfs:
    c0 = rusage_children()
    whole = subprocess.run([PM, *win, pdf], capture_output=True, check=True).stdout.decode().split('\f')
    c1 = rusage_children()
    body = json.loads(subprocess.run([PM, *win, '--estimate-body', pdf], capture_output=True, check=True).stdout)
    c2 = rusage_children()
    n = len(whole); got = []; ident = set()
    for f in range(1, n + 1, SHARD):
        l = min(n, f + SHARD - 1)
        r = subprocess.run([PM, *win, '--report', '--body-size', str(body['body_size']), '--body-color', body['body_color'],
                            '-f', str(f), '-l', str(l), '--context', str(CTX), pdf], capture_output=True, check=True)
        got += r.stdout.decode().split('\f')
        ident |= set(json.loads(r.stderr.decode().splitlines()[0])['range']['identity'])
    c3 = rusage_children()
    same = sum(a == b for a, b in zip(whole, got))
    tot_same += same; tot_pages += n
    print(f"{os.path.basename(pdf)[:24]:24s} pages {n} identical {same}/{len(got)} identity{sorted(ident)} cpu whole {c1-c0:.2f}s estimate {c2-c1:.2f}s shards {c3-c2:.2f}s", flush=True)
print(f'total identical {tot_same}/{tot_pages}')
