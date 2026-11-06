//! Formula structure from glyph geometry: a baseline layout tree in the manner of DRACULAE
//! (Zanibbi, Blostein and Cordy 2002), built from positioned symbols and rules, and its LaTeX.
//!
//! A formula is split into horizontal segments wherever no symbol or rule covers an x
//! position. A segment is a single symbol, a fraction (a rule with symbols above and below),
//! a radical (a radical sign and the rule over its radicand), an accented base, a big
//! operator with stacked limits, or a stack of rows. The segments of a row are then read
//! left to right against the row's baseline: smaller symbols above or below it become
//! superscripts and subscripts of the symbol before them. Rows of full-size symbols at
//! several baselines are matrices, cases or aligned equations, split into cells at gaps
//! that are common to every row.
//!
//! Every step is bounded: the recursion depth, the symbols per formula and the work units
//! a formula may use. A formula that hits a bound is reported as failed.

use super::symbols::{self, Class, FontRole, Style, Sym};

/// One positioned symbol of a formula.
#[derive(Clone, Debug)]
pub struct Atom {
    pub x0: f64,
    pub x1: f64,
    pub base: f64,
    pub size: f64,
    pub sym: Sym,
    pub role: FontRole,
    /// a letter of an upright text font (LaTeX `\mathrm`, or `\text` for words)
    pub upright_text: bool,
    /// a TeX extension-font glyph (CMEX and look-alikes) whose baseline is its top
    pub tex_ext: bool,
    /// gap to the previous glyph of the same word is a space (for `\text` runs)
    pub word_start: bool,
    /// the vertical extent of a delimiter merged from stacked pieces (top, bottom)
    pub span: Option<(f64, f64)>,
}

impl Atom {
    fn cx(&self) -> f64 {
        (self.x0 + self.x1) / 2.0
    }

    /// Symbols that are centred on the math axis and whose baseline says nothing about the
    /// row (big operators, big or extensible delimiters, radical signs).
    fn axis_symbol(&self) -> bool {
        self.tex_ext
            || matches!(self.sym.class, Class::LargeOp | Class::Radical | Class::Piece)
            || (self.sym.big && matches!(self.sym.class, Class::Open | Class::Close | Class::Fence))
    }
}

/// A thin horizontal rule (fraction bar, radical vinculum, overline).
#[derive(Clone, Copy, Debug)]
pub struct Bar {
    pub x0: f64,
    pub x1: f64,
    pub y: f64,
}

#[derive(Clone, Debug)]
pub enum Node {
    Leaf(Atom),
    Group(Vec<Item>),
    Frac(Box<Item>, Box<Item>),
    Sqrt(Box<Item>, Option<Box<Item>>),
    Over(Box<Item>),
    Under(Box<Item>),
    Accent(&'static str, Box<Item>),
    Scripts {
        base: Box<Item>,
        sub: Option<Box<Item>>,
        sup: Option<Box<Item>>,
    },
    /// rows at several baselines that are not yet resolved (scripts or a matrix)
    Stack(Vec<Item>),
    Matrix {
        env: &'static str,
        rows: Vec<Vec<Item>>,
    },
}

#[derive(Clone, Debug)]
pub struct Item {
    pub node: Node,
    pub x0: f64,
    pub x1: f64,
    /// the baseline this item sits on (for a fraction, the math axis below its bar)
    pub base: f64,
    pub size: f64,
    /// whether `base` is evidence for the row's baseline
    pub votes: bool,
}

impl Item {
    fn leaf(a: Atom) -> Item {
        let votes = !a.axis_symbol();
        Item { x0: a.x0, x1: a.x1, base: a.base, size: a.size, votes, node: Node::Leaf(a) }
    }

    fn group(items: Vec<Item>) -> Item {
        let (x0, x1) = span(&items);
        let size = items.iter().map(|i| i.size).fold(0.0, f64::max);
        let base = row_baseline(&items).unwrap_or_else(|| items.first().map_or(0.0, |i| i.base));
        let votes = items.iter().any(|i| i.votes);
        Item { node: Node::Group(items), x0, x1, base, size, votes }
    }

    fn is_empty(&self) -> bool {
        matches!(&self.node, Node::Group(v) if v.is_empty())
    }

    fn class(&self) -> Option<Class> {
        match &self.node {
            Node::Leaf(a) => Some(a.sym.class),
            _ => None,
        }
    }
}

fn span(items: &[Item]) -> (f64, f64) {
    let x0 = items.iter().map(|i| i.x0).fold(f64::INFINITY, f64::min);
    let x1 = items.iter().map(|i| i.x1).fold(f64::NEG_INFINITY, f64::max);
    if x0.is_finite() { (x0, x1) } else { (0.0, 0.0) }
}

/// The baseline of a row: the median baseline of its voting items of the largest size.
fn row_baseline(items: &[Item]) -> Option<f64> {
    let s = items.iter().filter(|i| i.votes).map(|i| i.size).fold(0.0, f64::max);
    let mut b: Vec<f64> = items.iter().filter(|i| i.votes && i.size >= 0.9 * s).map(|i| i.base).collect();
    if b.is_empty() {
        return None;
    }
    b.sort_by(f64::total_cmp);
    Some(b[b.len() / 2])
}

/// Limits and penalties of one formula.
pub struct Ctx {
    pub work: u64,
    pub max_work: u64,
    /// set when a bound was hit; the formula must not be used
    pub failed: bool,
    /// 0 = certain; each doubtful decision adds to it
    pub doubt: f64,
}

impl Ctx {
    pub fn new(max_work: u64) -> Ctx {
        Ctx { work: 0, max_work, failed: false, doubt: 0.0 }
    }

    fn charge(&mut self, units: usize) -> bool {
        self.work = self.work.saturating_add(units as u64);
        if self.work > self.max_work {
            self.failed = true;
        }
        !self.failed
    }
}

const MAX_DEPTH: usize = 24;

/// Merge stacked pieces of extensible delimiters (and integral pieces) into one big symbol.
pub fn merge_pieces(atoms: Vec<Atom>) -> Vec<Atom> {
    if !atoms.iter().any(|a| a.sym.class == Class::Piece) {
        return atoms;
    }
    let (mut pieces, mut out): (Vec<Atom>, Vec<Atom>) = atoms.into_iter().partition(|a| a.sym.class == Class::Piece);
    pieces.sort_by(|a, b| a.cx().total_cmp(&b.cx()).then(a.base.total_cmp(&b.base)));
    let mut i = 0;
    while i < pieces.len() {
        let mut j = i + 1;
        let tol = 0.35 * pieces[i].size.max(1.0);
        while j < pieces.len()
            && (pieces[j].cx() - pieces[i].cx()).abs() <= tol
            && pieces[j].sym.text == pieces[i].sym.text
        {
            j += 1;
        }
        let group = &pieces[i..j];
        let mut a = group[0].clone();
        a.x0 = group.iter().map(|p| p.x0).fold(f64::INFINITY, f64::min);
        a.x1 = group.iter().map(|p| p.x1).fold(f64::NEG_INFINITY, f64::max);
        let (top, bottom) = (
            group.iter().map(|p| p.base).fold(f64::INFINITY, f64::min),
            group.iter().map(|p| p.base).fold(f64::NEG_INFINITY, f64::max),
        );
        a.base = (top + bottom) / 2.0;
        a.span = Some((top - a.size, bottom + 0.5 * a.size));
        a.sym.class = match a.sym.text.as_str() {
            "(" | "[" | "{" | "⌈" | "⌊" | "⟨" => Class::Open,
            ")" | "]" | "}" | "⌉" | "⌋" | "⟩" => Class::Close,
            "∫" => Class::LargeOp,
            "√" => Class::Radical,
            _ => Class::Fence,
        };
        if group.len() == 1 && group[0].sym.piece == Some(symbols::PieceKind::Extender) && a.sym.class != Class::Fence {
            // a lone extender: a vertical line segment
            a.sym.class = Class::Fence;
            a.sym.text = "|".into();
        }
        // a brace extender alone could be either side: decide by the neighbours later
        a.sym.piece = None;
        a.sym.big = true;
        out.push(a);
        i = j;
    }
    out
}

/// Delimiters drawn at a larger font size than the formula's symbols are big; vertical
/// bars stacked at one x are pieces of one tall bar.
fn mark_big(atoms: &mut [Atom]) {
    let mut sizes: Vec<f64> = atoms
        .iter()
        .filter(|a| !a.axis_symbol() && !matches!(a.sym.class, Class::Open | Class::Close | Class::Fence))
        .map(|a| a.size)
        .collect();
    if sizes.is_empty() {
        return;
    }
    sizes.sort_by(f64::total_cmp);
    let median = sizes[sizes.len() / 2];
    for a in atoms.iter_mut() {
        if a.sym.text == "∣" && a.size >= 1.35 * median {
            a.sym.text = "|".into();
            a.sym.class = Class::Fence;
        }
        if matches!(a.sym.class, Class::Open | Class::Close | Class::Fence | Class::LargeOp | Class::Radical)
            && a.size >= 1.35 * median
        {
            a.sym.big = true;
        }
    }
    // fences of the same character at one x on several baselines
    let fences: Vec<usize> = (0..atoms.len())
        .filter(|&i| (atoms[i].sym.class == Class::Fence || atoms[i].sym.text == "∣") && !atoms[i].sym.big)
        .collect();
    for &i in &fences {
        let key = |t: &str| if t == "∣" { "|".to_string() } else { t.to_string() };
        let twin = fences.iter().any(|&j| {
            j != i
                && key(&atoms[j].sym.text) == key(&atoms[i].sym.text)
                && (atoms[j].cx() - atoms[i].cx()).abs() <= 0.25 * atoms[i].size
                && (atoms[j].base - atoms[i].base).abs() <= 1.3 * atoms[i].size
                && (atoms[j].base - atoms[i].base).abs() > 0.3 * atoms[i].size
        });
        if twin {
            if atoms[i].sym.text == "∣" {
                atoms[i].sym.text = "|".into();
            }
            atoms[i].sym.class = Class::Piece;
            atoms[i].sym.piece = Some(symbols::PieceKind::Extender);
        }
    }
    // a bar with relation spacing on both sides is \mid (TeX sets \mid as a relation and a
    // plain | tight)
    for i in 0..atoms.len() {
        if atoms[i].sym.text != "|" || atoms[i].sym.big || atoms[i].sym.class != Class::Fence {
            continue;
        }
        let a = &atoms[i];
        let same = |b: &Atom| (b.base - a.base).abs() <= 0.2 * a.size && b.size >= 0.8 * a.size;
        // a pair of bars on one baseline is a pair of fences (|x|, a determinant)
        if atoms.iter().enumerate().any(|(j, b)| j != i && same(b) && matches!(b.sym.text.as_str(), "|" | "∣")) {
            continue;
        }
        let left =
            atoms.iter().filter(|b| same(b) && b.x1 <= a.x0 + 0.01).map(|b| a.x0 - b.x1).fold(f64::INFINITY, f64::min);
        let right =
            atoms.iter().filter(|b| same(b) && b.x0 >= a.x1 - 0.01).map(|b| b.x0 - a.x1).fold(f64::INFINITY, f64::min);
        if left.is_finite() && right.is_finite() && left >= 0.18 * a.size && right >= 0.18 * a.size {
            atoms[i].sym.text = "∣".into();
            atoms[i].sym.class = Class::Rel;
        }
    }
}

/// A formula's layout tree.
pub fn parse(mut atoms: Vec<Atom>, bars: Vec<Bar>, ctx: &mut Ctx) -> Item {
    if atoms.len() <= 4000 {
        mark_big(&mut atoms);
    }
    let atoms = merge_pieces(atoms);
    parse_row(atoms, bars, ctx, 0)
}

fn parse_row(atoms: Vec<Atom>, bars: Vec<Bar>, ctx: &mut Ctx, depth: usize) -> Item {
    parse_row_with(atoms, bars, Vec::new(), ctx, depth)
}

/// [`parse_row`] with items already read (matrices) that belong to the row.
fn parse_row_with(atoms: Vec<Atom>, bars: Vec<Bar>, extra: Vec<Item>, ctx: &mut Ctx, depth: usize) -> Item {
    if atoms.is_empty() {
        return assemble(extra, ctx, depth + 1);
    }
    if depth > MAX_DEPTH || !ctx.charge(atoms.len() + bars.len()) {
        ctx.failed = true;
        return flat(atoms);
    }
    let (atoms, bars, mut items) = matrix_regions(atoms, bars, ctx, depth);
    items.extend(extra);
    if atoms.is_empty() && bars.is_empty() {
        return assemble(items, ctx, depth + 1);
    }
    let segments = segment(atoms, bars);
    items.reserve(segments.len());
    for (a, b) in segments {
        items.push(parse_segment(a, b, ctx, depth + 1));
        if ctx.failed {
            break;
        }
    }
    assemble(items, ctx, depth + 1)
}

/// Row baselines of a set of atoms (see [`split_rows`]) that no bar explains: a fraction's
/// numerator and denominator are rows with a bar between them.
fn unexplained_rows(rows: &[Vec<Atom>], bars: &[Bar]) -> Vec<bool> {
    let base = |r: &Vec<Atom>| {
        let s = r.iter().filter(|a| !a.axis_symbol()).map(|a| a.size).fold(0.0, f64::max);
        let mut b: Vec<f64> = r.iter().filter(|a| !a.axis_symbol() && a.size >= 0.9 * s).map(|a| a.base).collect();
        b.sort_by(f64::total_cmp);
        (b.get(b.len() / 2).copied().unwrap_or(0.0), s.max(1.0))
    };
    let ext = |r: &Vec<Atom>| {
        let s = r.iter().filter(|a| !a.axis_symbol()).map(|a| a.size).fold(0.0, f64::max);
        let full = || r.iter().filter(|a| !a.axis_symbol() && a.size >= 0.9 * s);
        (full().map(|a| a.x0).fold(f64::INFINITY, f64::min), full().map(|a| a.x1).fold(f64::NEG_INFINITY, f64::max))
    };
    rows.windows(2)
        .map(|w| {
            let ((b0, s0), (b1, s1)) = (base(&w[0]), base(&w[1]));
            let s = s0.max(s1);
            let ((a0, a1), (c0, c1)) = (ext(&w[0]), ext(&w[1]));
            let (o0, o1) = (a0.max(c0), a1.min(c1));
            // rows side by side (a fraction next to the rest of its row) are not stacked
            if o1 - o0 < 0.3 * (a1 - a0).min(c1 - c0) {
                return false;
            }
            // explained: every full-size symbol of the upper row is a numerator (over a bar
            // between the rows), or every one of the lower row is a denominator
            let between = |b: &Bar| b.y >= b0 - 0.6 * s && b.y <= b1;
            let full = |r: &Vec<Atom>| {
                let m = r.iter().filter(|a| !a.axis_symbol()).map(|a| a.size).fold(0.0, f64::max);
                r.iter().filter(move |a| !a.axis_symbol() && a.size >= 0.9 * m).cloned().collect::<Vec<Atom>>()
            };
            let under = |a: &Atom, b: &Bar| a.cx() >= b.x0 - 0.1 * s && a.cx() <= b.x1 + 0.1 * s;
            let nums = full(&w[0]).iter().all(|a| bars.iter().any(|b| between(b) && b.y > a.base && under(a, b)));
            let dens = full(&w[1]).iter().all(|a| bars.iter().any(|b| between(b) && b.y < a.base && under(a, b)));
            !(nums || dens)
        })
        .collect()
}

/// Matrices, cases and aligned rows read row by row: the atoms between a pair of big
/// delimiters (or after a lone big brace, or a whole formula at the top level) that lie on
/// several rows no bar explains. Returns the other atoms and bars, and the matrices.
fn matrix_regions(atoms: Vec<Atom>, bars: Vec<Bar>, ctx: &mut Ctx, depth: usize) -> (Vec<Atom>, Vec<Bar>, Vec<Item>) {
    // big delimiters, and (when few) any delimiter: OpenType fonts draw big variants at the
    // same font size, so only the rows between them tell
    let n_delims = atoms
        .iter()
        .filter(|a| matches!(a.sym.class, Class::Open | Class::Close | Class::Fence) || a.sym.text == "∣")
        .count();
    let is_delim = |a: &Atom| {
        (a.sym.big || n_delims <= 64)
            && (matches!(a.sym.class, Class::Open | Class::Close | Class::Fence) || a.sym.text == "∣")
    };
    let delims: Vec<usize> = {
        let mut d: Vec<usize> = (0..atoms.len()).filter(|&i| is_delim(&atoms[i])).collect();
        d.sort_by(|&a, &b| atoms[a].x0.total_cmp(&atoms[b].x0));
        d
    };
    let multi = |content: &[Atom], bars: &[Bar]| {
        let rows = split_rows(content);
        rows.len() >= 2 && unexplained_rows(&rows, bars).iter().any(|&u| u)
    };
    // outermost pairs: an opening delimiter and its closing partner (or, for a brace, none)
    let mut regions: Vec<(Option<usize>, Option<usize>, f64, f64)> = Vec::new();
    let mut k = 0;
    while k < delims.len() {
        let i = delims[k];
        let a = &atoms[i];
        let opens = a.sym.class == Class::Open || a.sym.class == Class::Fence || a.sym.text == "∣";
        if !opens {
            k += 1;
            continue;
        }
        let mut depth_d = 0;
        let mut close = None;
        for (m, &j) in delims.iter().enumerate().skip(k + 1) {
            let b = &atoms[j];
            match b.sym.class {
                Class::Open => depth_d += 1,
                Class::Close if depth_d > 0 => depth_d -= 1,
                Class::Close => {
                    close = Some((m, j));
                    break;
                }
                _ if (a.sym.class == Class::Fence || a.sym.text == "∣") && depth_d == 0 && b.sym.text == a.sym.text =>
                {
                    close = Some((m, j));
                    break;
                }
                _ => {}
            }
        }
        // delimiters of one font size enclose rows only around their own baseline (OpenType
        // size variants keep the font size; their baseline is the matrix's axis)
        let close = close.filter(|&(_, j)| {
            let b = &atoms[j];
            a.sym.big && b.sym.big || (a.base - b.base).abs() <= 0.3 * a.size.max(b.size)
        });
        match close {
            Some((m, j)) => {
                regions.push((Some(i), Some(j), a.x1, atoms[j].x0));
                k = m + 1;
            }
            None if a.sym.text == "{" => {
                regions.push((Some(i), None, a.x1, f64::INFINITY));
                k = delims.len();
            }
            None => k += 1,
        }
    }
    let mut taken = vec![false; atoms.len()];
    let mut bar_taken = vec![false; bars.len()];
    let mut out = Vec::new();
    for (l, r, x0, x1) in regions {
        let inside: Vec<usize> = (0..atoms.len())
            .filter(|&i| !taken[i] && Some(i) != l && Some(i) != r && atoms[i].cx() > x0 && atoms[i].cx() < x1)
            .collect();
        if inside.is_empty() {
            continue;
        }
        // only what the delimiters reach vertically
        let reach = |d: &Atom| -> (f64, f64) {
            if let Some(sp) = d.span {
                sp
            } else if d.tex_ext {
                // an extension-font glyph hangs from its baseline
                (d.base - 0.6 * d.size, d.base + 3.4 * d.size)
            } else if d.sym.big {
                (d.base - 2.2 * d.size, d.base + 1.6 * d.size)
            } else {
                (d.base - 3.0 * d.size, d.base + 3.0 * d.size)
            }
        };
        let (top, bottom) = [l, r]
            .into_iter()
            .flatten()
            .map(|i| reach(&atoms[i]))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |acc, (t, b)| (acc.0.min(t), acc.1.max(b)));
        let inside: Vec<usize> = if top.is_finite() {
            inside.into_iter().filter(|&i| atoms[i].base >= top && atoms[i].base <= bottom).collect()
        } else {
            inside
        };
        if inside.is_empty() {
            continue;
        }
        let content: Vec<Atom> = inside.iter().map(|&i| atoms[i].clone()).collect();
        if let Some(d) = l.map(|i| &atoms[i]).filter(|d| !d.sym.big) {
            // rows above and below the delimiter's own baseline
            let s = d.size;
            let above = content.iter().any(|a| !a.axis_symbol() && a.size >= 0.85 * s && a.base < d.base - 0.2 * s);
            let below = content.iter().any(|a| !a.axis_symbol() && a.size >= 0.85 * s && a.base > d.base + 0.2 * s);
            if !(above && below) {
                continue;
            }
            // matrix rows stay between the delimiters; lines of a multi-line equation do not
            let (lx, rx) = (d.x0, r.map_or(f64::INFINITY, |j| atoms[j].x1));
            let outside = content
                .iter()
                .filter(|a| (a.base - d.base).abs() > 0.3 * s && !a.axis_symbol() && a.size >= 0.85 * s)
                .any(|a| {
                    atoms.iter().any(|b| {
                        !b.axis_symbol()
                            && b.size >= 0.85 * s
                            && (b.base - a.base).abs() <= 0.3 * s
                            && (b.x1 <= lx || b.x0 >= rx)
                    })
                });
            if outside {
                continue;
            }
        }
        let inner_bars: Vec<usize> =
            (0..bars.len()).filter(|&b| bars[b].x0 >= x0 - 1.0 && bars[b].x1 <= x1 + 1.0).collect();
        let bv: Vec<Bar> = inner_bars.iter().map(|&b| bars[b]).collect();
        if !multi(&content, &bv) {
            continue;
        }
        let left = l.map(|i| atoms[i].sym.text.clone());
        let right = r.map(|i| atoms[i].sym.text.clone());
        let env = match (left.as_deref(), right.as_deref()) {
            (Some("("), Some(")")) => "pmatrix",
            (Some("["), Some("]")) => "bmatrix",
            (Some("{"), Some("}")) => "Bmatrix",
            (Some("|" | "∣"), Some("|" | "∣")) => "vmatrix",
            (Some("‖"), Some("‖")) => "Vmatrix",
            (Some("{"), None) => "cases",
            (None, None) => "aligned",
            _ => "matrix",
        };
        let mut m = matrix_rows(content, bv, Vec::new(), env, ctx, depth);
        if env == "matrix" {
            // delimiters the environment cannot spell stay symbols around it
            for i in [l, r].into_iter().flatten() {
                out.push(Item::leaf(atoms[i].clone()));
                taken[i] = true;
            }
        } else {
            for i in [l, r].into_iter().flatten() {
                m.x0 = m.x0.min(atoms[i].x0);
                m.x1 = m.x1.max(atoms[i].x1);
                taken[i] = true;
            }
        }
        for &i in &inside {
            taken[i] = true;
        }
        for b in inner_bars {
            bar_taken[b] = true;
        }
        out.push(m);
    }
    let (rest, rest_bars) = if out.is_empty() {
        (atoms, bars)
    } else {
        (
            atoms.into_iter().enumerate().filter(|(i, _)| !taken[*i]).map(|(_, a)| a).collect(),
            bars.into_iter().enumerate().filter(|(i, _)| !bar_taken[*i]).map(|(_, b)| b).collect(),
        )
    };
    // a whole display on several lines no bar explains: aligned rows, each read on its own
    if depth == 0 && multi(&rest, &rest_bars) {
        let m = matrix_rows(rest, rest_bars, out, "aligned", ctx, depth);
        return (Vec::new(), Vec::new(), vec![m]);
    }
    (rest, rest_bars, out)
}

/// A matrix from atoms on several rows: rows a bar explains are merged (fractions in cells),
/// each row is read on its own, and cells are cut at gaps common to every row.
fn matrix_rows(
    content: Vec<Atom>,
    bars: Vec<Bar>,
    extra: Vec<Item>,
    env: &'static str,
    ctx: &mut Ctx,
    depth: usize,
) -> Item {
    let rows = split_rows(&content);
    let unexplained = unexplained_rows(&rows, &bars);
    let mut merged: Vec<Vec<Atom>> = Vec::new();
    for (k, r) in rows.into_iter().enumerate() {
        if k > 0 && !unexplained[k - 1] {
            merged.last_mut().unwrap().extend(r);
        } else {
            merged.push(r);
        }
    }
    let s = max_size(&content);
    // items already read go to the row whose vertical span holds their baseline
    let spans: Vec<(f64, f64)> = merged
        .iter()
        .map(|r| {
            (
                r.iter().map(|a| a.base - a.size).fold(f64::INFINITY, f64::min),
                r.iter().map(|a| a.base + 0.5 * a.size).fold(f64::NEG_INFINITY, f64::max),
            )
        })
        .collect();
    let mut extras: Vec<Vec<Item>> = vec![Vec::new(); merged.len().max(1)];
    for it in extra {
        let k = spans
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let d = |(t, u): &(f64, f64)| {
                    if it.base < *t {
                        t - it.base
                    } else if it.base > *u {
                        it.base - u
                    } else {
                        0.0
                    }
                };
                d(a.1).total_cmp(&d(b.1))
            })
            .map_or(0, |(k, _)| k);
        extras[k].push(it);
    }
    let mut row_items: Vec<Vec<Item>> = Vec::new();
    let mut row_bases = Vec::new();
    for (k, r) in merged.into_iter().enumerate() {
        let (top, bot) = spans[k];
        let rb: Vec<Bar> = bars.iter().copied().filter(|b| b.y > top && b.y < bot).collect();
        let it = parse_row_with(r, rb, std::mem::take(&mut extras[k]), ctx, depth + 1);
        row_bases.push(it.base);
        row_items.push(match it.node {
            Node::Group(v) => v,
            other => vec![Item { node: other, ..it }],
        });
    }
    // column cuts: gaps no row item covers, wider than any space between symbols
    let mut spans: Vec<(f64, f64)> = row_items.iter().flatten().map(|i| (i.x0, i.x1)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cuts: Vec<f64> = Vec::new();
    let mut end = f64::NEG_INFINITY;
    for (a, b) in spans {
        if end.is_finite() && a - end >= 0.45 * s {
            cuts.push((a + end) / 2.0);
        }
        end = end.max(b);
    }
    let aligned = env == "aligned";
    let any_rel = row_items.iter().any(|row| row.iter().skip(1).any(|i| i.class() == Some(Class::Rel)));
    let rows: Vec<Vec<Item>> = row_items
        .into_iter()
        .map(|row| {
            if aligned {
                // split each row before its first relation; a continuation line (no relation)
                // goes after the alignment point
                let k = row.iter().position(|i| i.class() == Some(Class::Rel)).filter(|&k| k > 0);
                let mut row = row;
                match k {
                    Some(k) => {
                        let rhs = row.split_off(k);
                        vec![Item::group(row), Item::group(rhs)]
                    }
                    None if any_rel => vec![Item::group(Vec::new()), Item::group(row)],
                    None => vec![Item::group(row)],
                }
            } else {
                let mut cols: Vec<Vec<Item>> = vec![Vec::new(); cuts.len() + 1];
                for it in row {
                    let c = cuts.iter().filter(|&&x| x < (it.x0 + it.x1) / 2.0).count();
                    cols[c].push(it);
                }
                cols.into_iter().map(Item::group).collect()
            }
        })
        .collect();

    let (x0, x1) = (
        content.iter().map(|a| a.x0).fold(f64::INFINITY, f64::min),
        content.iter().map(|a| a.x1).fold(f64::NEG_INFINITY, f64::max),
    );
    let base = row_bases.get(row_bases.len() / 2).copied().unwrap_or(0.0);
    let _ = depth;
    Item { node: Node::Matrix { env, rows }, x0, x1, base, size: s, votes: false }
}

/// The atoms as one row, left to right, for formulas that failed.
fn flat(mut atoms: Vec<Atom>) -> Item {
    atoms.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    Item::group(atoms.into_iter().map(Item::leaf).collect())
}

/// Split atoms and bars into horizontal segments that do not overlap.
fn segment(atoms: Vec<Atom>, bars: Vec<Bar>) -> Vec<(Vec<Atom>, Vec<Bar>)> {
    #[derive(Clone, Copy)]
    enum Ref {
        A(usize),
        B(usize),
    }
    let mut spans: Vec<(f64, f64, Ref)> = Vec::with_capacity(atoms.len() + bars.len());
    for (i, a) in atoms.iter().enumerate() {
        let w = (a.x1 - a.x0).max(0.0);
        // shrink by an italic-overhang tolerance so neighbours that touch do not merge; a
        // radical sign reaches its vinculum
        let e = (0.12 * a.size).min(0.3 * w);
        let combining =
            a.sym.class == Class::Accent
                && (w <= 0.01
                    || a.sym.text.chars().next().is_some_and(|c| {
                        ('\u{300}'..='\u{36f}').contains(&c) || ('\u{20d0}'..='\u{20ff}').contains(&c)
                    }));
        let (s, t) = if a.sym.text == "\u{338}" {
            // a negation slash is drawn over the relation that follows it
            (a.x0, a.x0 + 0.5 * a.size)
        } else if combining && w < 0.3 * a.size {
            // a combining mark sits over the glyph before it (often drawn at that glyph's end)
            (a.x0 - 0.6 * a.size, a.x0 + w.min(0.3 * a.size).max(0.01))
        } else if w <= 0.01 {
            (a.x0 - 0.01, a.x0 + 0.01)
        } else {
            (a.x0 + e, a.x1 - e)
        };
        let t = if a.sym.class == Class::Radical { a.x1 + 0.15 * a.size } else { t };
        spans.push((s, t.max(s), Ref::A(i)));
    }
    for (i, b) in bars.iter().enumerate() {
        spans.push((b.x0, b.x1, Ref::B(i)));
    }
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(Vec<Atom>, Vec<Bar>)> = Vec::new();
    let mut end = f64::NEG_INFINITY;
    let mut cur: (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    let flush = |cur: &mut (Vec<usize>, Vec<usize>), out: &mut Vec<(Vec<Atom>, Vec<Bar>)>| {
        if !cur.0.is_empty() || !cur.1.is_empty() {
            out.push((cur.0.iter().map(|&i| atoms[i].clone()).collect(), cur.1.iter().map(|&i| bars[i]).collect()));
            cur.0.clear();
            cur.1.clear();
        }
    };
    for (s, t, r) in spans {
        if s >= end {
            flush(&mut cur, &mut out);
        }
        match r {
            Ref::A(i) => cur.0.push(i),
            Ref::B(i) => cur.1.push(i),
        }
        end = end.max(t);
    }
    flush(&mut cur, &mut out);
    out
}

fn max_size(atoms: &[Atom]) -> f64 {
    atoms.iter().map(|a| a.size).fold(0.0, f64::max).max(1.0)
}

fn parse_segment(mut atoms: Vec<Atom>, mut bars: Vec<Bar>, ctx: &mut Ctx, depth: usize) -> Item {
    if !ctx.charge(atoms.len() + bars.len()) {
        return flat(atoms);
    }
    if atoms.len() == 1 && bars.is_empty() {
        return Item::leaf(atoms.pop().unwrap());
    }
    let s = max_size(&atoms);
    if atoms.is_empty() {
        // a rule with nothing near it (a dash or a table rule): drop it
        ctx.doubt += 0.1;
        return Item::group(Vec::new());
    }
    // negated relations: a negation slash over a relation
    if atoms.len() == 2
        && bars.is_empty()
        && let Some(k) = atoms.iter().position(|a| a.sym.text == "\u{338}")
    {
        let rel = atoms.swap_remove(1 - k);
        let mut a = rel.clone();
        a.sym.text = match rel.sym.text.as_str() {
            "=" => "≠",
            "∈" => "∉",
            "≡" => "≢",
            "<" => "≮",
            ">" => "≯",
            "≤" => "≰",
            "≥" => "≱",
            "⊂" => "⊄",
            "⊃" => "⊅",
            "⊆" => "⊈",
            "∼" => "≁",
            "≈" => "≉",
            "∣" | "|" => "∤",
            _ => {
                a.sym.text = format!("{}\u{338}", rel.sym.text);
                return Item::leaf(a);
            }
        }
        .to_string();
        a.sym.class = Class::Rel;
        return Item::leaf(a);
    }
    // the widest bar decides: fraction, radical, overline or underline
    bars.sort_by(|a, b| (b.x1 - b.x0).total_cmp(&(a.x1 - a.x0)));
    if let Some(&bar) = bars.first() {
        let width = bar.x1 - bar.x0;
        // radical: a radical sign whose right edge meets the bar's left end
        if let Some(r) = atoms.iter().position(|a| {
            a.sym.class == Class::Radical && (bar.x0 - a.x1).abs() <= 0.45 * a.size.max(1.0) && a.x0 < bar.x0
        }) {
            let rad = atoms.swap_remove(r);
            let (mut body, mut index, mut rest) = (Vec::new(), Vec::new(), Vec::new());
            for a in atoms {
                if a.cx() >= bar.x0 - 0.1 * s && a.cx() <= bar.x1 + 0.1 * s && a.base > bar.y {
                    body.push(a);
                } else if a.cx() < rad.x1 && a.size < 0.9 * rad.size.max(s) {
                    index.push(a);
                } else {
                    rest.push(a);
                }
            }
            let inner: Vec<Bar> = bars[1..].iter().copied().filter(|b| b.y > bar.y).collect();
            let body = parse_row(body, inner, ctx, depth + 1);
            let index = (!index.is_empty()).then(|| Box::new(parse_row(index, Vec::new(), ctx, depth + 1)));
            let item = Item {
                x0: rad.x0,
                x1: bar.x1,
                base: body.base,
                size: body.size.max(rad.size.min(s)),
                votes: body.votes,
                node: Node::Sqrt(Box::new(body), index),
            };
            return with_rest(item, rest, ctx, depth);
        }
        let inside = |a: &Atom| a.cx() >= bar.x0 - 0.15 * s && a.cx() <= bar.x1 + 0.15 * s;
        let (mut above, mut below, mut rest) = (Vec::new(), Vec::new(), Vec::new());
        for a in atoms {
            if !inside(&a) {
                rest.push(a);
            } else if a.base < bar.y {
                above.push(a);
            } else {
                below.push(a);
            }
        }
        let others: Vec<Bar> = bars[1..].to_vec();
        let (bars_above, bars_below): (Vec<Bar>, Vec<Bar>) = others.into_iter().partition(|b| b.y < bar.y);
        let covered = above.iter().chain(below.iter()).map(|a| a.x1 - a.x0).fold(0.0, f64::max);
        if width + 0.5 * s < covered {
            ctx.doubt += 0.2;
        }
        let item = match (above.is_empty(), below.is_empty()) {
            (false, false) => {
                let num = parse_row(above, bars_above, ctx, depth + 1);
                let den = parse_row(below, bars_below, ctx, depth + 1);
                let size = num.size.max(den.size);
                Item {
                    x0: bar.x0,
                    x1: bar.x1,
                    base: bar.y + 0.25 * size,
                    size,
                    votes: true,
                    node: Node::Frac(Box::new(num), Box::new(den)),
                }
            }
            (true, false) => {
                let body = parse_row(below, bars_below, ctx, depth + 1);
                Item {
                    x0: bar.x0.min(body.x0),
                    x1: bar.x1.max(body.x1),
                    base: body.base,
                    size: body.size,
                    votes: body.votes,
                    node: Node::Over(Box::new(body)),
                }
            }
            (false, true) => {
                let body = parse_row(above, bars_above, ctx, depth + 1);
                Item {
                    x0: bar.x0.min(body.x0),
                    x1: bar.x1.max(body.x1),
                    base: body.base,
                    size: body.size,
                    votes: body.votes,
                    node: Node::Under(Box::new(body)),
                }
            }
            (true, true) => Item::group(Vec::new()),
        };
        return with_rest(item, rest, ctx, depth);
    }
    // an accent over (or under) a base
    if let Some(k) = atoms.iter().position(|a| a.sym.class == Class::Accent) {
        let acc = atoms.swap_remove(k);
        let cmd = acc.sym.text.chars().next().and_then(symbols::accent_command).unwrap_or("\\hat");
        let combining = acc.x1 - acc.x0 <= 0.01
            || acc
                .sym
                .text
                .chars()
                .next()
                .is_some_and(|c| ('\u{300}'..='\u{36f}').contains(&c) || ('\u{20d0}'..='\u{20ff}').contains(&c));
        let nearest_left =
            atoms.iter().filter(|a| a.x0 <= acc.x0 + 0.05 * a.size).map(|a| a.x0).fold(f64::NEG_INFINITY, f64::max);
        let reach = |a: &Atom| {
            let w = (acc.x1 - acc.x0).max(0.0);
            if combining {
                // the glyph the mark is drawn on or right after
                (a.x0 <= acc.x0 + 0.05 * a.size && a.x0 >= nearest_left - 0.01) || (a.x0 <= acc.x0 && acc.x0 <= a.x1)
            } else if w <= 0.01 {
                a.x0 - 0.1 * a.size <= acc.x0 && acc.x0 <= a.x1 + 0.1 * a.size
            } else {
                let ov = acc.x1.min(a.x1) - acc.x0.max(a.x0);
                ov >= 0.3 * w.min(a.x1 - a.x0).max(0.01)
            }
        };
        let (base, rest): (Vec<Atom>, Vec<Atom>) = atoms.into_iter().partition(reach);
        if base.is_empty() {
            ctx.doubt += 0.3;
            return with_rest(Item::leaf(acc), rest, ctx, depth);
        }
        let body = parse_row(base, Vec::new(), ctx, depth + 1);
        let item = Item {
            x0: body.x0.min(acc.x0),
            x1: body.x1.max(acc.x1),
            base: body.base,
            size: body.size,
            votes: body.votes,
            node: Node::Accent(cmd, Box::new(body)),
        };
        return with_rest(item, rest, ctx, depth);
    }
    // a big operator with limits stacked over and under it
    if let Some(k) = atoms.iter().position(|a| a.sym.class == Class::LargeOp) {
        let op = atoms.swap_remove(k);
        let centre = if op.tex_ext {
            op.base + 0.5 * op.size * if op.sym.big { 1.4 } else { 1.0 }
        } else {
            op.base - 0.3 * op.size
        };
        let (over, under): (Vec<Atom>, Vec<Atom>) = atoms.into_iter().partition(|a| a.base < centre);
        let sup = (!over.is_empty()).then(|| Box::new(parse_row(over, Vec::new(), ctx, depth + 1)));
        let sub = (!under.is_empty()).then(|| Box::new(parse_row(under, Vec::new(), ctx, depth + 1)));
        let (x0, x1, size) = (op.x0, op.x1, op.size);
        let base = Item::leaf(op);
        let mut item = Item {
            x0,
            x1,
            base: base.base,
            size,
            votes: false,
            node: Node::Scripts { base: Box::new(base), sub, sup },
        };
        for s in [&item.node] {
            if let Node::Scripts { sub, sup, .. } = s {
                for c in [sub, sup].into_iter().flatten() {
                    item.x0 = item.x0.min(c.x0);
                    item.x1 = item.x1.max(c.x1);
                }
            }
        }
        return item;
    }
    // rows: cluster the baselines of the largest symbols
    rows_or_line(atoms, ctx, depth)
}

/// An item plus atoms of the same segment that were not part of it, as one row.
fn with_rest(item: Item, rest: Vec<Atom>, ctx: &mut Ctx, depth: usize) -> Item {
    if rest.is_empty() {
        return item;
    }
    ctx.doubt += 0.03;
    let mut items = vec![item];
    let r = parse_row(rest, Vec::new(), ctx, depth + 1);
    match r.node {
        Node::Group(v) => items.extend(v),
        _ => items.push(r),
    }
    items.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    Item::group(items)
}

/// Atoms that overlap horizontally: one line of overlapping glyphs, or several rows.
fn rows_or_line(atoms: Vec<Atom>, ctx: &mut Ctx, depth: usize) -> Item {
    let rows = split_rows(&atoms);
    if rows.len() <= 1 {
        let mut atoms = atoms;
        atoms.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        // same baseline: the glyphs just touch or kern into each other
        return Item::group(atoms.into_iter().map(Item::leaf).collect());
    }
    let mut items = Vec::with_capacity(rows.len());
    for r in rows {
        items.push(parse_row(r, Vec::new(), ctx, depth + 1));
    }
    let (x0, x1) = span(&items);
    let size = items.iter().map(|i| i.size).fold(0.0, f64::max);
    let base = items[0].base;
    Item { node: Node::Stack(items), x0, x1, base, size, votes: false }
}

/// Rows of a set of atoms: baselines of the largest atoms more than 0.45 of their size apart;
/// smaller atoms join the nearest row.
fn split_rows(atoms: &[Atom]) -> Vec<Vec<Atom>> {
    let s = atoms.iter().filter(|a| !a.axis_symbol()).map(|a| a.size).fold(0.0, f64::max);
    if s <= 0.0 {
        return vec![atoms.to_vec()];
    }
    let mut bases: Vec<f64> = atoms.iter().filter(|a| !a.axis_symbol() && a.size >= 0.9 * s).map(|a| a.base).collect();
    bases.sort_by(f64::total_cmp);
    let mut rows: Vec<f64> = Vec::new();
    let mut members = 0usize;
    for b in bases {
        match rows.last_mut() {
            Some(last) if b - *last <= 0.45 * s => {
                members += 1;
                *last += (b - *last) / members as f64;
            }
            _ => {
                rows.push(b);
                members = 1;
            }
        }
    }
    if rows.len() <= 1 {
        return vec![atoms.to_vec()];
    }
    // full-size atoms go to their row; smaller ones (scripts) to the row of the horizontally
    // nearest full-size atom among the rows within reach, else the nearest baseline
    let nearest_row = |b: f64| {
        rows.iter().enumerate().min_by(|x, y| (x.1 - b).abs().total_cmp(&(y.1 - b).abs())).map_or(0, |(k, _)| k)
    };
    let full: Vec<(usize, &Atom)> =
        atoms.iter().filter(|a| !a.axis_symbol() && a.size >= 0.9 * s).map(|a| (nearest_row(a.base), a)).collect();
    let mut out: Vec<Vec<Atom>> = vec![Vec::new(); rows.len()];
    for a in atoms {
        let k = if !a.axis_symbol() && a.size >= 0.9 * s {
            nearest_row(a.base)
        } else {
            // a superscript sits up to about half an em above its row's baseline, a subscript
            // a little below it: rows under the atom are preferred
            let gap = |f: &Atom| (a.x0 - f.x1).max(f.x0 - a.x1).max(0.0);
            let vertical = |k: usize| if rows[k] >= a.base { rows[k] - a.base } else { 1.6 * (a.base - rows[k]) };
            let score = |(k, f): &(usize, &Atom)| vertical(*k) + 0.5 * gap(f);
            full.iter()
                .filter(|(k, _)| (rows[*k] - a.base).abs() <= 0.9 * s)
                .min_by(|x, y| score(x).total_cmp(&score(y)))
                .map_or_else(|| nearest_row(a.base), |(k, _)| *k)
        };
        out[k].push(a.clone());
    }
    out.retain(|r| !r.is_empty());
    out
}

/// Items of one row, left to right, with scripts attached to their bases and matrices
/// resolved.
fn assemble(mut items: Vec<Item>, ctx: &mut Ctx, depth: usize) -> Item {
    items.retain(|i| !i.is_empty());
    items.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    if items.len() == 1 && !matches!(items[0].node, Node::Stack(_)) {
        return items.pop().unwrap();
    }
    if depth > MAX_DEPTH || !ctx.charge(items.len()) {
        ctx.failed = true;
        return Item::group(items);
    }
    let items = matrices(items, ctx, depth);
    let base = row_baseline(&items);
    let s = items.iter().filter(|i| i.votes).map(|i| i.size).fold(0.0, f64::max);
    let s = if s > 0.0 { s } else { items.iter().map(|i| i.size).fold(1.0, f64::max) };
    let Some(b) = base else {
        // no symbol fixes a baseline (only operators, delimiters or stacks)
        return Item::group(items.into_iter().map(|i| resolve_stack(i, ctx, depth)).collect());
    };
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    let mut pending: Vec<Item> = Vec::new();
    for it in items {
        if is_script(&it, b, s) && (!out.is_empty() || it.size < 0.8 * s) {
            pending.push(it);
            continue;
        }
        attach(&mut out, std::mem::take(&mut pending), b, s, ctx, depth);
        out.push(resolve_stack(it, ctx, depth));
    }
    attach(&mut out, pending, b, s, ctx, depth);
    let mut g = Item::group(out);
    g.base = b;
    g
}

/// A stack that is not a script: its rows as a matrix.
fn resolve_stack(it: Item, ctx: &mut Ctx, depth: usize) -> Item {
    match it.node {
        Node::Stack(rows) => {
            ctx.doubt += 0.15;
            let rows = rows.into_iter().map(|r| vec![r]).collect();
            let _ = depth;
            Item { node: Node::Matrix { env: "matrix", rows }, ..it }
        }
        _ => it,
    }
}

/// Is an item a superscript or subscript candidate for the row with baseline `b` and size `s`?
fn is_script(it: &Item, b: f64, s: f64) -> bool {
    match &it.node {
        Node::Leaf(a) if a.axis_symbol() => false,
        Node::Matrix { .. } => false,
        Node::Stack(rows) => rows.iter().all(|r| r.size <= 0.9 * s),
        Node::Frac(..) => it.size <= 0.85 * s && (it.base - b).abs() > 0.2 * s,
        _ => it.size <= 0.92 * s && ((it.base - b) < -0.1 * s || (it.base - b) > 0.05 * s),
    }
}

/// Attach pending script items to the last item of `out` (or an empty base).
fn attach(out: &mut Vec<Item>, pending: Vec<Item>, b: f64, s: f64, ctx: &mut Ctx, depth: usize) {
    if pending.is_empty() {
        return;
    }
    let (mut sup, mut sub): (Vec<Item>, Vec<Item>) = (Vec::new(), Vec::new());
    for it in pending {
        match it.node {
            Node::Stack(rows) => {
                for r in rows {
                    if r.base < b { sup.push(r) } else { sub.push(r) }
                }
            }
            _ => {
                if it.base < b - 0.02 * s {
                    sup.push(it)
                } else {
                    sub.push(it)
                }
            }
        }
    }
    let script = |v: Vec<Item>, ctx: &mut Ctx| -> Option<Box<Item>> {
        if v.is_empty() {
            return None;
        }
        Some(Box::new(assemble(v, ctx, depth + 1)))
    };
    let sup = script(sup, ctx);
    let sub = script(sub, ctx);
    let base = match out.pop() {
        Some(prev) => prev,
        None => {
            // a script with no base before it: a prescript, or a misread
            ctx.doubt += 0.12;
            Item::group(Vec::new())
        }
    };
    let (mut x0, mut x1) = (base.x0, base.x1);
    for c in [&sub, &sup].into_iter().flatten() {
        x0 = x0.min(c.x0);
        x1 = x1.max(c.x1);
    }
    let (bb, bs, bv) = (base.base, base.size, base.votes);
    // a base that already has scripts gets more: a prime after a subscript, or the rest of a
    // script that segmentation split (a limit that starts under the operator)
    let join = |old: Option<Box<Item>>, new: Option<Box<Item>>, ctx: &mut Ctx| -> Option<Box<Item>> {
        match (old, new) {
            (Some(o), Some(n)) => {
                let mut v = match o.node {
                    Node::Group(v) => v,
                    other => vec![Item { node: other, ..*o }],
                };
                match n.node {
                    Node::Group(w) => v.extend(w),
                    other => v.push(Item { node: other, ..*n }),
                }
                Some(Box::new(assemble(v, ctx, depth + 1)))
            }
            (o, n) => o.or(n),
        }
    };
    let node = match base.node {
        Node::Scripts { base: inner, sub: s0, sup: p0 } => {
            Node::Scripts { base: inner, sub: join(s0, sub, ctx), sup: join(p0, sup, ctx) }
        }
        n => Node::Scripts {
            base: Box::new(Item { node: n, x0: base.x0, x1: base.x1, base: bb, size: bs, votes: bv }),
            sub,
            sup,
        },
    };
    let _ = s;
    out.push(Item { node, x0, x1, base: bb, size: bs, votes: bv });
}

/// Matrices, cases and aligned rows: a run of full-size stacks between delimiters (or the
/// whole row), re-read row by row and split into cells at gaps common to every row.
fn matrices(items: Vec<Item>, ctx: &mut Ctx, depth: usize) -> Vec<Item> {
    let full = |i: &Item, s: f64| matches!(&i.node, Node::Stack(rows) if rows.iter().filter(|r| r.size >= 0.85 * s).count() >= 2);
    let s = items.iter().map(|i| i.size).fold(0.0, f64::max);
    if !items.iter().any(|i| full(i, s)) {
        return items;
    }
    let mut out: Vec<Item> = Vec::new();
    let mut i = 0;
    let n = items.len();
    let mut items: Vec<Option<Item>> = items.into_iter().map(Some).collect();
    while i < n {
        let it = items[i].as_ref().unwrap();
        let open = matches!(it.class(), Some(Class::Open | Class::Fence));
        // the region: an opening delimiter followed by stacks, up to its closing delimiter
        if open && i + 1 < n && items[i + 1].as_ref().is_some_and(|x| full(x, s)) {
            let mut depth_d = 0i32;
            let mut j = i + 1;
            let mut close = None;
            let fence = it.class() == Some(Class::Fence);
            while j < n {
                let c = items[j].as_ref().unwrap().class();
                match c {
                    Some(Class::Open) => depth_d += 1,
                    Some(Class::Close) if depth_d > 0 => depth_d -= 1,
                    Some(Class::Close) => {
                        close = Some(j);
                        break;
                    }
                    Some(Class::Fence) if fence && depth_d == 0 => {
                        close = Some(j);
                        break;
                    }
                    _ => {}
                }
                j += 1;
            }
            let end = close.unwrap_or(n);
            let left = text_of(items[i].as_ref().unwrap());
            let right = close.map(|c| text_of(items[c].as_ref().unwrap()));
            let env = match (left.as_str(), right.as_deref()) {
                ("(", Some(")")) => "pmatrix",
                ("[", Some("]")) => "bmatrix",
                ("{", Some("}")) => "Bmatrix",
                ("|", Some("|")) => "vmatrix",
                ("‖", Some("‖")) => "Vmatrix",
                ("{", None) => "cases",
                _ => "",
            };
            let inner: Vec<Item> = (i + 1..end).filter_map(|k| items[k].take()).collect();
            if env.is_empty() {
                // keep the delimiters as symbols around a plain matrix
                out.push(items[i].take().unwrap());
                out.push(matrix(inner, "matrix", ctx, depth));
                if let Some(c) = close {
                    out.push(items[c].take().unwrap());
                }
            } else {
                let (x0, x1) = (
                    items[i].as_ref().unwrap().x0,
                    close.and_then(|c| items[c].as_ref()).map_or(span(&inner).1, |c| c.x1),
                );
                let mut m = matrix(inner, env, ctx, depth);
                m.x0 = x0;
                m.x1 = x1;
                out.push(m);
                items[i] = None;
                if let Some(c) = close {
                    items[c] = None;
                }
            }
            i = close.map_or(n, |c| c + 1);
            continue;
        }
        if full(it, s) {
            // stacks without delimiters: the maximal run of stacks
            let mut j = i;
            while j < n && items[j].as_ref().is_some_and(|x| full(x, s) || !matches!(x.node, Node::Leaf(_))) {
                j += 1;
            }
            let inner: Vec<Item> = (i..j.max(i + 1)).filter_map(|k| items[k].take()).collect();
            out.push(matrix(inner, if depth <= 2 { "aligned" } else { "matrix" }, ctx, depth));
            i = j.max(i + 1);
            continue;
        }
        out.push(items[i].take().unwrap());
        i += 1;
    }
    out
}

fn text_of(it: &Item) -> String {
    match &it.node {
        Node::Leaf(a) => a.sym.text.clone(),
        _ => String::new(),
    }
}

/// Rows and cells from the items between a matrix's delimiters.
fn matrix(inner: Vec<Item>, env: &'static str, ctx: &mut Ctx, depth: usize) -> Item {
    // row anchors: the baselines of full-size rows of the stacks, and of full-size items
    let s = inner.iter().map(|i| i.size).fold(0.0, f64::max).max(1.0);
    let mut pieces: Vec<Item> = Vec::new();
    for it in inner {
        match it.node {
            Node::Stack(rows) => pieces.extend(rows),
            _ => pieces.push(it),
        }
    }
    let mut anchors: Vec<f64> = pieces
        .iter()
        .filter(|p| p.size >= 0.85 * s && (p.votes || matches!(p.node, Node::Frac(..))))
        .map(|p| p.base)
        .collect();
    anchors.sort_by(f64::total_cmp);
    let mut rows_b: Vec<f64> = Vec::new();
    for a in anchors {
        match rows_b.last() {
            Some(&l) if a - l <= 0.45 * s => {}
            _ => rows_b.push(a),
        }
    }
    if rows_b.is_empty() {
        rows_b.push(pieces.first().map_or(0.0, |p| p.base));
    }
    let mut rows: Vec<Vec<Item>> = vec![Vec::new(); rows_b.len()];
    for p in pieces {
        let k = rows_b
            .iter()
            .enumerate()
            .min_by(|x, y| (x.1 - p.base).abs().total_cmp(&(y.1 - p.base).abs()))
            .map_or(0, |(k, _)| k);
        rows[k].push(p);
    }
    let rows: Vec<Item> = rows.into_iter().filter(|r| !r.is_empty()).map(|r| assemble(r, ctx, depth + 1)).collect();
    // flatten each row into its top-level items
    let rows: Vec<Vec<Item>> = rows
        .into_iter()
        .map(|r| match r.node {
            Node::Group(v) => v,
            _ => vec![r],
        })
        .collect();
    // column gaps: x intervals no row item covers, at least 0.45 em wide (wider than any
    // space between symbols)
    let mut spans: Vec<(f64, f64)> = rows.iter().flatten().map(|i| (i.x0, i.x1)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cuts: Vec<f64> = Vec::new();
    let mut end = f64::NEG_INFINITY;
    for (a, b) in spans {
        if end.is_finite() && a - end >= 0.45 * s {
            cuts.push((a + end) / 2.0);
        }
        end = end.max(b);
    }
    let cells: Vec<Vec<Item>> = if env == "aligned" && cuts.is_empty() {
        // aligned equations: split each row before its first relation
        rows.into_iter()
            .map(|row| {
                let k = row.iter().position(|i| i.class() == Some(Class::Rel));
                match k {
                    Some(k) if k > 0 => {
                        let mut row = row;
                        let rhs = row.split_off(k);
                        vec![Item::group(row), Item::group(rhs)]
                    }
                    _ => vec![Item::group(row)],
                }
            })
            .collect()
    } else {
        rows.into_iter()
            .map(|row| {
                let mut cols: Vec<Vec<Item>> = vec![Vec::new(); cuts.len() + 1];
                for it in row {
                    let c = cuts.iter().filter(|&&x| x < (it.x0 + it.x1) / 2.0).count();
                    cols[c].push(it);
                }
                cols.into_iter().map(Item::group).collect()
            })
            .collect()
    };
    let env = if env == "aligned" && !cuts.is_empty() { "matrix" } else { env };
    let all: Vec<&Item> = cells.iter().flatten().collect();
    let x0 = all.iter().map(|i| i.x0).fold(f64::INFINITY, f64::min);
    let x1 = all.iter().map(|i| i.x1).fold(f64::NEG_INFINITY, f64::max);
    let base = rows_b[rows_b.len() / 2];
    Item { node: Node::Matrix { env, rows: cells }, x0, x1, base, size: s, votes: false }
}

// ---------------------------------------------------------------------------------------
// LaTeX

/// LaTeX of a layout tree.
pub fn latex(item: &Item) -> String {
    let mut w = Tex::default();
    emit(item, &mut w);
    w.out.trim().to_string()
}

#[derive(Default)]
struct Tex {
    out: String,
    /// the last thing written was a control word (`\alpha`): a letter must not follow directly
    after_word: bool,
}

impl Tex {
    fn push(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if self.after_word && s.starts_with(|c: char| c.is_ascii_alphabetic()) {
            self.out.push(' ');
        }
        self.out.push_str(s);
        // a control word: backslash then letters at the end
        let tail = s.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        self.after_word = tail.ends_with('\\') && tail.len() < s.len();
    }

    fn group(&mut self, item: &Item) {
        self.push("{");
        self.after_word = false;
        emit(item, self);
        self.push("}");
    }
}

fn leaf_tex(a: &Atom) -> String {
    let t = &a.sym.text;
    let mut chars = t.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return t.chars().map(|c| symbols::tex_of_char(c).map_or_else(|| c.to_string(), str::to_string)).collect();
    };
    if c.is_ascii_alphabetic() || matches!(c, 'ı' | 'ȷ') || ('\u{391}'..='\u{3ff}').contains(&c) {
        let core = symbols::tex_of_char(c).map_or_else(|| c.to_string(), str::to_string);
        let style = if a.upright_text && a.sym.style == Style::Default { Style::Upright } else { a.sym.style };
        // Greek and letters in the default style need no command
        if c.is_ascii_alphabetic()
            && let Some(cmd) = style.command()
        {
            return format!("{cmd}{{{core}}}");
        }
        if matches!(style, Style::Bold | Style::BoldItalic | Style::SansBold | Style::SansBoldItalic) {
            return format!("\\boldsymbol{{{core}}}");
        }
        return core;
    }
    if c.is_ascii_digit() {
        return match a.sym.style {
            Style::Bold => format!("\\mathbf{{{c}}}"),
            Style::DoubleStruck => format!("\\mathbb{{{c}}}"),
            _ => c.to_string(),
        };
    }
    symbols::tex_of_char(c).map_or_else(|| c.to_string(), str::to_string)
}

fn emit(item: &Item, w: &mut Tex) {
    match &item.node {
        Node::Leaf(a) => w.push(&leaf_tex(a)),
        Node::Group(items) => emit_row(items, w),
        Node::Frac(n, d) => {
            w.push("\\frac");
            w.group(n);
            w.group(d);
        }
        Node::Sqrt(body, index) => {
            w.push("\\sqrt");
            if let Some(i) = index {
                w.push("[");
                w.after_word = false;
                emit(i, w);
                w.push("]");
            }
            w.group(body);
        }
        Node::Over(b) => {
            w.push("\\overline");
            w.group(b);
        }
        Node::Under(b) => {
            w.push("\\underline");
            w.group(b);
        }
        Node::Accent(cmd, b) => {
            w.push(cmd);
            w.group(b);
        }
        Node::Scripts { base, sub, sup } => {
            if base.is_empty() {
                w.push("{}");
            } else if needs_group(base) {
                w.group(base);
            } else {
                emit(base, w);
            }
            if let Some(s) = sub {
                w.push("_");
                w.group(s);
            }
            if let Some(s) = sup {
                if primes(s) {
                    let n = count_primes(s);
                    w.push(&"'".repeat(n));
                } else {
                    w.push("^");
                    w.group(s);
                }
            }
        }
        Node::Stack(rows) => {
            w.push("\\begin{matrix}");
            for (k, r) in rows.iter().enumerate() {
                if k > 0 {
                    w.push("\\\\");
                }
                emit(r, w);
            }
            w.push("\\end{matrix}");
        }
        Node::Matrix { env, rows } => {
            w.push(&format!("\\begin{{{env}}}"));
            w.after_word = false;
            for (k, r) in rows.iter().enumerate() {
                if k > 0 {
                    w.push("\\\\");
                    w.after_word = false;
                }
                for (c, cell) in r.iter().enumerate() {
                    if c > 0 {
                        w.push("&");
                        w.after_word = false;
                    }
                    emit(cell, w);
                }
            }
            w.push(&format!("\\end{{{env}}}"));
            w.after_word = false;
        }
    }
}

fn needs_group(it: &Item) -> bool {
    match &it.node {
        Node::Group(v) => v.len() > 1,
        Node::Scripts { .. } => true,
        _ => false,
    }
}

fn primes(it: &Item) -> bool {
    match &it.node {
        Node::Leaf(a) => matches!(a.sym.text.as_str(), "′" | "″" | "‴" | "'"),
        Node::Group(v) => !v.is_empty() && v.iter().all(primes),
        _ => false,
    }
}

fn count_primes(it: &Item) -> usize {
    match &it.node {
        Node::Leaf(a) => match a.sym.text.as_str() {
            "″" => 2,
            "‴" => 3,
            _ => 1,
        },
        Node::Group(v) => v.iter().map(count_primes).sum(),
        _ => 0,
    }
}

/// A row: runs of upright text letters become `\text{...}` or an operator name, and pairs
/// of double bars around something are a norm.
fn emit_row(items: &[Item], w: &mut Tex) {
    let double = |it: &Item| matches!(&it.node, Node::Leaf(a) if matches!(a.sym.text.as_str(), "‖" | "∥"));
    let doubles: Vec<usize> = (0..items.len()).filter(|&k| double(&items[k])).collect();
    let mut norm_open: Option<usize> = None;
    let mut i = 0;
    while i < items.len() {
        if double(&items[i]) && doubles.len() >= 2 {
            let k = doubles.iter().position(|&d| d == i).unwrap_or(0);
            let opening = norm_open.is_none() && k + 1 < doubles.len() && doubles[k + 1] > i + 1;
            if opening {
                norm_open = Some(i);
                w.push("\\lVert");
                i += 1;
                continue;
            }
            if norm_open.is_some() {
                norm_open = None;
                w.push("\\rVert");
                i += 1;
                continue;
            }
        }
        let upright = |a: &Atom| {
            (a.upright_text && a.sym.style == Style::Default)
                || (a.sym.style == Style::Upright && a.role != FontRole::EulerRoman)
        };
        if let Node::Leaf(a) = &items[i].node
            && upright(a)
            && a.sym.text.chars().all(|c| c.is_ascii_alphabetic())
        {
            let mut j = i + 1;
            while j < items.len()
                && let Node::Leaf(b) = &items[j].node
                && upright(b)
                && b.sym.text.chars().all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '-')
            {
                j += 1;
            }
            if j - i >= 2 {
                let mut word = String::new();
                for (k, it) in items[i..j].iter().enumerate() {
                    if let Node::Leaf(b) = &it.node {
                        if k > 0 && (b.word_start || it.x0 - items[i + k - 1].x1 > 0.2 * b.size) {
                            word.push(' ');
                        }
                        word.push_str(&b.sym.text);
                    }
                }
                if let Some(f) = symbols::function_name(&word) {
                    w.push(&format!("\\{f}"));
                } else {
                    w.push(&format!("\\text{{{}}}", symbols::text_escape(&word)));
                    w.after_word = false;
                }
                i = j;
                continue;
            }
        }
        emit(&items[i], w);
        i += 1;
    }
}
