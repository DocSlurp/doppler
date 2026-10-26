"""Regenerate src/entities.rs from Python's html.entities.html5 (the table html.unescape uses)."""
import html.entities
import pathlib


def rust(s):
    return '"' + ''.join(c if c.isascii() and c.isprintable() and c not in '"\\' else '\\u{%x}' % ord(c) for c in s) + '"'


items = sorted(html.entities.html5.items())
lines = ["// Generated from Python's html.entities.html5 by scripts/gen_entities.py. Do not edit.",
         "// Sorted by name for binary search.",
         "pub static HTML5: &[(&str, &str)] = &["]
lines += ["    (%s, %s)," % (rust(k), rust(v)) for k, v in items]
lines.append("];")
pathlib.Path(__file__).resolve().parent.parent.joinpath('src', 'entities.rs').write_text('\n'.join(lines) + '\n')

import html as _html
inv = sorted(_html._invalid_charrefs.items())
cps = sorted(_html._invalid_codepoints)
extra = ["pub static INVALID_CHARREFS: &[(u32, &str)] = &["]
extra += ["    (0x%x, %s)," % (k, rust(v)) for k, v in inv]
extra += ["];", "pub static INVALID_CODEPOINTS: &[u32] = &["]
extra += ["    " + ", ".join("0x%x" % c for c in cps[i:i + 12]) + "," for i in range(0, len(cps), 12)]
extra.append("];")
path = pathlib.Path(__file__).resolve().parent.parent.joinpath('src', 'entities.rs')
path.write_text(path.read_text() + '\n'.join(extra) + '\n')
