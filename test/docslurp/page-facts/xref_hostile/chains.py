"""Hostile /Prev chains (issue 45): a loop, 100,000 empty sections, an older xref stream of
/Size 50,000,000 (a Flate bomb), an older section starting at object 50,000,000, and 1,000
trailers whose strings run over 4 MB. With the Ghostscript manual installed, also the manual
with 999 and 1,200 incremental updates (perf only). Usage: chains.py OUT_DIR"""
import re, zlib, struct, sys, os
D = sys.argv[1]
GS = '/usr/share/doc/ghostscript/Ghostscript.pdf'
gs = open(GS, 'rb').read() if os.path.exists(GS) else None
if gs:
  sx = int(re.findall(rb'startxref\s+(\d+)', gs)[-1])
  # root ref from the trailer or xref stream dict
  root = re.findall(rb'/Root\s+(\d+)\s+(\d+)\s+R', gs)[-1]
  size = max(int(x) for x in re.findall(rb'/Size\s+(\d+)', gs))
  def incr(n_updates, path):
      out = bytearray(gs); prev = sx; nxt = size
      if not out.endswith(b'\n'): out += b'\n'
      for k in range(n_updates):
          num = nxt; nxt += 1
          at = len(out)
          out += b'%d 0 obj\n<< /Producer (update %d) >>\nendobj\n' % (num, k)
          xat = len(out)
          out += b'xref\n0 1\n0000000000 65535 f \n%d 1\n%010d 00000 n \ntrailer\n<< /Size %d /Root %s %s R /Info %d 0 R /Prev %d >>\nstartxref\n%d\n%%%%EOF\n' % (num, at, nxt, root[0], root[1], num, prev, xat)
          prev = xat
      open(path, 'wb').write(out)
  incr(999, os.path.join(D, 'gs-incr-999.pdf'))
  incr(1200, os.path.join(D, 'gs-incr-1200.pdf'))

def base_objs():
    body = bytearray(b'%PDF-1.5\n')
    off = {}
    objs = {1: b'<< /Type /Catalog /Pages 2 0 R >>', 2: b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>', 3: b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>', 5: b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'}
    c = b'BT /F1 12 Tf 72 700 Td (Hostile chain) Tj ET'
    objs[4] = b'<< /Length %d >>\nstream\n' % len(c) + c + b'\nendstream'
    for n in sorted(objs):
        off[n] = len(body); body += b'%d 0 obj\n' % n + objs[n] + b'\nendobj\n'
    return body, off
def table(off, prev=None, extra=b''):
    t = b'xref\n0 6\n0000000000 65535 f \n' + b''.join(b'%010d 00000 n \n' % off[n] for n in range(1, 6))
    return t + b'trailer\n<< /Size 6 /Root 1 0 R%s%s >>\n' % (b' /Prev %d' % prev if prev is not None else b'', extra)
# loop: newest -> older -> newest
b, off = base_objs()
a1 = len(b); b += b'xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 6 /Root 1 0 R /Prev %010d >>\n' % 0
a2 = len(b); b += table(off, a1)
b = b.replace(b'/Prev 0000000000', b'/Prev %010d' % a2)
b += b'startxref\n%d\n%%%%EOF\n' % a2
open(os.path.join(D, 'h-loop.pdf'), 'wb').write(b)
# deep: 100,000 tiny sections
b, off = base_objs(); prev = len(b); b += table(off)
for k in range(100000):
    at = len(b); b += b'xref\n0 0\ntrailer\n<< /Size 6 /Root 1 0 R /Prev %d >>\n' % prev; prev = at
b += b'startxref\n%d\n%%%%EOF\n' % prev
open(os.path.join(D, 'h-deep.pdf'), 'wb').write(b)
# flate-bomb older xref stream: /Size 50,000,000
b, off = base_objs()
co = zlib.compressobj(9)
rows = b''.join(co.compress(b'\x00' * (7 * 1000000)) for _ in range(50)) + co.flush()
ox = len(b); b += b'20 0 obj\n<< /Type /XRef /Size 50000000 /W [1 4 2] /Root 1 0 R /Filter /FlateDecode /Length %d >>\nstream\n' % len(rows) + rows + b'\nendstream\nendobj\n'
nx = len(b)
r2 = b''.join(b'\x01' + struct.pack('>I', off[n]) + b'\x00\x00' for n in range(1, 6))
r2 = b'\x00\x00\x00\x00\x00\xff\xff' + r2
b += b'21 0 obj\n<< /Type /XRef /Size 22 /Index [0 6] /W [1 4 2] /Root 1 0 R /Prev %d /Length %d >>\nstream\n' % (ox, len(r2)) + r2 + b'\nendstream\nendobj\n'
b += b'startxref\n%d\n%%%%EOF\n' % nx
open(os.path.join(D, 'h-bomb.pdf'), 'wb').write(b)
# huge first in an older classic section
b, off = base_objs()
o = len(b); b += b'xref\n0 1\n0000000000 65535 f \n50000000 1\n0000000009 00000 n \ntrailer\n<< /Size 6 /Root 1 0 R >>\n'
n = len(b); b += table(off, o)
b += b'startxref\n%d\n%%%%EOF\n' % n
open(os.path.join(D, 'h-bigfirst.pdf'), 'wb').write(b)
# overlapping: 1000 sections whose trailers each open a string running over the rest of the file
b, off = base_objs(); first = len(b); b += table(off)
starts = []
for k in range(1000):
    starts.append(len(b)); b += b'xref\n0 0\ntrailer\n<< /Size 6 /Root 1 0 R /Prev PREV%04d /J (' % k
b += b'x' * 4000000 + b')' * 1000 + b' >>\n'
# fix prevs: section k points to k-1 (section 0 to first)
for k in range(1000):
    tgt = first if k == 0 else starts[k - 1]
    b = b.replace(b'PREV%04d' % k, b'%08d' % tgt, 1)
b += b'startxref\n%d\n%%%%EOF\n' % starts[-1]
open(os.path.join(D, 'h-overlap.pdf'), 'wb').write(b)
