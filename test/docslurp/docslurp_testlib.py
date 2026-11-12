"""Helpers for DocSlurp regression tests.

Tests run with ``python3 -I``, so import this module by path:

    import os, sys
    sys.path.insert(0, os.environ.get("DOCSLURP_TESTLIB_DIR",
                                      os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    import docslurp_testlib as t

Then build a PDF in memory, write it to a temp dir, and run a util against it:

    pdf = t.simple_page_pdf(b"BT /F1 12 Tf 72 700 Td (Hello) Tj ET")
    with t.TempDir() as d:
        path = d.write("hello.pdf", pdf)
        out = t.Utils.from_args().run("pdftotext", path, "-")
        t.check("Hello" in out, "text extracted", out)
    t.finish()
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import zlib


def _b(x):
    return x if isinstance(x, bytes) else x.encode("latin-1")


def stream(dict_body, data, compress=False):
    """Return an indirect-object body for a stream with a correct /Length."""
    data = _b(data)
    dict_body = _b(dict_body)
    if compress:
        data = zlib.compress(data)
        dict_body += b" /Filter /FlateDecode"
    return b"<< " + dict_body + b" /Length %d >>\nstream\n" % len(data) + data + b"\nendstream"


def build_pdf(objects, root=1, trailer_extra=""):
    """Build a complete PDF from object bodies numbered 1..N, with a valid xref table."""
    out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for num, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % num + _b(body) + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root %d 0 R " % (len(objects) + 1, root) + _b(trailer_extra) + b" >>\n"
    out += b"startxref\n%d\n%%%%EOF\n" % xref
    return bytes(out)


def simple_page_pdf(content, resources="<< /Font << /F1 4 0 R >> >>", extra_objects=(), page_extra="", catalog_extra="", mediabox=(0, 0, 612, 792)):
    """One-page PDF. Objects: 1 Catalog, 2 Pages, 3 Page, 4 Helvetica, 5 contents, 6.. extra_objects."""
    box = " ".join(str(v) for v in mediabox)
    objects = [
        "<< /Type /Catalog /Pages 2 0 R %s >>" % catalog_extra,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [%s] /Resources %s /Contents 5 0 R %s >>" % (box, resources, page_extra),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        stream("", content),
    ] + list(extra_objects)
    return build_pdf(objects)


# Standard JPEG luminance DC Huffman table (ITU T.81 K.3): code lengths and symbols.
_DC_BITS = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0]
_DC_VALS = list(range(12))


def _huffman_codes(bits, vals):
    codes, code, k = {}, 0, 0
    for length in range(1, 17):
        for _ in range(bits[length - 1]):
            codes[vals[k]] = (code, length)
            code += 1
            k += 1
        code <<= 1
    return codes


def jpeg(width, height, comps=1, level=None):
    """A baseline JPEG (SOF0, no subsampling) built from DC terms only: every 8x8 block is one
    flat level, ``level(bx, by, c) -> 0..255`` (default: a gradient on component 0, 128 on the
    others), so any decoder gives exact blocks. Stdlib only, for fixtures that need a real
    DCT stream (copy jobs, scans). ``comps`` is 1 (grey) or 3 (YCbCr, JFIF)."""
    if level is None:
        level = lambda bx, by, c: (bx * 37 + by * 23) % 200 + 28 if c == 0 else 128  # noqa: E731
    dc = _huffman_codes(_DC_BITS, _DC_VALS)
    out = bytearray(b"\xff\xd8")
    # JFIF APP0 (density 1:1)
    out += b"\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00"
    # one quantisation table, all 16 (DC term = (level - 128) * 8 / 16)
    out += b"\xff\xdb\x00\x43\x00" + bytes([16] * 64)
    out += b"\xff\xc0" + (8 + 3 * comps).to_bytes(2, "big") + b"\x08" + height.to_bytes(2, "big") + width.to_bytes(2, "big") + bytes([comps])
    for c in range(comps):
        out += bytes([c + 1, 0x11, 0])
    # DC table 0 (standard), AC table 0: one symbol, end of block, code "0"
    out += b"\xff\xc4" + (3 + 16 + 12).to_bytes(2, "big") + b"\x00" + bytes(_DC_BITS) + bytes(_DC_VALS)
    out += b"\xff\xc4" + (3 + 16 + 1).to_bytes(2, "big") + b"\x10" + bytes([1] + [0] * 15) + b"\x00"
    out += b"\xff\xda" + (6 + 2 * comps).to_bytes(2, "big") + bytes([comps])
    for c in range(comps):
        out += bytes([c + 1, 0x00])
    out += b"\x00\x3f\x00"
    bits = []

    def put(value, length):
        for i in range(length - 1, -1, -1):
            bits.append((value >> i) & 1)

    pred = [0] * comps
    for by in range((height + 7) // 8):
        for bx in range((width + 7) // 8):
            for c in range(comps):
                coef = (max(0, min(255, int(level(bx, by, c)))) - 128) // 2
                diff = coef - pred[c]
                pred[c] = coef
                size = abs(diff).bit_length()
                code, length = dc[size]
                put(code, length)
                if size:
                    put(diff if diff > 0 else diff + (1 << size) - 1, size)
                bits.append(0)  # end of block
    while len(bits) % 8:
        bits.append(1)
    for i in range(0, len(bits), 8):
        byte = int("".join(map(str, bits[i : i + 8])), 2)
        out.append(byte)
        if byte == 0xFF:
            out.append(0)
    out += b"\xff\xd9"
    return bytes(out)


def ccitt_g4_white(width, height):
    """A CCITT Group 4 (K -1) stream of an all-white image: each row repeats the imaginary
    white reference line (one vertical-0 code, bit 1), then EOFB."""
    bits = [1] * height + [0] * 11 + [1] + [0] * 11 + [1]
    while len(bits) % 8:
        bits.append(0)
    return bytes(int("".join(map(str, bits[i : i + 8])), 2) for i in range(0, len(bits), 8))


class TempDir:
    """Scratch directory under $TMPDIR (never hardcoded /tmp), removed on exit."""

    def __enter__(self):
        self.path = tempfile.mkdtemp(prefix="docslurp-test.")
        return self

    def __exit__(self, *exc):
        shutil.rmtree(self.path, ignore_errors=True)

    def write(self, name, data):
        path = os.path.join(self.path, name)
        with open(path, "wb") as f:
            f.write(_b(data))
        return path


class Utils:
    """Runs the poppler utils built in this tree."""

    def __init__(self, bindir):
        self.bindir = bindir

    @classmethod
    def from_args(cls, argv=None):
        parser = argparse.ArgumentParser()
        parser.add_argument("--bindir", required=True)
        args, _ = parser.parse_known_args(argv)
        return cls(args.bindir)

    def run(self, util, *args, timeout=60, check=True, binary=False):
        cmd = [os.path.join(self.bindir, util)] + [str(a) for a in args]
        proc = subprocess.run(cmd, capture_output=True, timeout=timeout)
        if check and proc.returncode != 0:
            raise AssertionError("%s exited %d\nstderr:\n%s" % (" ".join(cmd), proc.returncode, proc.stderr.decode(errors="replace")))
        return proc.stdout if binary else proc.stdout.decode("utf-8", errors="replace")


def schema_errors(instance, schema, root=None, path="$"):
    """Validate <instance> against the subset of JSON Schema (draft 2020-12) that the DocSlurp
    schemas use: $ref (local), oneOf, type, enum, const, required, properties,
    additionalProperties, propertyNames, items, minItems, maxItems, uniqueItems, minimum,
    maximum, pattern, maxLength. Returns a list of error strings (empty when valid)."""
    root = schema if root is None else root
    if "$ref" in schema:
        node = root
        for part in schema["$ref"].lstrip("#/").split("/"):
            node = node[part]
        return schema_errors(instance, node, root, path)
    errors = []
    if "oneOf" in schema:
        matches = [opt for opt in schema["oneOf"] if not schema_errors(instance, opt, root, path)]
        if len(matches) != 1:
            best = min((schema_errors(instance, opt, root, path) for opt in schema["oneOf"]), key=len)
            return ["%s: matches %d of oneOf; closest: %s" % (path, len(matches), best[:3])]
    kinds = {"object": dict, "array": list, "string": str, "boolean": bool}
    t = schema.get("type")
    if t == "integer":
        if isinstance(instance, bool) or not isinstance(instance, int):
            return ["%s: not an integer: %r" % (path, instance)]
    elif t == "number":
        if isinstance(instance, bool) or not isinstance(instance, (int, float)):
            return ["%s: not a number: %r" % (path, instance)]
    elif t is not None and not isinstance(instance, kinds[t]):
        return ["%s: not %s: %r" % (path, t, instance)]
    if "const" in schema and instance != schema["const"]:
        errors.append("%s: %r != const %r" % (path, instance, schema["const"]))
    if "enum" in schema and instance not in schema["enum"]:
        errors.append("%s: %r not in enum" % (path, instance))
    if isinstance(instance, (int, float)) and not isinstance(instance, bool):
        if "minimum" in schema and instance < schema["minimum"]:
            errors.append("%s: %r < %r" % (path, instance, schema["minimum"]))
        if "maximum" in schema and instance > schema["maximum"]:
            errors.append("%s: %r > %r" % (path, instance, schema["maximum"]))
    if isinstance(instance, str):
        if "pattern" in schema and not re.search(schema["pattern"], instance):
            errors.append("%s: %r does not match %s" % (path, instance, schema["pattern"]))
        if "maxLength" in schema and len(instance) > schema["maxLength"]:
            errors.append("%s: longer than %d" % (path, schema["maxLength"]))
    if isinstance(instance, list):
        if "minItems" in schema and len(instance) < schema["minItems"]:
            errors.append("%s: fewer than %d items" % (path, schema["minItems"]))
        if "maxItems" in schema and len(instance) > schema["maxItems"]:
            errors.append("%s: more than %d items" % (path, schema["maxItems"]))
        if schema.get("uniqueItems") and len(set(json.dumps(i, sort_keys=True) for i in instance)) != len(instance):
            errors.append("%s: items not unique" % path)
        if "items" in schema:
            for i, item in enumerate(instance):
                errors += schema_errors(item, schema["items"], root, "%s[%d]" % (path, i))
    if isinstance(instance, dict):
        for key in schema.get("required", []):
            if key not in instance:
                errors.append("%s: missing %s" % (path, key))
        props = schema.get("properties", {})
        for key, value in instance.items():
            if "propertyNames" in schema:
                errors += schema_errors(key, schema["propertyNames"], root, "%s.<name %s>" % (path, key))
            if key in props:
                errors += schema_errors(value, props[key], root, "%s.%s" % (path, key))
            elif schema.get("additionalProperties") is False:
                errors.append("%s: unexpected %s" % (path, key))
            elif isinstance(schema.get("additionalProperties"), dict):
                errors += schema_errors(value, schema["additionalProperties"], root, "%s.%s" % (path, key))
    return errors


_failures = []


def check(cond, what, detail=""):
    """Record one assertion; print PASS/FAIL with detail so ctest logs are useful."""
    if cond:
        print("PASS: %s" % what)
    else:
        print("FAIL: %s\n%s" % (what, detail))
        _failures.append(what)


def finish():
    """Exit non-zero if any check failed."""
    if _failures:
        print("%d check(s) failed" % len(_failures))
        sys.exit(1)
    print("all checks passed")
    sys.exit(0)
