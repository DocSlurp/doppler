# PDF writer: explicit object-stream groups; everything else stored uncompressed
import zlib, struct
class PDFG:
    def __init__(self): self.objs = {}; self.n = 0
    def reserve(self): self.n += 1; return self.n
    def add(self, body): self.n += 1; self.set(self.n, body); return self.n
    def set(self, num, body): self.objs[num] = body if isinstance(body, bytes) else body.encode('latin1')
    def stream(self, data, extra=''):
        if isinstance(data, str): data = data.encode('latin1')
        return self.add(b'<<' + extra.encode() + b'/Length %d>>stream\n' % len(data) + data + b'\nendstream')
    def write(self, path, root, groups=()):
        out = bytearray(b'%PDF-1.7\n%\xe2\xe3\xcf\xd3\n'); entries = {}
        grouped = set(k for g in groups for k in g)
        for k in sorted(self.objs):
            if k in grouped: continue
            entries[k] = (1, len(out), 0); out += b'%d 0 obj\n' % k + self.objs[k] + b'\nendobj\n'
        nxt = self.n + 1
        for chunk in groups:
            hdr = bytearray(); body = bytearray()
            for j, k in enumerate(chunk):
                hdr += b'%d %d ' % (k, len(body)); body += self.objs[k] + b'\n'; entries[k] = (2, nxt, j)
            data = zlib.compress(bytes(hdr) + bytes(body), 9)
            entries[nxt] = (1, len(out), 0)
            out += b'%d 0 obj\n<</Type/ObjStm/N %d/First %d/Filter/FlateDecode/Length %d>>stream\n' % (nxt, len(chunk), len(hdr), len(data)) + data + b'\nendstream\nendobj\n'
            nxt += 1
        xs = nxt; entries[xs] = (1, len(out), 0); rows = bytearray()
        for k in range(0, xs + 1):
            t, a, b = entries.get(k, (0, 0, 0)); rows += struct.pack('>BIH', t, a, b)
        data = zlib.compress(bytes(rows), 9)
        out += b'%d 0 obj\n<</Type/XRef/Size %d/W[1 4 2]/Root %d 0 R/Filter/FlateDecode/Length %d>>stream\n' % (xs, xs + 1, root, len(data)) + data + b'\nendstream\nendobj\n'
        out += b'startxref\n%d\n%%%%EOF\n' % entries[xs][1]
        open(path, 'wb').write(out); return len(out)
