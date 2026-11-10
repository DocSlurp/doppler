//! SVG of a vector figure (`pdfregions` `svg` records, cairo output): check it against an
//! allowlist and rewrite it compactly.
//!
//! The input is parsed as XML (no DTD, no entities beyond the predefined ones) and every element
//! and attribute is checked against a fixed allowlist; anything else rejects the SVG, and the
//! caller renders the figure as a raster instead. The output is written from the parsed tree, so
//! only allowlisted names and escaped values reach it.
//!
//! Compaction: numbers are rounded to `precision` decimals of a point in the figure's final
//! space (more where a transform enlarges, fewer where it shrinks), path data is rewritten with
//! the shorter of absolute and relative commands and no redundant letters or separators,
//! colours become hex, default-valued properties go when nothing above them sets another value,
//! identical definitions (cairo's glyph outlines) are merged, ids get short names, unreferenced
//! definitions and empty groups are removed, and so is the whitespace between tags. Embedded
//! image placement keeps at least 2 decimals and transform scale factors 6 significant digits.

use std::collections::HashMap;
use std::fmt::Write as _;

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// Elements accepted (and written).
const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "path",
    "use",
    "clipPath",
    "mask",
    "rect",
    "image",
    "symbol",
    "linearGradient",
    "radialGradient",
    "stop",
    "pattern",
    "filter",
    "feColorMatrix",
];

/// Attributes accepted (in no namespace; `xlink:href` is the only namespaced one).
const ATTRIBUTES: &[&str] = &[
    "id",
    "d",
    "transform",
    "x",
    "y",
    "width",
    "height",
    "viewBox",
    "preserveAspectRatio",
    "href",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
    "clip-path",
    "clip-rule",
    "mask",
    "filter",
    "clipPathUnits",
    "maskUnits",
    "maskContentUnits",
    "patternUnits",
    "patternContentUnits",
    "patternTransform",
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "fx",
    "fy",
    "fr",
    "offset",
    "stop-color",
    "stop-opacity",
    "color-interpolation-filters",
    "filterUnits",
    "primitiveUnits",
    "type",
    "values",
    "in",
    "result",
    "version",
];

/// Elements that are drawn only where something references them.
const REFERENCED_ONLY: &[&str] =
    &["clipPath", "mask", "symbol", "linearGradient", "radialGradient", "pattern", "filter"];

/// Element and nesting limits: past them the SVG is rejected (rendered as a raster instead).
const MAX_NODES: u32 = 2_000_000;
const MAX_DEPTH: usize = 256;

/// Why an SVG was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected(pub String);

impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn reject<T>(why: impl Into<String>) -> Result<T, Rejected> {
    Err(Rejected(why.into()))
}

#[derive(Debug, Clone)]
struct Node {
    name: &'static str,
    attrs: Vec<(&'static str, String)>,
    children: Vec<usize>,
    parent: Option<usize>,
    removed: bool,
}

impl Node {
    fn get(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str())
    }
    fn set(&mut self, k: &'static str, v: String) {
        match self.attrs.iter_mut().find(|(n, _)| *n == k) {
            Some(slot) => slot.1 = v,
            None => self.attrs.push((k, v)),
        }
    }
    fn unset(&mut self, k: &str) {
        self.attrs.retain(|(n, _)| *n != k);
    }
}

/// Check `svg` against the allowlist and write it compactly with `precision` decimals (1 or 2)
/// of a point.
pub fn compact(svg: &str, precision: u8) -> Result<String, Rejected> {
    let opt = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: MAX_NODES, ..Default::default() };
    let doc = match roxmltree::Document::parse_with_options(svg, opt) {
        Ok(d) => d,
        Err(e) => return reject(format!("xml: {e}")),
    };
    let mut nodes = build(&doc)?;
    let referencers = check_references(&nodes)?;
    let order = topo_order(&nodes, &referencers)?;
    let scale = scales(&nodes, &referencers, &order)?;
    let above = set_above(&nodes, &referencers, &order);
    let dash_bit = 1 << DEFAULTS.iter().position(|(k, _)| *k == "stroke-dasharray").unwrap_or(0);
    let digits: Vec<u8> = scale
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            // a dash pattern runs along the whole path: its phase drifts with the rounding of
            // every vertex, so dashed strokes keep one more decimal
            let dashed =
                nodes[i].get("stroke-dasharray").is_some_and(|v| v.trim() != "none") || above[i] & dash_bit != 0;
            let d = (f64::from(precision) + s.log10() - 1e-9).ceil() + if dashed { 1.0 } else { 0.0 };
            d.clamp(0.0, 8.0) as u8
        })
        .collect();
    rewrite_numbers(&mut nodes, &digits)?;
    drop_defaults(&mut nodes, &above);
    merge_duplicates(&mut nodes);
    rename_ids(&mut nodes);
    hoist_styles(&mut nodes);
    prune(&mut nodes);
    let mut out = String::with_capacity(svg.len() / 2);
    write_node(&nodes, 0, &mut out);
    Ok(out)
}

/// The allowlisted tree, root first.
fn build(doc: &roxmltree::Document) -> Result<Vec<Node>, Rejected> {
    let root = doc.root_element();
    let mut nodes: Vec<Node> = Vec::new();
    // (xml node, parent index, depth)
    let mut stack = vec![(root, None::<usize>, 0usize)];
    while let Some((x, parent, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return reject("nesting too deep");
        }
        if x.tag_name().namespace() != Some(SVG_NS) {
            return reject("element outside the SVG namespace");
        }
        let tag = x.tag_name().name();
        let Some(&name) = ELEMENTS.iter().find(|e| **e == tag) else {
            return reject(format!("element {tag}"));
        };
        if (name == "svg") != parent.is_none() {
            return reject("svg element not at the root");
        }
        let mut attrs: Vec<(&'static str, String)> = Vec::with_capacity(x.attributes().len());
        for a in x.attributes() {
            let key = match a.namespace() {
                None => a.name(),
                Some(XLINK_NS) if a.name() == "href" => "href",
                Some(_) => return reject(format!("attribute {}", a.name())),
            };
            let Some(&key) = ATTRIBUTES.iter().find(|k| **k == key) else {
                return reject(format!("attribute {key}"));
            };
            if key == "version" {
                continue;
            }
            if attrs.iter().any(|(k, _)| *k == key) {
                return reject("repeated attribute");
            }
            check_value(name, key, a.value())?;
            attrs.push((key, a.value().to_string()));
        }
        let me = nodes.len();
        nodes.push(Node { name, attrs, children: Vec::new(), parent, removed: false });
        if let Some(p) = parent {
            nodes[p].children.push(me);
        }
        // children in document order: push reversed
        let mut kids = Vec::new();
        for c in x.children() {
            if c.is_element() {
                kids.push(c);
            } else if c.is_text() {
                if !c.text().unwrap_or("").chars().all(char::is_whitespace) {
                    return reject("text content");
                }
            } else if c.is_comment() {
                // dropped
            } else {
                return reject("processing instruction");
            }
        }
        for c in kids.into_iter().rev() {
            stack.push((c, Some(me), depth + 1));
        }
    }
    // the stack visits a node's children in order, but interleaved with deeper levels: the
    // parent links and children lists are what matter, and they are in document order
    let root = &nodes[0];
    let vb = root.get("viewBox").map(numbers).transpose()?.unwrap_or_default();
    if vb.len() != 4 || vb[2] <= 0.0 || vb[3] <= 0.0 {
        return reject("viewBox");
    }
    Ok(nodes)
}

/// Values that could reach outside the document, or that are not what their attribute takes.
fn check_value(element: &str, key: &str, v: &str) -> Result<(), Rejected> {
    if v.len() > 64 << 20 {
        return reject("value too long");
    }
    match key {
        "href" => {
            if element == "image" {
                return check_png_data_uri(v);
            }
            if element != "use" {
                return reject("href on this element");
            }
            local_ref(v.strip_prefix('#').unwrap_or("\u{0}"))?;
        }
        "id" => local_ref(v)?,
        _ => {
            // every url(...) is a local reference, nothing else
            let mut rest = v;
            while let Some(at) = rest.find("url(") {
                let after = &rest[at + 4..];
                let Some(end) = after.find(')') else { return reject("url") };
                let inner = after[..end].trim();
                let Some(id) = inner.strip_prefix('#') else { return reject("non-local url") };
                local_ref(id)?;
                rest = &after[end + 1..];
            }
            let lower = v.to_ascii_lowercase();
            if lower.contains("javascript:") || lower.contains("data:") || lower.contains("expression(") {
                return reject("script in a value");
            }
        }
    }
    Ok(())
}

fn local_ref(id: &str) -> Result<(), Rejected> {
    let ok = !id.is_empty()
        && id.len() <= 256
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':');
    if ok { Ok(()) } else { reject("reference is not a local id") }
}

/// `data:image/png;base64,...` with valid base64 that decodes to a PNG signature.
fn check_png_data_uri(v: &str) -> Result<(), Rejected> {
    let Some(b64) = v.strip_prefix("data:image/png;base64,") else {
        return reject("image href is not a PNG data URI");
    };
    let body = b64.trim_end_matches('=');
    if b64.len() - body.len() > 2 || !body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/') {
        return reject("image data is not base64");
    }
    if body.len() < 12 {
        return reject("image data too short");
    }
    let val = |b: u8| -> u32 {
        match b {
            b'A'..=b'Z' => u32::from(b - b'A'),
            b'a'..=b'z' => u32::from(b - b'a') + 26,
            b'0'..=b'9' => u32::from(b - b'0') + 52,
            b'+' => 62,
            _ => 63,
        }
    };
    let mut head = Vec::with_capacity(9);
    for quad in body.as_bytes()[..12].chunks(4) {
        let n = quad.iter().fold(0u32, |acc, &b| (acc << 6) | val(b));
        head.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    if !head.starts_with(b"\x89PNG\r\n\x1a\n") {
        return reject("image data is not a PNG");
    }
    Ok(())
}

/// The ids that each element references, and for each element the elements that reference it.
fn check_references(nodes: &[Node]) -> Result<Vec<Vec<usize>>, Rejected> {
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        if let Some(id) = n.get("id")
            && by_id.insert(id, i).is_some()
        {
            return reject("duplicate id");
        }
    }
    let mut referencers = vec![Vec::new(); nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        for (_, v) in &n.attrs {
            for id in refs_in(v, n.name) {
                if let Some(&t) = by_id.get(id) {
                    referencers[t].push(i);
                }
            }
        }
    }
    Ok(referencers)
}

/// The local ids a value references: `url(#id)` anywhere, or `#id` as a whole (`href`).
fn refs_in<'a>(v: &'a str, element: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    if let Some(id) = v.strip_prefix('#')
        && element == "use"
    {
        out.push(id);
        return out;
    }
    let mut rest = v;
    while let Some(at) = rest.find("url(#") {
        let after = &rest[at + 5..];
        let end = after.find(')').unwrap_or(after.len());
        out.push(after[..end].trim());
        rest = &after[end..];
    }
    out
}

/// The 2x2 linear part of a transform list, or `None` when it does not parse.
fn linear(t: &str) -> Option<[f64; 4]> {
    let mut m = [1.0, 0.0, 0.0, 1.0];
    let mut rest = t.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let args = numbers(&rest[open + 1..close]).ok()?;
        let f = match (name, args.len()) {
            ("matrix", 6) => [args[0], args[1], args[2], args[3]],
            ("translate", 1 | 2) => [1.0, 0.0, 0.0, 1.0],
            ("scale", 1) => [args[0], 0.0, 0.0, args[0]],
            ("scale", 2) => [args[0], 0.0, 0.0, args[1]],
            ("rotate", 1 | 3) => {
                let (s, c) = args[0].to_radians().sin_cos();
                [c, s, -s, c]
            }
            ("skewX", 1) => [1.0, 0.0, args[0].to_radians().tan(), 1.0],
            ("skewY", 1) => [1.0, args[0].to_radians().tan(), 0.0, 1.0],
            _ => return None,
        };
        m = [
            m[0] * f[0] + m[2] * f[1],
            m[1] * f[0] + m[3] * f[1],
            m[0] * f[2] + m[2] * f[3],
            m[1] * f[2] + m[3] * f[3],
        ];
        rest = rest[close + 1..].trim_start_matches([' ', ',', '\t', '\n', '\r']);
    }
    m.iter().all(|v| v.is_finite()).then_some(m)
}

/// The elements in an order where each comes after its parent and after every element that
/// references it; `Err` on a cycle of references.
fn topo_order(nodes: &[Node], referencers: &[Vec<usize>]) -> Result<Vec<usize>, Rejected> {
    // 0: not visited, 1: on the stack, 2: done
    let mut state = vec![0u8; nodes.len()];
    let mut order = Vec::with_capacity(nodes.len());
    for start in 0..nodes.len() {
        if state[start] == 2 {
            continue;
        }
        let mut stack = vec![start];
        while let Some(&n) = stack.last() {
            if state[n] == 2 {
                stack.pop();
                continue;
            }
            state[n] = 1;
            let mut pending = None;
            for d in nodes[n].parent.into_iter().chain(referencers[n].iter().copied()) {
                match state[d] {
                    0 => {
                        pending = Some(d);
                        break;
                    }
                    1 => return reject("reference cycle"),
                    _ => {}
                }
            }
            match pending {
                Some(d) => stack.push(d),
                None => {
                    state[n] = 2;
                    order.push(n);
                    stack.pop();
                }
            }
        }
    }
    Ok(order)
}

/// How much a unit of each element's own coordinates (after its transform) is enlarged in the
/// figure's space: the largest over the ways it is drawn (in place, and through every element
/// that references it or an ancestor).
fn scales(nodes: &[Node], referencers: &[Vec<usize>], order: &[usize]) -> Result<Vec<f64>, Rejected> {
    let local: Vec<f64> = nodes
        .iter()
        .map(|n| {
            let t = n.get("transform").or(n.get("patternTransform")).or(n.get("gradientTransform"));
            match t {
                None => Ok(1.0),
                Some(t) => match linear(t) {
                    // the longer image of the unit vectors
                    Some(m) => Ok((m[0].hypot(m[1])).max(m[2].hypot(m[3])).max(1e-6)),
                    None => reject("transform"),
                },
            }
        })
        .collect::<Result<_, _>>()?;
    let mut eff = vec![1.0f64; nodes.len()];
    for &n in order {
        let base = nodes[n].parent.map(|p| eff[p]).unwrap_or(1.0);
        let base = referencers[n].iter().fold(base, |b, &r| b.max(eff[r]));
        eff[n] = (local[n] * base).clamp(1e-6, 1e12);
    }
    Ok(eff)
}

/// Inherited properties of [`DEFAULTS`] (a bit each) that something above each element sets:
/// its ancestors, and the `use` elements that draw it (with their ancestors).
fn set_above(nodes: &[Node], referencers: &[Vec<usize>], order: &[usize]) -> Vec<u32> {
    let own: Vec<u32> = nodes
        .iter()
        .map(|n| {
            DEFAULTS.iter().enumerate().filter(|(_, (k, _))| n.get(k).is_some()).fold(0u32, |m, (b, _)| m | (1 << b))
        })
        .collect();
    let mut above = vec![0u32; nodes.len()];
    for &n in order {
        let mut m = nodes[n].parent.map(|p| above[p] | own[p]).unwrap_or(0);
        for &r in &referencers[n] {
            if nodes[r].name == "use" {
                m |= above[r] | own[r];
            }
        }
        above[n] = m;
    }
    above
}

/// Parsed number list (whitespace and commas); `Err` on anything else.
fn numbers(s: &str) -> Result<Vec<f64>, Rejected> {
    let mut out = Vec::new();
    let mut p = PathLexer { s: s.as_bytes(), i: 0 };
    loop {
        p.skip_sep();
        if p.i >= p.s.len() {
            return Ok(out);
        }
        match p.number() {
            Some(v) => out.push(v),
            None => return reject("number list"),
        }
    }
}

/// Rewrites every numeric attribute; `digits[i]` decimals for element i's own coordinates,
/// translations of a transform in the parent's.
fn rewrite_numbers(nodes: &mut [Node], digits: &[u8]) -> Result<(), Rejected> {
    for i in 0..nodes.len() {
        let d = digits[i];
        let image = nodes[i].name == "image";
        let parent_d = nodes[i].parent.map(|p| digits[p]).unwrap_or(d);
        let attrs = std::mem::take(&mut nodes[i].attrs);
        let mut out = Vec::with_capacity(attrs.len());
        for (k, v) in attrs {
            let nv = match k {
                "d" => path_data(&v, d)?,
                "transform" | "patternTransform" | "gradientTransform" => transform(&v, parent_d.max(2))?,
                "x" | "y" | "width" | "height" if nodes[i].name == "svg" => v,
                "x" | "y" | "width" | "height" | "x1" | "y1" | "x2" | "y2" | "cx" | "cy" | "r" | "fx" | "fy" | "fr" => {
                    if v.trim_end().ends_with('%') {
                        v
                    } else {
                        let units = nodes[i].get("gradientUnits").or(nodes[i].get("patternUnits"));
                        let bbox_units = matches!(nodes[i].name, "linearGradient" | "radialGradient" | "pattern")
                            && units != Some("userSpaceOnUse");
                        let dd = if bbox_units {
                            4
                        } else if image {
                            d.max(2)
                        } else {
                            d
                        };
                        number_list(&v, dd, ' ')?
                    }
                }
                "stroke-width" | "stroke-dashoffset" => number_list(&v, d + 1, ' ')?,
                "stroke-dasharray" => {
                    if v.trim() == "none" {
                        v
                    } else {
                        number_list(&v, d + 1, ',')?
                    }
                }
                "stroke-miterlimit" | "opacity" | "fill-opacity" | "stroke-opacity" | "stop-opacity" | "offset" => {
                    if v.trim_end().ends_with('%') { v } else { number_list(&v, 4, ' ')? }
                }
                "values" => number_list(&v, 6, ' ')?,
                "fill" | "stroke" | "stop-color" => color(&v),
                _ => v,
            };
            out.push((k, nv));
        }
        nodes[i].attrs = out;
    }
    Ok(())
}

fn number_list(v: &str, digits: u8, sep: char) -> Result<String, Rejected> {
    let mut s = String::new();
    for (k, n) in numbers(v)?.into_iter().enumerate() {
        if k > 0 {
            s.push(sep);
        }
        fmt_units(&mut s, to_units(n, digits)?, digits);
    }
    Ok(s)
}

/// A transform list with translations at `digits` decimals and the other factors at 6
/// significant digits.
fn transform(v: &str, digits: u8) -> Result<String, Rejected> {
    let mut out = String::new();
    let mut rest = v.trim();
    while !rest.is_empty() {
        let (Some(open), Some(close)) = (rest.find('('), rest.find(')')) else {
            return reject("transform");
        };
        if close < open {
            return reject("transform");
        }
        let name = rest[..open].trim().trim_start_matches(',').trim();
        if !matches!(name, "matrix" | "translate" | "scale" | "rotate" | "skewX" | "skewY") {
            return reject("transform");
        }
        let mut args = numbers(&rest[open + 1..close])?;
        let mut name = name;
        // matrix(1 0 0 1 e f) is translate(e f)
        if name == "matrix" && args.len() == 6 && args[..4] == [1.0, 0.0, 0.0, 1.0] {
            name = "translate";
            args.drain(..4);
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(name);
        out.push('(');
        for (k, a) in args.iter().enumerate() {
            if k > 0 {
                out.push(' ');
            }
            let translation = match name {
                "matrix" => k >= 4,
                "translate" => true,
                "rotate" => k >= 1,
                _ => false,
            };
            if translation {
                fmt_units(&mut out, to_units(*a, digits)?, digits);
            } else {
                fmt_significant(&mut out, *a, 6);
            }
        }
        out.push(')');
        rest = rest[close + 1..].trim_start_matches([' ', ',', '\t', '\n', '\r']);
    }
    Ok(out)
}

fn fmt_significant(out: &mut String, v: f64, sig: i32) {
    if v == 0.0 {
        out.push('0');
        return;
    }
    let mag = v.abs().log10().floor() as i32;
    let decimals = (sig - 1 - mag).clamp(0, 12) as u8;
    match to_units(v, decimals) {
        Ok(u) => fmt_units(out, u, decimals),
        Err(_) => {
            let _ = write!(out, "{v}");
        }
    }
}

/// `rgb(r%, g%, b%)` and `rgb(r, g, b)` as the shortest hex form; anything else unchanged.
fn color(v: &str) -> String {
    let t = v.trim();
    let Some(inner) = t.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) else {
        return v.to_string();
    };
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return v.to_string();
    }
    let mut c = [0u8; 3];
    for (k, p) in parts.iter().enumerate() {
        let val = if let Some(pct) = p.strip_suffix('%') {
            pct.parse::<f64>().ok().map(|x| x / 100.0 * 255.0)
        } else {
            p.parse::<f64>().ok()
        };
        match val {
            Some(x) if x.is_finite() => c[k] = x.round().clamp(0.0, 255.0) as u8,
            _ => return v.to_string(),
        }
    }
    if c.iter().all(|b| b >> 4 == b & 15) {
        format!("#{:x}{:x}{:x}", c[0] & 15, c[1] & 15, c[2] & 15)
    } else {
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    }
}

/// Inherited presentation properties and their initial values: dropped where nothing above
/// sets another value. Only for elements drawn in place (a definition inherits from where it
/// is used).
const DEFAULTS: &[(&str, &str)] = &[
    ("fill-opacity", "1"),
    ("stroke-opacity", "1"),
    ("fill-rule", "nonzero"),
    ("clip-rule", "nonzero"),
    ("stroke-linecap", "butt"),
    ("stroke-linejoin", "miter"),
    ("stroke-miterlimit", "4"),
    ("stroke-dashoffset", "0"),
    ("stroke-dasharray", "none"),
];

fn drop_defaults(nodes: &mut [Node], above: &[u32]) {
    for (i, n) in nodes.iter_mut().enumerate() {
        n.attrs.retain(|(k, v)| {
            if *k == "opacity" {
                return v != "1";
            }
            match DEFAULTS.iter().position(|(p, _)| p == k) {
                Some(b) => !(v == DEFAULTS[b].1 && above[i] & (1 << b) == 0),
                None => true,
            }
        });
    }
}

/// Inherited presentation properties: the same value on a run of sibling elements moves to a
/// group around them.
const INHERITED: &[&str] = &[
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
];

/// Wraps runs of siblings that set the same inherited properties in a group that sets them
/// once. Not for elements with an id (a `use` of one would no longer see the group), nor in
/// `defs` or `clipPath` (which take no groups).
fn hoist_styles(nodes: &mut Vec<Node>) {
    let style = |n: &Node| -> Option<Vec<(&'static str, String)>> {
        if n.get("id").is_some() || !matches!(n.name, "path" | "use" | "rect" | "image" | "g") {
            return None;
        }
        let s: Vec<(&'static str, String)> = n.attrs.iter().filter(|(k, _)| INHERITED.contains(k)).cloned().collect();
        (!s.is_empty()).then_some(s)
    };
    for p in 0..nodes.len() {
        if matches!(nodes[p].name, "defs" | "clipPath") || nodes[p].children.len() < 2 {
            continue;
        }
        let kids = nodes[p].children.clone();
        let mut out: Vec<usize> = Vec::with_capacity(kids.len());
        let mut k = 0;
        while k < kids.len() {
            let Some(st) = style(&nodes[kids[k]]) else {
                out.push(kids[k]);
                k += 1;
                continue;
            };
            let mut end = k + 1;
            while end < kids.len() && style(&nodes[kids[end]]).as_ref() == Some(&st) {
                end += 1;
            }
            let run = &kids[k..end];
            let saved: usize = st.iter().map(|(a, v)| a.len() + v.len() + 4).sum::<usize>() * (run.len() - 1);
            if run.len() < 2 || saved <= 7 {
                out.extend_from_slice(run);
                k = end;
                continue;
            }
            let g = nodes.len();
            nodes.push(Node { name: "g", attrs: st.clone(), children: run.to_vec(), parent: Some(p), removed: false });
            for &c in run {
                nodes[c].parent = Some(g);
                nodes[c].attrs.retain(|(a, _)| !INHERITED.contains(a));
            }
            out.push(g);
            k = end;
        }
        nodes[p].children = out;
    }
}

/// Whether `i` is a definition: a child of `defs`, or of groups without an id inside it.
fn in_defs(nodes: &[Node], i: usize) -> bool {
    let mut p = nodes[i].parent;
    while let Some(k) = p {
        if nodes[k].name == "defs" {
            return true;
        }
        if nodes[k].name != "g" || nodes[k].get("id").is_some() {
            return false;
        }
        p = nodes[k].parent;
    }
    false
}

/// Definitions with the same content (cairo writes a glyph outline per font and size) are
/// merged into the first: references to the others point to it.
fn merge_duplicates(nodes: &mut [Node]) {
    let mut first: HashMap<String, String> = HashMap::new();
    let mut map: HashMap<String, String> = HashMap::new();
    for i in 0..nodes.len() {
        let Some(id) = nodes[i].get("id").map(str::to_string) else { continue };
        if !in_defs(nodes, i) {
            continue;
        }
        let mut key = String::new();
        let saved = nodes[i].attrs.clone();
        nodes[i].unset("id");
        write_node(nodes, i, &mut key);
        nodes[i].attrs = saved;
        // content that references other ids could change meaning only through those ids,
        // which are the same strings here
        match first.get(&key) {
            Some(keep) => {
                map.insert(id, keep.clone());
                nodes[i].removed = true;
            }
            None => {
                first.insert(key, id);
            }
        }
    }
    if map.is_empty() {
        return;
    }
    for n in nodes.iter_mut().filter(|n| !n.removed) {
        for (k, v) in n.attrs.iter_mut() {
            if *k == "id" {
                continue;
            }
            *v = replace_refs(v, n.name, |id| map.get(id).cloned());
        }
    }
}

fn replace_refs(v: &str, element: &str, f: impl Fn(&str) -> Option<String>) -> String {
    if let Some(id) = v.strip_prefix('#')
        && element == "use"
    {
        return format!("#{}", f(id).unwrap_or_else(|| id.to_string()));
    }
    if !v.contains("url(#") {
        return v.to_string();
    }
    let mut out = String::new();
    let mut rest = v;
    while let Some(at) = rest.find("url(#") {
        out.push_str(&rest[..at + 5]);
        let after = &rest[at + 5..];
        let end = after.find(')').unwrap_or(after.len());
        let id = after[..end].trim();
        out.push_str(&f(id).unwrap_or_else(|| id.to_string()));
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Short ids (`a`, `b`, ..., `Z`, `a0`, ...) for the referenced ones, in document order; ids
/// nothing references are removed.
fn rename_ids(nodes: &mut [Node]) {
    let mut referenced: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for n in nodes.iter().filter(|n| !n.removed) {
        for (k, v) in &n.attrs {
            if *k == "id" {
                continue;
            }
            for id in refs_in(v, n.name) {
                if !referenced.contains_key(id) {
                    referenced.insert(id.to_string(), String::new());
                    order.push(id.to_string());
                }
            }
        }
    }
    // name in order of definition
    let mut next = 0usize;
    for n in nodes.iter().filter(|n| !n.removed) {
        if let Some(id) = n.get("id")
            && let Some(slot) = referenced.get_mut(id)
            && slot.is_empty()
        {
            *slot = short_id(next);
            next += 1;
        }
    }
    for id in order {
        if referenced[&id].is_empty() {
            // referenced but not defined: keep a name that matches nothing
            let name = short_id(next);
            next += 1;
            referenced.insert(id, name);
        }
    }
    for n in nodes.iter_mut().filter(|n| !n.removed) {
        let name = n.name;
        let mut keep_id = None;
        for (k, v) in n.attrs.iter_mut() {
            if *k == "id" {
                keep_id = referenced.get(v.as_str()).cloned();
            } else {
                *v = replace_refs(v, name, |id| referenced.get(id).cloned());
            }
        }
        match keep_id {
            Some(id) => n.set("id", id),
            None => n.unset("id"),
        }
    }
}

fn short_id(mut k: usize) -> String {
    const FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const REST: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut s = String::new();
    s.push(FIRST[k % FIRST.len()] as char);
    k /= FIRST.len();
    while k > 0 {
        k -= 1;
        s.push(REST[k % REST.len()] as char);
        k /= REST.len();
    }
    s
}

/// Removes definitions nothing references and empty groups, unwraps groups without
/// attributes, and puts the id of a group around one path on the path.
fn prune(nodes: &mut [Node]) {
    // post-order from the root
    let mut post = Vec::with_capacity(nodes.len());
    let mut stack = vec![(0usize, false)];
    while let Some((i, done)) = stack.pop() {
        if done {
            post.push(i);
            continue;
        }
        stack.push((i, true));
        for &c in nodes[i].children.iter().rev() {
            stack.push((c, false));
        }
    }
    // groups whose children take their place in the parent's list
    let mut unwrap = vec![false; nodes.len()];
    for i in post {
        if nodes[i].removed {
            continue;
        }
        let mut live = Vec::with_capacity(nodes[i].children.len());
        for &c in &nodes[i].children {
            if nodes[c].removed {
                continue;
            }
            if unwrap[c] {
                live.extend(nodes[c].children.iter().copied());
                nodes[c].removed = true;
            } else {
                live.push(c);
            }
        }
        for &c in &live {
            nodes[c].parent = Some(i);
        }
        nodes[i].children = live;
        if i == 0 {
            continue;
        }
        let name = nodes[i].name;
        let has_id = nodes[i].get("id").is_some();
        let container = matches!(name, "g" | "defs");
        let unreferenced_def = !has_id && (REFERENCED_ONLY.contains(&name) || (!container && in_defs(nodes, i)));
        let empty_container = container && nodes[i].children.is_empty() && !has_id;
        if unreferenced_def || empty_container {
            nodes[i].removed = true;
            continue;
        }
        // a group with no attributes adds nothing: its children take its place; nor does a
        // group with only an id around one path without one (a glyph)
        let kids = &nodes[i].children;
        let only_id = nodes[i].attrs.len() == 1 && has_id;
        let lone_path = kids.len() == 1 && nodes[kids[0]].name == "path" && nodes[kids[0]].get("id").is_none();
        if name == "g" && nodes[i].attrs.is_empty() {
            unwrap[i] = true;
        } else if name == "g" && only_id && lone_path {
            let k = kids[0];
            let id = nodes[i].get("id").unwrap_or_default().to_string();
            nodes[k].attrs.insert(0, ("id", id));
            nodes[i].unset("id");
            unwrap[i] = true;
        }
    }
}

fn write_node(nodes: &[Node], i: usize, out: &mut String) {
    let n = &nodes[i];
    out.push('<');
    out.push_str(n.name);
    if i == 0 && n.parent.is_none() {
        out.push_str(" xmlns=\"");
        out.push_str(SVG_NS);
        out.push('"');
    }
    for (k, v) in &n.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        for ch in v.chars() {
            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\n' | '\r' | '\t' => out.push(' '),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    let kids: Vec<usize> = n.children.iter().copied().filter(|&c| !nodes[c].removed).collect();
    if kids.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for c in kids {
        write_node(nodes, c, out);
    }
    out.push_str("</");
    out.push_str(n.name);
    out.push('>');
}

// ---- path data ----

struct PathLexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl PathLexer<'_> {
    fn skip_sep(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b',') {
            self.i += 1;
        }
    }
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c') {
            self.i += 1;
        }
    }
    /// A number per the SVG grammar: sign, digits, fraction, exponent.
    fn number(&mut self) -> Option<f64> {
        let s = self.s;
        let start = self.i;
        let mut i = self.i;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        let digits = |i: &mut usize| {
            let b = *i;
            while *i < s.len() && s[*i].is_ascii_digit() {
                *i += 1;
            }
            *i > b
        };
        let int = digits(&mut i);
        let mut frac = false;
        if i < s.len() && s[i] == b'.' {
            i += 1;
            frac = digits(&mut i);
        }
        if !int && !frac {
            return None;
        }
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
                j += 1;
            }
            if j < s.len() && s[j].is_ascii_digit() {
                i = j;
                digits(&mut i);
            }
        }
        let v: f64 = std::str::from_utf8(&s[start..i]).ok()?.parse().ok()?;
        self.i = i;
        v.is_finite().then_some(v)
    }
    fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        let b = *self.s.get(self.i)?;
        self.i += 1;
        match b {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// One segment, absolute, in integer units of 10^-digits.
#[derive(Clone, Copy)]
enum Seg {
    M(i64, i64),
    L(i64, i64),
    H(i64),
    V(i64),
    C([i64; 6]),
    S([i64; 4]),
    Q([i64; 4]),
    T(i64, i64),
    A { rx: i64, ry: i64, rot: i64, large: bool, sweep: bool, x: i64, y: i64 },
    Z,
}

const MAX_COORD: f64 = 1e9;

fn to_units(v: f64, digits: u8) -> Result<i64, Rejected> {
    if !v.is_finite() || v.abs() > MAX_COORD {
        return reject("number out of range");
    }
    Ok((v * 10f64.powi(i32::from(digits))).round() as i64)
}

/// Path data rewritten compactly at `digits` decimals.
pub(crate) fn path_data(d: &str, digits: u8) -> Result<String, Rejected> {
    let mut segs = parse_path(d, digits)?;
    // a moveto followed by another moveto or by nothing draws nothing (cairo ends closed paths
    // with one)
    let keep: Vec<bool> = (0..segs.len())
        .map(|k| !(matches!(segs[k], Seg::M(..)) && matches!(segs.get(k + 1), None | Some(Seg::M(..)))))
        .collect();
    let mut k = 0;
    segs.retain(|_| {
        k += 1;
        keep[k - 1]
    });
    Ok(write_path(&segs, digits))
}

fn parse_path(d: &str, digits: u8) -> Result<Vec<Seg>, Rejected> {
    let mut p = PathLexer { s: d.as_bytes(), i: 0 };
    let mut segs = Vec::new();
    // current point and subpath start, unrounded
    let (mut cx, mut cy, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut cmd: Option<u8> = None;
    let u = |v: f64| to_units(v, digits);
    loop {
        p.skip_sep();
        if p.i >= p.s.len() {
            break;
        }
        let b = p.s[p.i];
        if b.is_ascii_alphabetic() {
            if !b"MmLlHhVvCcSsQqTtAaZz".contains(&b) {
                return reject("path command");
            }
            p.i += 1;
            cmd = Some(b);
            if b == b'Z' || b == b'z' {
                segs.push(Seg::Z);
                cx = sx;
                cy = sy;
                cmd = None;
                continue;
            }
        } else if cmd.is_none() {
            return reject("path data");
        }
        let c = cmd.unwrap_or(b'M');
        let rel = c.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        let mut num = || -> Result<f64, Rejected> {
            p.skip_sep();
            p.number().ok_or(Rejected("path number".into()))
        };
        match c.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = (num()? + ox, num()? + oy);
                segs.push(Seg::M(u(x)?, u(y)?));
                (cx, cy, sx, sy) = (x, y, x, y);
                // further pairs are line-tos
                cmd = Some(if rel { b'l' } else { b'L' });
            }
            b'L' => {
                let (x, y) = (num()? + ox, num()? + oy);
                segs.push(Seg::L(u(x)?, u(y)?));
                (cx, cy) = (x, y);
            }
            b'H' => {
                let x = num()? + ox;
                segs.push(Seg::H(u(x)?));
                cx = x;
            }
            b'V' => {
                let y = num()? + if rel { cy } else { 0.0 };
                segs.push(Seg::V(u(y)?));
                cy = y;
            }
            b'C' => {
                let v = [num()? + ox, num()? + oy, num()? + ox, num()? + oy, num()? + ox, num()? + oy];
                segs.push(Seg::C([u(v[0])?, u(v[1])?, u(v[2])?, u(v[3])?, u(v[4])?, u(v[5])?]));
                (cx, cy) = (v[4], v[5]);
            }
            b'S' | b'Q' => {
                let v = [num()? + ox, num()? + oy, num()? + ox, num()? + oy];
                let a = [u(v[0])?, u(v[1])?, u(v[2])?, u(v[3])?];
                segs.push(if c.eq_ignore_ascii_case(&b'S') { Seg::S(a) } else { Seg::Q(a) });
                (cx, cy) = (v[2], v[3]);
            }
            b'T' => {
                let (x, y) = (num()? + ox, num()? + oy);
                segs.push(Seg::T(u(x)?, u(y)?));
                (cx, cy) = (x, y);
            }
            b'A' => {
                let rx = num()?;
                let ry = num()?;
                let rot = num()?;
                let large = p.flag().ok_or(Rejected("arc flag".into()))?;
                let sweep = p.flag().ok_or(Rejected("arc flag".into()))?;
                p.skip_sep();
                let x = p.number().ok_or(Rejected("path number".into()))? + ox;
                p.skip_sep();
                let y = p.number().ok_or(Rejected("path number".into()))? + oy;
                segs.push(Seg::A {
                    rx: u(rx.abs())?,
                    ry: u(ry.abs())?,
                    rot: to_units(rot, 3)?,
                    large,
                    sweep,
                    x: u(x)?,
                    y: u(y)?,
                });
                (cx, cy) = (x, y);
            }
            _ => return reject("path command"),
        }
        p.skip_ws();
    }
    Ok(segs)
}

/// Writes numbers with the fewest separators: none before a minus sign, nor before a leading
/// `.` when the previous number already has one.
struct NumWriter<'a> {
    out: &'a mut String,
    /// the last thing written was a number, and whether it holds a '.'
    last: Option<bool>,
    digits: u8,
}

impl NumWriter<'_> {
    fn letter(&mut self, c: u8) {
        self.out.push(c as char);
        self.last = None;
    }
    fn num_with(&mut self, v: i64, digits: u8) {
        let mut s = String::new();
        fmt_units(&mut s, v, digits);
        if let Some(had_dot) = self.last {
            let b = s.as_bytes()[0];
            if !(b == b'-' || (b == b'.' && had_dot)) {
                self.out.push(' ');
            }
        }
        self.last = Some(s.contains('.'));
        self.out.push_str(&s);
    }
    fn num(&mut self, v: i64) {
        self.num_with(v, self.digits);
    }
    fn flag(&mut self, f: bool) {
        if self.last.is_some() {
            self.out.push(' ');
        }
        self.out.push(if f { '1' } else { '0' });
        self.last = Some(false);
    }
}

/// Length of `v` formatted.
fn units_len(v: i64, digits: u8) -> usize {
    let mut s = String::new();
    fmt_units(&mut s, v, digits);
    s.len() + 1
}

/// A segment as absolute and relative values, its letter and its end point.
type SegParts = (Vec<i64>, Vec<i64>, u8, Option<(i64, i64)>);

fn write_path(segs: &[Seg], digits: u8) -> String {
    let mut out = String::with_capacity(segs.len() * 12);
    let mut w = NumWriter { out: &mut out, last: None, digits };
    let (mut cx, mut cy, mut sx, mut sy) = (0i64, 0i64, 0i64, 0i64);
    // the command a bare number continues (after M, L; after m, l)
    let mut implicit: Option<u8> = None;
    let len = |vals: &[i64]| vals.iter().map(|&v| units_len(v, digits)).sum::<usize>();
    for (k, seg) in segs.iter().enumerate() {
        let (abs_vals, rel_vals, letter, end): SegParts = match *seg {
            Seg::M(x, y) => (vec![x, y], vec![x - cx, y - cy], b'M', Some((x, y))),
            Seg::L(x, y) => {
                // a horizontal or vertical line is shorter as H or V
                if y == cy && x != cx {
                    (vec![x], vec![x - cx], b'H', Some((x, y)))
                } else if x == cx && y != cy {
                    (vec![y], vec![y - cy], b'V', Some((x, y)))
                } else {
                    (vec![x, y], vec![x - cx, y - cy], b'L', Some((x, y)))
                }
            }
            Seg::H(x) => (vec![x], vec![x - cx], b'H', Some((x, cy))),
            Seg::V(y) => (vec![y], vec![y - cy], b'V', Some((cx, y))),
            Seg::C(a) => (
                a.to_vec(),
                vec![a[0] - cx, a[1] - cy, a[2] - cx, a[3] - cy, a[4] - cx, a[5] - cy],
                b'C',
                Some((a[4], a[5])),
            ),
            Seg::S(a) => (a.to_vec(), vec![a[0] - cx, a[1] - cy, a[2] - cx, a[3] - cy], b'S', Some((a[2], a[3]))),
            Seg::Q(a) => (a.to_vec(), vec![a[0] - cx, a[1] - cy, a[2] - cx, a[3] - cy], b'Q', Some((a[2], a[3]))),
            Seg::T(x, y) => (vec![x, y], vec![x - cx, y - cy], b'T', Some((x, y))),
            Seg::A { rx, ry, rot, large, sweep, x, y } => {
                // arcs stay absolute
                w.letter(b'A');
                w.num(rx);
                w.num(ry);
                w.num_with(rot, 3);
                w.flag(large);
                w.flag(sweep);
                w.num(x);
                w.num(y);
                (cx, cy) = (x, y);
                implicit = Some(b'A');
                continue;
            }
            Seg::Z => {
                w.letter(b'z');
                (cx, cy) = (sx, sy);
                implicit = None;
                continue;
            }
        };
        // the first command of a path must be an absolute moveto (a relative one there is
        // relative to 0,0, the same)
        let use_rel = k > 0 && len(&rel_vals) < len(&abs_vals);
        let l = if use_rel { letter.to_ascii_lowercase() } else { letter };
        let vals = if use_rel { &rel_vals } else { &abs_vals };
        if implicit != Some(l) || letter == b'M' {
            w.letter(l);
        }
        for &v in vals {
            w.num(v);
        }
        implicit = Some(match l {
            b'M' => b'L',
            b'm' => b'l',
            o => o,
        });
        if let Some((x, y)) = end {
            (cx, cy) = (x, y);
            if letter == b'M' {
                (sx, sy) = (x, y);
            }
        }
    }
    out
}

/// `v` units of 10^-digits, shortest: no trailing zeros, no leading zero before the point.
fn fmt_units(out: &mut String, v: i64, digits: u8) {
    if v == 0 {
        out.push('0');
        return;
    }
    if v < 0 {
        out.push('-');
    }
    let a = v.unsigned_abs();
    let p = 10u64.pow(u32::from(digits));
    let (int, mut frac) = (a / p, a % p);
    if int > 0 {
        let _ = write!(out, "{int}");
    }
    if frac > 0 {
        let mut d = digits as usize;
        while frac % 10 == 0 {
            frac /= 10;
            d -= 1;
        }
        let _ = write!(out, ".{frac:0d$}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg(body: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" \
             xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"100pt\" height=\"50pt\" viewBox=\"10 20 100 50\">\n{body}\n</svg>\n"
        )
    }

    #[test]
    fn numbers_are_short() {
        let mut s = String::new();
        for (v, d) in [(5, 1), (-5, 1), (0, 2), (1230, 2), (-100, 1), (7, 0)] {
            fmt_units(&mut s, v, d);
            s.push('|');
        }
        assert_eq!(s, ".5|-.5|0|12.3|-10|7|");
    }

    #[test]
    fn paths_use_relative_commands_where_shorter() {
        let d = "M 100.04 200.01 L 100.54 200.01 L 100.54 210.26 C 101 211 102 212 103 213 Z M 0 0 L 0.5 0.5";
        assert_eq!(path_data(d, 1).unwrap(), "M100 200h.5v10.3c.5.7 1.5 1.7 2.5 2.7zM0 0 .5.5");
    }

    #[test]
    fn relative_input_is_resolved() {
        assert_eq!(path_data("m 10 10 l 5 0 l 0 5 z m 1 1 l 1 1", 1).unwrap(), "M10 10h5v5zm1 1 1 1");
    }

    #[test]
    fn compacts_a_cairo_like_figure() {
        let body = r##"<defs>
<g>
<g id="glyph-0-0">
<path d="M 1.378906 -2.878906 L 2 -2.878906 Z "/>
</g>
<g id="glyph-0-1">
<path d="M 1.378906 -2.878906 L 2 -2.878906 Z "/>
</g>
</g>
<clipPath id="clip-0"><path clip-rule="nonzero" d="M 10 20 L 110 20 L 110 70 L 10 70 Z "/></clipPath>
<clipPath id="clip-unused"><path d="M 0 0 L 1 1 Z"/></clipPath>
</defs>
<g clip-path="url(#clip-0)">
<path fill-rule="nonzero" fill="rgb(100%, 0%, 0%)" fill-opacity="1" d="M 20.123 30.456 L 40.987 30.456 "/>
</g>
<g fill="rgb(0%, 0%, 0%)" fill-opacity="1">
<use xlink:href="#glyph-0-0" x="50.5049" y="60.2"/>
<use xlink:href="#glyph-0-1" x="55.5049" y="60.2"/>
</g>
<g></g>"##;
        let out = compact(&svg(body), 1).unwrap();
        assert_eq!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100pt\" height=\"50pt\" viewBox=\"10 20 100 50\">\
             <defs><path id=\"a\" d=\"M1.4-2.9H2z\"/><clipPath id=\"b\"><path d=\"M10 20H110V70H10z\"/></clipPath></defs>\
             <g clip-path=\"url(#b)\"><path fill=\"#f00\" d=\"M20.1 30.5H41\"/></g>\
             <g fill=\"#000\"><use href=\"#a\" x=\"50.5\" y=\"60.2\"/><use href=\"#a\" x=\"55.5\" y=\"60.2\"/></g></svg>"
        );
    }

    #[test]
    fn scaled_content_keeps_more_digits() {
        let body = r#"<path transform="matrix(100, 0, 0, 100, 5.123456, 6)" d="M 0.123456 0.5 L 1 1"/>"#;
        let out = compact(&svg(body), 1).unwrap();
        assert!(out.contains("matrix(100 0 0 100 5.12 6)"), "{out}");
        assert!(out.contains("d=\"M.123.5 1 1\""), "{out}");
    }

    #[test]
    fn image_placement_keeps_two_decimals() {
        let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGNgAAAAAgABSK+kcQAAAABJRU5ErkJggg==";
        let body = format!(
            r#"<image transform="matrix(0.11523, 0, 0, 0.115303, 269.194218, 31.345342)" width="192" height="133" xlink:href="{png}"/>"#
        );
        let out = compact(&svg(&body), 1).unwrap();
        assert!(out.contains("matrix(.11523 0 0 .115303 269.19 31.35)"), "{out}");
    }

    fn rejected(body: &str) -> String {
        compact(&svg(body), 1).unwrap_err().0
    }

    #[test]
    fn the_allowlist_rejects_active_content() {
        assert!(rejected("<script>alert(1)</script>").contains("script"));
        assert!(rejected("<foreignObject><div/></foreignObject>").contains("foreignObject"));
        assert!(rejected("<style>path{}</style>").contains("style"));
        assert!(rejected(r#"<path onload="alert(1)" d="M0 0"/>"#).contains("onload"));
        assert!(rejected(r#"<path style="fill:red" d="M0 0"/>"#).contains("style"));
        assert!(rejected(r#"<use xlink:href="http://example.com/x.svg#a"/>"#).contains("local"));
        assert!(rejected(r#"<use href="other.svg#a"/>"#).contains("local"));
        assert!(rejected(r#"<image href="http://example.com/x.png"/>"#).contains("PNG"));
        assert!(rejected(r#"<image href="data:image/svg+xml;base64,PHN2Zz4="/>"#).contains("PNG"));
        // base64 of "GIF89a......"
        assert!(rejected(r#"<image href="data:image/png;base64,R0lGODlhAQABAAAAACw="/>"#).contains("PNG"));
        assert!(rejected(r#"<path fill="url(http://example.com/#a)" d="M0 0"/>"#).contains("url"));
        assert!(rejected(r#"<g mask="url(x.svg#m)"/>"#).contains("url"));
        assert!(rejected("<text>hi</text>").contains("text"));
        assert!(rejected("<svg/>").contains("root"));
        assert!(rejected(r##"<a href="#x"/>"##).contains("element a"));
        let doctype = "<?xml version=\"1.0\"?><!DOCTYPE svg [<!ENTITY x \"y\">]><svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\">&x;</svg>";
        assert!(compact(doctype, 1).unwrap_err().0.contains("xml"));
        let other_ns = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:h=\"http://www.w3.org/1999/xhtml\" viewBox=\"0 0 1 1\"><h:p/></svg>";
        assert!(compact(other_ns, 1).unwrap_err().0.contains("namespace"));
    }

    #[test]
    fn reference_cycles_are_rejected() {
        let body = r##"<defs><g id="a"><use xlink:href="#b"/></g><g id="b"><use xlink:href="#a"/></g></defs><use xlink:href="#a"/>"##;
        assert!(rejected(body).contains("cycle"));
    }

    #[test]
    fn values_are_escaped() {
        // an id can hold none of these, and a path rejects them: escaping guards the rest
        let body = r##"<path fill="#123456" d="M0 0"/>"##;
        let out = compact(&svg(body), 2).unwrap();
        assert!(out.contains("fill=\"#123456\""));
    }
}
