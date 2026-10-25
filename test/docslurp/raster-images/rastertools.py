"""Helpers shared by the raster-images regression tests."""

import os
import struct
import zlib

import docslurp_testlib as t

FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures")


def fixture(name):
    with open(os.path.join(FIXTURES, name), "rb") as f:
        return f.read()


def png_chunks(data):
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos = 8
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        yield data[pos + 4:pos + 8], data[pos + 8:pos + 8 + length]
        pos += 12 + length


def png_header(data):
    """Return (width, height, bit_depth, color_type) from the IHDR chunk."""
    for kind, body in png_chunks(data):
        if kind == b"IHDR":
            w, h, depth, ctype, _comp, _filt, _inter = struct.unpack(">IIBBBBB", body)
            return w, h, depth, ctype
    raise AssertionError("no IHDR")


def png_pixels(data):
    """Decode a non-interlaced PNG to (header, list of unfiltered scanline bytes)."""
    w, h, depth, ctype = png_header(data)
    channels = {0: 1, 2: 3, 4: 2, 6: 4}[ctype]
    bpp = max(1, channels * depth // 8)
    stride = (w * channels * depth + 7) // 8
    raw = zlib.decompress(b"".join(body for kind, body in png_chunks(data) if kind == b"IDAT"))
    rows = []
    prev = bytearray(stride)
    pos = 0
    for _ in range(h):
        ft = raw[pos]
        cur = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += 1 + stride
        for i in range(stride):
            a = cur[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if ft == 1:
                cur[i] = (cur[i] + a) & 255
            elif ft == 2:
                cur[i] = (cur[i] + b) & 255
            elif ft == 3:
                cur[i] = (cur[i] + ((a + b) >> 1)) & 255
            elif ft == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                cur[i] = (cur[i] + pred) & 255
        rows.append(bytes(cur))
        prev = cur
    return (w, h, depth, ctype), rows


def pnm_read(data):
    """Parse P4/P5/P6. Returns (magic, width, height, payload)."""
    magic = data[:2]
    pos = 2
    fields = []
    maxfields = 2 if magic == b"P4" else 3
    while len(fields) < maxfields:
        while data[pos:pos + 1].isspace():
            pos += 1
        if data[pos:pos + 1] == b"#":
            while data[pos:pos + 1] != b"\n":
                pos += 1
            continue
        start = pos
        while not data[pos:pos + 1].isspace():
            pos += 1
        fields.append(int(data[start:pos]))
    return magic, fields[0], fields[1], data[pos + 1:]


def jpx_image_pdf(jpx, width, height, colorspace="", extra=""):
    """One-page PDF drawing a JPXDecode image over most of the page."""
    img = t.stream("/Type /XObject /Subtype /Image /Width %d /Height %d /Filter /JPXDecode %s %s" % (width, height, colorspace, extra), jpx)
    content = b"q 200 0 0 150 50 50 cm /Im0 Do Q"
    return t.simple_page_pdf(content, resources="<< /XObject << /Im0 6 0 R >> >>", extra_objects=[img], mediabox=(0, 0, 300, 250))


def box(kind, payload):
    return struct.pack(">I", 8 + len(payload)) + kind + payload


def palette_jp2(gray_codestream, width, height):
    """Wrap a 1-component codestream in a JP2 whose pclr/cmap boxes expand it to RGB."""
    sig = box(b"jP  ", b"\x0d\x0a\x87\x0a")
    ftyp = box(b"ftyp", b"jp2 " + struct.pack(">I", 0) + b"jp2 ")
    ihdr = box(b"ihdr", struct.pack(">IIHBBBB", height, width, 1, 7, 7, 0, 0))
    colr = box(b"colr", bytes([1, 0, 0]) + struct.pack(">I", 16))
    entries = 256
    pal = struct.pack(">HB", entries, 3) + bytes([7, 7, 7])
    for i in range(entries):
        pal += bytes([i, 255 - i, (i * 7) & 255])
    pclr = box(b"pclr", pal)
    cmap = box(b"cmap", b"".join(struct.pack(">HBB", 0, 1, c) for c in range(3)))
    jp2h = box(b"jp2h", ihdr + colr + pclr + cmap)
    jp2c = box(b"jp2c", gray_codestream)
    return sig + ftyp + jp2h + jp2c
