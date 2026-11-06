//! Glyph identity for math: font roles, TeX glyph names, legacy encodings, Unicode math
//! alphanumerics, and the LaTeX spelling of each symbol.
//!
//! The tables are written for this module from the Unicode character database (names and
//! the mathematical alphanumeric layout), the Adobe Glyph List naming scheme (BSD), the
//! Adobe Symbol encoding and the TeX font encodings (OML/OMS/OMX glyph names as the fonts
//! carry them). Nothing here is copied from another converter's tables.

use std::borrow::Cow;

/// What a font tells about the glyphs drawn with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontRole {
    /// prose text font (also used by TeX for upright math digits and operators)
    Text,
    /// TeX math italic (CMMI and look-alikes): letters are italic variables
    MathItalic,
    /// TeX math symbols (CMSY and look-alikes): capitals are calligraphic
    MathSymbols,
    /// TeX math extension (CMEX and look-alikes): big operators and delimiters
    MathExtension,
    /// AMS symbol fonts; capitals of MSBM are blackboard bold
    AmsA,
    AmsB,
    /// Euler fraktur and script
    Fraktur,
    Script,
    /// Euler roman (upright math letters)
    EulerRoman,
    /// an OpenType or Unicode math font (Latin Modern Math, STIX, Cambria Math, ...)
    UnicodeMath,
    /// Adobe Symbol or a font in its encoding
    Symbol,
    /// other symbol or math fonts with no known encoding (MT Extra, MathematicalPi, ...)
    OtherMath,
}

impl FontRole {
    /// Every glyph in such a font is evidence of math.
    pub fn is_math(self) -> bool {
        !matches!(self, FontRole::Text)
    }
}

/// The role of a font from its base name (subset tag already removed by the producer).
pub fn font_role(name: &str) -> FontRole {
    let lower = name.to_ascii_lowercase();
    let n = lower.as_str();
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| n.starts_with(p));
    if starts(&[
        "cmmi",
        "lmmathitalic",
        "ntxmi",
        "newtxmi",
        "txmi",
        "pxmi",
        "zxmi",
        "rtxmi",
        "libertinemathmi",
        "mdbchmi",
        "mdputmi",
        "mdugmmi",
        "mdsymi",
    ]) || n.contains("mathitalic")
    {
        return FontRole::MathItalic;
    }
    if starts(&[
        "cmsy",
        "cmbsy",
        "lmmathsymbols",
        "ntxsy",
        "newtxsy",
        "txsy",
        "pxsy",
        "zxsy",
        "rtxsy",
        "libertinemathsy",
        "msym",
    ]) && !starts(&["txsya", "txsyb", "ntxsya", "ntxsyb", "pxsya", "pxsyb"])
    {
        return FontRole::MathSymbols;
    }
    if starts(&[
        "cmex",
        "lmmathextension",
        "ntxex",
        "newtxex",
        "txex",
        "pxex",
        "zxex",
        "rtxex",
        "libertinemathex",
        "esint",
        "mathex",
    ]) {
        return FontRole::MathExtension;
    }
    if starts(&["msam", "txsya", "ntxsya", "pxsya"]) {
        return FontRole::AmsA;
    }
    if starts(&["msbm", "txsyb", "ntxsyb", "pxsyb", "bbold", "dsrom", "bbm"]) {
        return FontRole::AmsB;
    }
    if starts(&["eufm", "eufb", "yfrak"]) {
        return FontRole::Fraktur;
    }
    if starts(&["eusm", "eusb", "rsfs", "eus"]) {
        return FontRole::Script;
    }
    if starts(&["eurm", "eurb"]) {
        return FontRole::EulerRoman;
    }
    if n.contains("math") && !n.contains("mathematicalpi") && !n.contains("mathjax_main") && !n.contains("mathjax-main")
    {
        return FontRole::UnicodeMath;
    }
    if starts(&[
        "stixgeneral",
        "stixsize",
        "stixintegrals",
        "stixvariants",
        "stixnonunicode",
        "opensymbol",
        "asanamath",
        "xits",
        "cambria-math",
    ]) {
        return FontRole::UnicodeMath;
    }
    if starts(&["symbolmt", "symbol", "standardsymbolsps", "standardsyml", "symbolps"]) {
        return FontRole::Symbol;
    }
    if starts(&[
        "mtextra",
        "mt-extra",
        "mathematicalpi",
        "euclid",
        "mtsymbol",
        "mtmi",
        "mtsy",
        "mnsymbol",
        "fdsymbol",
        "advp",
        "wasy",
        "stmary",
    ]) {
        return FontRole::OtherMath;
    }
    FontRole::Text
}

/// Is a font name bold or italic (TeX text-font names included: CMBX, CMTI, CMSL)?
pub fn font_bold_italic(name: &str) -> (bool, bool) {
    let n = name.to_ascii_lowercase();
    let tex_bold = n.starts_with("cmbx")
        || n.starts_with("cmb10")
        || n.starts_with("lmroman") && n.contains("dunhill")
        || n.starts_with("cmbsy")
        || n.starts_with("cmmib");
    let tex_italic = n.starts_with("cmti") || n.starts_with("cmsl") || n.starts_with("cmbxti") || n.starts_with("cmmi");
    let bold = tex_bold
        || n.contains("bold")
        || n.contains("-bd")
        || n.ends_with(",bold")
        || n.contains("black")
        || n.contains("heavy")
        || n.contains("semibold");
    let italic = tex_italic
        || n.contains("italic")
        || n.contains("oblique")
        || n.contains("-it")
        || n.ends_with(",italic")
        || n.ends_with("-i");
    (bold, italic)
}

/// Math class of a symbol, for spacing and structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// letters, digits, ordinary symbols
    Ord,
    /// binary operators
    Bin,
    /// relations
    Rel,
    Open,
    Close,
    /// a fence that is open or close by position (| and ‖)
    Fence,
    Punct,
    /// big operators (sum, product, integral, ...)
    LargeOp,
    /// accent marks drawn over or under a base
    Accent,
    /// the radical sign
    Radical,
    /// a piece of an extensible delimiter (top, middle, bottom, extender)
    Piece,
}

/// Which piece of an extensible delimiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Top,
    Middle,
    Bottom,
    Extender,
}

/// A normalised symbol.
#[derive(Clone, Debug, PartialEq)]
pub struct Sym {
    /// the canonical character(s): plain letters for styled letters, the base delimiter for
    /// size variants and pieces
    pub text: String,
    pub class: Class,
    /// letter style from the font or the code point
    pub style: Style,
    /// a size variant or display form (CMEX big/Big/bigg/Bigg/display, size fonts)
    pub big: bool,
    pub piece: Option<PieceKind>,
    /// the symbol is certain: not U+FFFD, a control code, or an unresolved private-use value
    pub known: bool,
}

/// Letter style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// what LaTeX gives by default (italic letters, upright digits)
    Default,
    Upright,
    Bold,
    BoldItalic,
    DoubleStruck,
    Calligraphic,
    BoldCalligraphic,
    Fraktur,
    BoldFraktur,
    SansSerif,
    SansBold,
    SansItalic,
    SansBoldItalic,
    Monospace,
}

impl Style {
    /// The LaTeX command for a letter in this style, if any.
    pub fn command(self) -> Option<&'static str> {
        Some(match self {
            Style::Default => return None,
            Style::Upright => "\\mathrm",
            Style::Bold => "\\mathbf",
            Style::BoldItalic => "\\boldsymbol",
            Style::DoubleStruck => "\\mathbb",
            Style::Calligraphic => "\\mathcal",
            Style::BoldCalligraphic => "\\mathcal",
            Style::Fraktur => "\\mathfrak",
            Style::BoldFraktur => "\\mathfrak",
            Style::SansSerif => "\\mathsf",
            Style::SansBold => "\\mathsf",
            Style::SansItalic => "\\mathsf",
            Style::SansBoldItalic => "\\mathsf",
            Style::Monospace => "\\mathtt",
        })
    }
}

fn sym(text: impl Into<String>, class: Class) -> Sym {
    Sym { text: text.into(), class, style: Style::Default, big: false, piece: None, known: true }
}

/// Size and piece suffixes of TeX extension glyph names, longest first.
const SIZE_SUFFIXES: [&str; 9] = ["display", "widest", "wider", "Bigg", "bigg", "text", "wide", "Big", "big"];
const PIECE_SUFFIXES: [(&str, PieceKind); 6] = [
    ("vertex", PieceKind::Extender),
    ("mid", PieceKind::Middle),
    ("tp", PieceKind::Top),
    ("bt", PieceKind::Bottom),
    ("ex", PieceKind::Extender),
    ("vert", PieceKind::Extender),
];

/// The base character of a TeX extension or symbol glyph name stem.
fn tex_stem(stem: &str) -> Option<(&'static str, Class)> {
    Some(match stem {
        "parenleft" => ("(", Class::Open),
        "parenright" => (")", Class::Close),
        "bracketleft" => ("[", Class::Open),
        "bracketright" => ("]", Class::Close),
        "braceleft" => ("{", Class::Open),
        "braceright" => ("}", Class::Close),
        "floorleft" => ("⌊", Class::Open),
        "floorright" => ("⌋", Class::Close),
        "ceilingleft" => ("⌈", Class::Open),
        "ceilingright" => ("⌉", Class::Close),
        "angbracketleft" | "angleleft" => ("⟨", Class::Open),
        "angbracketright" | "angleright" => ("⟩", Class::Close),
        "slash" => ("/", Class::Ord),
        "backslash" => ("\\", Class::Ord),
        "vextendsingle" | "arrowvert" | "bar" | "divides" => ("|", Class::Fence),
        "vextenddouble" | "arrowdblvert" | "bardbl" | "parallel" => ("‖", Class::Fence),
        "radical" => ("√", Class::Radical),
        "summation" => ("∑", Class::LargeOp),
        "product" => ("∏", Class::LargeOp),
        "coproduct" => ("∐", Class::LargeOp),
        "integral" => ("∫", Class::LargeOp),
        "contintegral" => ("∮", Class::LargeOp),
        "union" => ("⋃", Class::LargeOp),
        "intersection" => ("⋂", Class::LargeOp),
        "unionmulti" => ("⨄", Class::LargeOp),
        "unionsq" => ("⨆", Class::LargeOp),
        "logicaland" => ("⋀", Class::LargeOp),
        "logicalor" => ("⋁", Class::LargeOp),
        "circledot" => ("⨀", Class::LargeOp),
        "circleplus" => ("⨁", Class::LargeOp),
        "circlemultiply" => ("⨂", Class::LargeOp),
        "hat" => ("\u{302}", Class::Accent),
        "tilde" => ("\u{303}", Class::Accent),
        "brace" => ("⏟", Class::Accent),
        "arrow" => ("|", Class::Fence),
        "arrowdbl" => ("‖", Class::Fence),
        _ => return None,
    })
}

/// TeX glyph names that the Adobe Glyph List does not cover (or covers with a different
/// character than the math fonts mean), and the symbol each one draws.
fn tex_name(name: &str) -> Option<Sym> {
    // pieces: parenlefttp, braceleftmid, bracketrightex, arrowvertex, radicalvertex, ...
    for (suffix, kind) in PIECE_SUFFIXES {
        if let Some(stem) = name.strip_suffix(suffix)
            && let Some((text, class)) = tex_stem(stem).or_else(|| (stem == "brace").then_some(("{", Class::Open)))
        {
            let mut s = sym(text, if class == Class::Radical { Class::Radical } else { Class::Piece });
            s.piece = Some(kind);
            s.big = true;
            // a brace extender belongs to either side; keep the stem's character
            if stem == "brace" && kind == PieceKind::Extender {
                s.text = "{".into();
            }
            return Some(s);
        }
    }
    for suffix in SIZE_SUFFIXES {
        if let Some(stem) = name.strip_suffix(suffix)
            && let Some((text, class)) = tex_stem(stem)
        {
            let mut s = sym(text, class);
            s.big = suffix != "text";
            return Some(s);
        }
    }
    // extensible bars: vextendsingle, vextenddouble
    if let Some(rest) = name.strip_prefix("vextend") {
        let mut s = sym(if rest == "double" { "‖" } else { "|" }, Class::Piece);
        s.piece = Some(PieceKind::Extender);
        s.big = true;
        return Some(s);
    }
    // accents named by txfonts/newtx (dotacc, ddotacc, hatacc, ...) and vec
    if let Some(stem) = name.strip_suffix("acc") {
        let c = match stem {
            "dot" => '\u{307}',
            "ddot" | "dieresis" => '\u{308}',
            "hat" | "circumflex" => '\u{302}',
            "bar" | "macron" => '\u{304}',
            "tilde" => '\u{303}',
            "acute" => '\u{301}',
            "grave" => '\u{300}',
            "breve" => '\u{306}',
            "check" | "caron" => '\u{30c}',
            "ring" => '\u{30a}',
            _ => '\u{302}',
        };
        return Some(sym(c.to_string(), Class::Accent));
    }
    if name == "vec" || name == "vecaccent" {
        return Some(sym("\u{20d7}", Class::Accent));
    }
    // brace tips of over/underbraces
    if name.starts_with("bracehtip") {
        let mut s = sym("⏟", Class::Accent);
        s.big = true;
        return Some(s);
    }
    let (text, class) = match name {
        // CMSY / OMS
        "minus" => ("−", Class::Bin),
        "periodcentered" | "dotmath" => ("⋅", Class::Bin),
        "multiply" => ("×", Class::Bin),
        "asteriskmath" => ("∗", Class::Bin),
        "divide" => ("÷", Class::Bin),
        "diamondmath" => ("⋄", Class::Bin),
        "plusminus" => ("±", Class::Bin),
        "minusplus" => ("∓", Class::Bin),
        "circleplus" => ("⊕", Class::Bin),
        "circleminus" => ("⊖", Class::Bin),
        "circlemultiply" => ("⊗", Class::Bin),
        "circledivide" => ("⊘", Class::Bin),
        "circledot" => ("⊙", Class::Bin),
        "circlecopyrt" | "openbullet" | "circle" => ("∘", Class::Bin),
        "bullet" => ("∙", Class::Bin),
        "equivasymptotic" => ("≍", Class::Rel),
        "equivalence" => ("≡", Class::Rel),
        "reflexsubset" => ("⊆", Class::Rel),
        "reflexsuperset" => ("⊇", Class::Rel),
        "lessequal" => ("≤", Class::Rel),
        "greaterequal" => ("≥", Class::Rel),
        "precedesequal" => ("⪯", Class::Rel),
        "followsequal" => ("⪰", Class::Rel),
        "similar" => ("∼", Class::Rel),
        "approxequal" => ("≈", Class::Rel),
        "propersubset" => ("⊂", Class::Rel),
        "propersuperset" => ("⊃", Class::Rel),
        "lessmuch" => ("≪", Class::Rel),
        "greatermuch" => ("≫", Class::Rel),
        "precedes" => ("≺", Class::Rel),
        "follows" => ("≻", Class::Rel),
        "arrowleft" => ("←", Class::Rel),
        "arrowright" => ("→", Class::Rel),
        "arrowup" => ("↑", Class::Rel),
        "arrowdown" => ("↓", Class::Rel),
        "arrowboth" => ("↔", Class::Rel),
        "arrownortheast" => ("↗", Class::Rel),
        "arrowsoutheast" => ("↘", Class::Rel),
        "arrownorthwest" => ("↖", Class::Rel),
        "arrowsouthwest" => ("↙", Class::Rel),
        "similarequal" => ("≃", Class::Rel),
        "arrowdblleft" => ("⇐", Class::Rel),
        "arrowdblright" => ("⇒", Class::Rel),
        "arrowdblup" => ("⇑", Class::Rel),
        "arrowdbldown" => ("⇓", Class::Rel),
        "arrowdblboth" => ("⇔", Class::Rel),
        "arrowbothv" => ("↕", Class::Rel),
        "arrowdblbothv" => ("⇕", Class::Rel),
        "proportional" => ("∝", Class::Rel),
        "prime" => ("′", Class::Ord),
        "infinity" => ("∞", Class::Ord),
        "element" => ("∈", Class::Rel),
        "owner" => ("∋", Class::Rel),
        "triangle" => ("△", Class::Ord),
        "triangleinv" => ("▽", Class::Ord),
        "negationslash" => ("\u{338}", Class::Rel),
        "mapsto" => ("↦", Class::Rel),
        "universal" => ("∀", Class::Ord),
        "existential" => ("∃", Class::Ord),
        "logicalnot" => ("¬", Class::Ord),
        "emptyset" => ("∅", Class::Ord),
        "Rfractur" => ("ℜ", Class::Ord),
        "Ifractur" => ("ℑ", Class::Ord),
        "latticetop" => ("⊤", Class::Ord),
        "perpendicular" => ("⊥", Class::Rel),
        "aleph" => ("ℵ", Class::Ord),
        "logicaland" => ("∧", Class::Bin),
        "logicalor" => ("∨", Class::Bin),
        "union" => ("∪", Class::Bin),
        "intersection" => ("∩", Class::Bin),
        "unionmulti" => ("⊎", Class::Bin),
        "turnstileleft" => ("⊢", Class::Rel),
        "turnstileright" => ("⊣", Class::Rel),
        "wreathproduct" => ("≀", Class::Bin),
        "nabla" | "gradient" => ("∇", Class::Ord),
        "bardbl" | "dblverticalbar" => ("‖", Class::Fence),
        "bar" | "verticalbar" => ("|", Class::Fence),
        "unionsq" => ("⊔", Class::Bin),
        "intersectionsq" => ("⊓", Class::Bin),
        "subsetsqequal" => ("⊑", Class::Rel),
        "supersetsqequal" => ("⊒", Class::Rel),
        "dagger" => ("†", Class::Bin),
        "daggerdbl" => ("‡", Class::Bin),
        "partialdiff" => ("∂", Class::Ord),
        "dotlessi" => ("ı", Class::Ord),
        "dotlessj" => ("ȷ", Class::Ord),
        "weierstrass" => ("℘", Class::Ord),
        "vector" => ("\u{20d7}", Class::Accent),
        "tie" => ("\u{311}", Class::Accent),
        "slurbelow" | "slurabove" => ("⌢", Class::Rel),
        "star" => ("⋆", Class::Bin),
        "epsilon1" => ("ε", Class::Ord),
        "theta1" => ("ϑ", Class::Ord),
        "pi1" => ("ϖ", Class::Ord),
        "rho1" => ("ϱ", Class::Ord),
        "sigma1" => ("ς", Class::Ord),
        "phi1" => ("φ", Class::Ord),
        "epsilon" => ("ϵ", Class::Ord),
        "phi" => ("ϕ", Class::Ord),
        "harpoonleftbt" => ("↽", Class::Rel),
        "harpoonlefttp" => ("↼", Class::Rel),
        "harpoonrightbt" => ("⇁", Class::Rel),
        "harpoonrighttp" => ("⇀", Class::Rel),
        // accents of CMR / OT1 when drawn as separate glyphs
        "circumflex" => ("\u{302}", Class::Accent),
        "tilde" => ("\u{303}", Class::Accent),
        "macron" => ("\u{304}", Class::Accent),
        "dotaccent" => ("\u{307}", Class::Accent),
        "dieresis" => ("\u{308}", Class::Accent),
        "acute" => ("\u{301}", Class::Accent),
        "grave" => ("\u{300}", Class::Accent),
        "breve" => ("\u{306}", Class::Accent),
        "caron" => ("\u{30c}", Class::Accent),
        "ring" => ("\u{30a}", Class::Accent),
        _ => return None,
    };
    Some(sym(text, class))
}

/// The Adobe Symbol encoding (codes 0x20-0xFE) as Unicode, for Symbol fonts whose text
/// came out as raw codes or as private-use U+F0xx values.
fn symbol_encoding(code: u32) -> Option<char> {
    const LOW: &str =
        " !∀#∃%&∋()∗+,−./0123456789:;<=>?≅ΑΒΧΔΕΦΓΗΙϑΚΛΜΝΟΠΘΡΣΤΥςΩΞΨΖ[∴]⊥_\u{F8E5}αβχδεφγηιϕκλμνοπθρστυϖωξψζ{|}∼";
    const HIGH: &str =
        "€ϒ′≤⁄∞ƒ♣♦♥♠↔←↑→↓°±″≥×∝∂•÷≠≡≈…⏐⎯↵ℵℑℜ℘⊗⊕∅∩∪⊃⊇⊄⊂⊆∈∉∠∇®©™∏√⋅¬∧∨⇔⇐⇑⇒⇓◊〈®©™∑⎛⎜⎝⎡⎢⎣⎧⎨⎩⎪\u{F8FF}〉∫⌠⎮⌡⎞⎟⎠⎤⎥⎦⎫⎬⎭";
    match code {
        0x20..=0x7e => LOW.chars().nth((code - 0x20) as usize),
        0xa0..=0xfe => HIGH.chars().nth((code - 0xa0) as usize),
        _ => None,
    }
}

/// Adobe's private-use code points for delimiter pieces and their Unicode pieces.
fn adobe_pua_piece(c: char) -> Option<char> {
    Some(match c as u32 {
        0xF8EB => '⎛',
        0xF8EC => '⎜',
        0xF8ED => '⎝',
        0xF8EE => '⎡',
        0xF8EF => '⎢',
        0xF8F0 => '⎣',
        0xF8F1 => '⎧',
        0xF8F2 => '⎨',
        0xF8F3 => '⎩',
        0xF8F4 => '⎪',
        0xF8F6 => '⎞',
        0xF8F7 => '⎟',
        0xF8F8 => '⎠',
        0xF8F9 => '⎤',
        0xF8FA => '⎥',
        0xF8FB => '⎦',
        0xF8FC => '⎫',
        0xF8FD => '⎬',
        0xF8FE => '⎭',
        0xF8F5 => '⌠',
        0xF8E5 => '⎯',
        0xF8E6 => '⏐',
        0xF8E7 => '⎯',
        _ => return None,
    })
}

/// Unicode delimiter and integral pieces (U+239B-U+23B3, U+23D0, U+2320/U+2321).
fn unicode_piece(c: char) -> Option<(&'static str, PieceKind, Class)> {
    use PieceKind::*;
    Some(match c {
        '⎛' => ("(", Top, Class::Open),
        '⎜' => ("(", Extender, Class::Open),
        '⎝' => ("(", Bottom, Class::Open),
        '⎞' => (")", Top, Class::Close),
        '⎟' => (")", Extender, Class::Close),
        '⎠' => (")", Bottom, Class::Close),
        '⎡' => ("[", Top, Class::Open),
        '⎢' => ("[", Extender, Class::Open),
        '⎣' => ("[", Bottom, Class::Open),
        '⎤' => ("]", Top, Class::Close),
        '⎥' => ("]", Extender, Class::Close),
        '⎦' => ("]", Bottom, Class::Close),
        '⎧' => ("{", Top, Class::Open),
        '⎨' => ("{", Middle, Class::Open),
        '⎩' => ("{", Bottom, Class::Open),
        '⎪' => ("{", Extender, Class::Open),
        '⎫' => ("}", Top, Class::Close),
        '⎬' => ("}", Middle, Class::Close),
        '⎭' => ("}", Bottom, Class::Close),
        '⎰' => ("{", Top, Class::Open),
        '⎱' => ("}", Top, Class::Close),
        '⌠' => ("∫", Top, Class::LargeOp),
        '⎮' => ("∫", Extender, Class::LargeOp),
        '⌡' => ("∫", Bottom, Class::LargeOp),
        '⏐' => ("|", Extender, Class::Fence),
        '⎷' => ("√", Bottom, Class::Radical),
        _ => return None,
    })
}

/// Class of a Unicode character in math.
pub fn char_class(c: char) -> Class {
    match c {
        '+' | '±' | '∓' | '×' | '÷' | '·' | '⋅' | '∘' | '∙' | '∗' | '⋆' | '⊕' | '⊖' | '⊗' | '⊘' | '⊙' | '∧' | '∨'
        | '∩' | '∪' | '⊎' | '⊓' | '⊔' | '∖' | '≀' | '⋄' | '−' | '†' | '‡' | '⊞' | '⊟' | '⊠' | '⊡' | '⋉' | '⋊' | '⋋'
        | '⋌' | '⊲' | '⊳' | '⊴' | '⊵' | '⋒' | '⋓' | '∔' | '⊻' | '⊼' | '⊽' | '⨿' | '⨯' => {
            Class::Bin
        }
        '=' | '<' | '>' | '≤' | '≥' | '≠' | '≡' | '≈' | '∼' | '≃' | '≅' | '∝' | '≪' | '≫' | '≺' | '≻' | '⪯' | '⪰'
        | '⊂' | '⊃' | '⊆' | '⊇' | '⊊' | '⊋' | '∈' | '∉' | '∋' | '⊢' | '⊣' | '⊨' | '∣' | '∤' | '⊥' | '≐' | '≍' | '⋈'
        | '⊏' | '⊐' | '⊑' | '⊒' | '≔' | '≦' | '≧' | '⩽' | '⩾' | '≲' | '≳' | '≶' | '≷' | '≜' | '≝' | '≟' | '∥' | '∦'
        | '≁' | '≉' | '≢' | '≮' | '≯' | '≰' | '≱' | '⊄' | '⊅' | '⊈' | '⊉' | '∌' | '⊩' | '⊪' | '⊫' | '⊮' | '⊯' | '∽'
        | '⋍' | '≊' | '≋' | '≌' | '≎' | '≏' | '≑' | '≒' | '≓' | '≖' | '≗' | '≙' | '≚' | '≛' | '⋐' | '⋑' | '⋖' | '⋗'
        | '⋘' | '⋙' | '⋚' | '⋛' | '⋞' | '⋟' | '⋦' | '⋧' | '⋨' | '⋩' | '⊰' | '⊱' | '⫅' | '⫆' | '⪅' | '⪆' | '⪇' | '⪈'
        | '⪉' | '⪊' | '⪕' | '⪖' | '⪵' | '⪶' | '⪷' | '⪸' | '⪹' | '⪺' | '∺' | '∻' | '≬' | '⋔' | '⫋' | '⫌' => {
            Class::Rel
        }
        '\u{2190}'..='\u{21ff}' | '\u{27f0}'..='\u{27ff}' | '\u{2900}'..='\u{297f}' => Class::Rel,
        '(' | '[' | '{' | '⟨' | '⌊' | '⌈' | '⟦' | '⟮' | '〈' | '⦃' | '⦇' | '⦉' => Class::Open,
        ')' | ']' | '}' | '⟩' | '⌋' | '⌉' | '⟧' | '⟯' | '〉' | '⦄' | '⦈' | '⦊' => Class::Close,
        '|' | '‖' => Class::Fence,
        ',' | ';' | ':' | '.' | '!' | '?' => Class::Punct,
        '∑' | '∏' | '∐' | '∫' | '∬' | '∭' | '∮' | '∯' | '∰' | '∱' | '∲' | '∳' | '⋃' | '⋂' | '⋁' | '⋀' | '⨁' | '⨂'
        | '⨀' | '⨄' | '⨆' | '⨅' | '⨌' | '⨍' | '⨎' | '⨏' | '⨐' | '⨑' | '⨒' | '⨓' | '⨔' | '⨕' | '⨖' | '⨗' | '⨘' | '⨙'
        | '⨚' | '⨛' | '⨜' => Class::LargeOp,
        '√' | '∛' | '∜' => Class::Radical,
        '\u{300}'..='\u{36f}'
        | '\u{20d0}'..='\u{20ff}'
        | 'ˆ'
        | '˜'
        | '¯'
        | '˙'
        | '¨'
        | '´'
        | '`'
        | '˘'
        | 'ˇ'
        | '˚'
        | '⏞'
        | '⏟'
        | '⏜'
        | '⏝'
        | '⏠'
        | '⏡'
        | '⎴'
        | '⎵' => Class::Accent,
        _ => Class::Ord,
    }
}

/// Style of a mathematical alphanumeric symbol (U+1D400-U+1D7FF, and the letterlike holes)
/// and its plain letter or digit.
pub fn math_alnum(c: char) -> Option<(char, Style)> {
    let v = c as u32;
    // letterlike symbols that fill holes of the alphanumeric block
    let letterlike = match c {
        'ℝ' => Some(('R', Style::DoubleStruck)),
        'ℕ' => Some(('N', Style::DoubleStruck)),
        'ℤ' => Some(('Z', Style::DoubleStruck)),
        'ℚ' => Some(('Q', Style::DoubleStruck)),
        'ℂ' => Some(('C', Style::DoubleStruck)),
        'ℙ' => Some(('P', Style::DoubleStruck)),
        'ℍ' => Some(('H', Style::DoubleStruck)),
        'ℬ' => Some(('B', Style::Calligraphic)),
        'ℰ' => Some(('E', Style::Calligraphic)),
        'ℱ' => Some(('F', Style::Calligraphic)),
        'ℋ' => Some(('H', Style::Calligraphic)),
        'ℐ' => Some(('I', Style::Calligraphic)),
        'ℒ' => Some(('L', Style::Calligraphic)),
        'ℳ' => Some(('M', Style::Calligraphic)),
        'ℛ' => Some(('R', Style::Calligraphic)),
        'ℯ' => Some(('e', Style::Calligraphic)),
        'ℊ' => Some(('g', Style::Calligraphic)),
        'ℴ' => Some(('o', Style::Calligraphic)),
        'ℭ' => Some(('C', Style::Fraktur)),
        'ℌ' => Some(('H', Style::Fraktur)),
        'ℑ' => Some(('I', Style::Fraktur)),
        'ℜ' => Some(('R', Style::Fraktur)),
        'ℨ' => Some(('Z', Style::Fraktur)),
        'ℎ' => Some(('h', Style::Default)),
        'ı' => Some(('ı', Style::Default)),
        _ => None,
    };
    if letterlike.is_some() {
        return letterlike;
    }
    const LATIN: [Style; 13] = [
        Style::Bold,
        Style::Default,
        Style::BoldItalic,
        Style::Calligraphic,
        Style::BoldCalligraphic,
        Style::Fraktur,
        Style::DoubleStruck,
        Style::BoldFraktur,
        Style::SansSerif,
        Style::SansBold,
        Style::SansItalic,
        Style::SansBoldItalic,
        Style::Monospace,
    ];
    if (0x1D400..0x1D400 + 13 * 52).contains(&v) {
        let off = v - 0x1D400;
        let style = LATIN[(off / 52) as usize];
        let k = off % 52;
        let letter = if k < 26 { (b'A' + k as u8) as char } else { (b'a' + (k - 26) as u8) as char };
        return Some((letter, style));
    }
    // dotless i and j
    if v == 0x1D6A4 {
        return Some(('ı', Style::Default));
    }
    if v == 0x1D6A5 {
        return Some(('ȷ', Style::Default));
    }
    // Greek: bold, italic, bold italic, sans bold, sans bold italic; 58 each starting at
    // capital Alpha (U+0391..U+03A9 with the theta symbol at 0x3A2, nabla, then lower case,
    // partial and the variant forms)
    const GREEK: [Style; 5] = [Style::Bold, Style::Default, Style::BoldItalic, Style::SansBold, Style::SansBoldItalic];
    if (0x1D6A8..0x1D6A8 + 5 * 58).contains(&v) {
        let off = v - 0x1D6A8;
        let style = GREEK[(off / 58) as usize];
        let k = off % 58;
        const ORDER: &str = "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡϴΣΤΥΦΧΨΩ∇αβγδεζηθικλμνξοπρςστυφχψω∂ϵϑϰϕϱϖ";
        let g = ORDER.chars().nth(k as usize)?;
        return Some((g, style));
    }
    if (0x1D7CA..=0x1D7CB).contains(&v) {
        return Some((if v == 0x1D7CA { 'Ϝ' } else { 'ϝ' }, Style::Bold));
    }
    // digits: bold, double-struck, sans, sans bold, monospace
    const DIGITS: [Style; 5] = [Style::Bold, Style::DoubleStruck, Style::SansSerif, Style::SansBold, Style::Monospace];
    if (0x1D7CE..0x1D7CE + 50).contains(&v) {
        let off = v - 0x1D7CE;
        return Some(((b'0' + (off % 10) as u8) as char, DIGITS[(off / 10) as usize]));
    }
    None
}

/// The LaTeX for one non-letter character, if it needs a command.
pub fn tex_of_char(c: char) -> Option<&'static str> {
    Some(match c {
        'α' => "\\alpha",
        'β' => "\\beta",
        'γ' => "\\gamma",
        'δ' => "\\delta",
        'ϵ' => "\\epsilon",
        'ε' => "\\varepsilon",
        'ζ' => "\\zeta",
        'η' => "\\eta",
        'θ' => "\\theta",
        'ϑ' => "\\vartheta",
        'ι' => "\\iota",
        'κ' => "\\kappa",
        'ϰ' => "\\varkappa",
        'λ' => "\\lambda",
        'μ' | 'µ' => "\\mu",
        'ν' => "\\nu",
        'ξ' => "\\xi",
        'ο' => "o",
        'π' => "\\pi",
        'ϖ' => "\\varpi",
        'ρ' => "\\rho",
        'ϱ' => "\\varrho",
        'σ' => "\\sigma",
        'ς' => "\\varsigma",
        'τ' => "\\tau",
        'υ' => "\\upsilon",
        'ϕ' => "\\phi",
        'φ' => "\\varphi",
        'χ' => "\\chi",
        'ψ' => "\\psi",
        'ω' => "\\omega",
        'Γ' => "\\Gamma",
        'Δ' => "\\Delta",
        'Θ' | 'ϴ' => "\\Theta",
        'Λ' => "\\Lambda",
        'Ξ' => "\\Xi",
        'Π' => "\\Pi",
        'Σ' => "\\Sigma",
        'Υ' | 'ϒ' => "\\Upsilon",
        'Φ' => "\\Phi",
        'Ψ' => "\\Psi",
        'Ω' | '\u{2126}' => "\\Omega",
        'Α' => "A",
        'Β' => "B",
        'Ε' => "E",
        'Ζ' => "Z",
        'Η' => "H",
        'Ι' => "I",
        'Κ' => "K",
        'Μ' => "M",
        'Ν' => "N",
        'Ο' => "O",
        'Ρ' => "P",
        'Τ' => "T",
        'Χ' => "X",
        '±' => "\\pm",
        '∓' => "\\mp",
        '×' => "\\times",
        '÷' => "\\div",
        '·' | '⋅' => "\\cdot",
        '∘' => "\\circ",
        '∙' => "\\bullet",
        '∗' => "\\ast",
        '⋆' => "\\star",
        '⊕' => "\\oplus",
        '⊖' => "\\ominus",
        '⊗' => "\\otimes",
        '⊘' => "\\oslash",
        '⊙' => "\\odot",
        '∧' => "\\wedge",
        '∨' => "\\vee",
        '∩' => "\\cap",
        '∪' => "\\cup",
        '⊎' => "\\uplus",
        '⊓' => "\\sqcap",
        '⊔' => "\\sqcup",
        '∖' => "\\setminus",
        '≀' => "\\wr",
        '⋄' => "\\diamond",
        '†' => "\\dagger",
        '‡' => "\\ddagger",
        '−' => "-",
        '≤' | '⩽' => "\\le",
        '≥' | '⩾' => "\\ge",
        '≠' => "\\ne",
        '≡' => "\\equiv",
        '≈' => "\\approx",
        '∼' => "\\sim",
        '≃' => "\\simeq",
        '≅' => "\\cong",
        '∝' => "\\propto",
        '≪' => "\\ll",
        '≫' => "\\gg",
        '≺' => "\\prec",
        '≻' => "\\succ",
        '⪯' => "\\preceq",
        '⪰' => "\\succeq",
        '⊂' => "\\subset",
        '⊃' => "\\supset",
        '⊆' => "\\subseteq",
        '⊇' => "\\supseteq",
        '⊊' => "\\subsetneq",
        '⊋' => "\\supsetneq",
        '∈' => "\\in",
        '∉' => "\\notin",
        '∋' => "\\ni",
        '⊢' => "\\vdash",
        '⊣' => "\\dashv",
        '⊨' => "\\models",
        '∣' => "\\mid",
        '∥' => "\\parallel",
        '⊥' => "\\perp",
        '≐' => "\\doteq",
        '≍' => "\\asymp",
        '⋈' => "\\bowtie",
        '⊏' => "\\sqsubset",
        '⊐' => "\\sqsupset",
        '⊑' => "\\sqsubseteq",
        '⊒' => "\\sqsupseteq",
        '≔' => "\\coloneqq",
        '≲' => "\\lesssim",
        '≳' => "\\gtrsim",
        '≦' => "\\leqq",
        '≧' => "\\geqq",
        '≜' => "\\triangleq",
        '∤' => "\\nmid",
        '→' => "\\to",
        '←' => "\\leftarrow",
        '↑' => "\\uparrow",
        '↓' => "\\downarrow",
        '↔' => "\\leftrightarrow",
        '⇒' => "\\Rightarrow",
        '⇐' => "\\Leftarrow",
        '⇔' => "\\Leftrightarrow",
        '⇑' => "\\Uparrow",
        '⇓' => "\\Downarrow",
        '↦' => "\\mapsto",
        '⟶' => "\\longrightarrow",
        '⟵' => "\\longleftarrow",
        '⟹' => "\\Longrightarrow",
        '⟸' => "\\Longleftarrow",
        '⟺' => "\\Longleftrightarrow",
        '⟷' => "\\longleftrightarrow",
        '⟼' => "\\longmapsto",
        '↪' => "\\hookrightarrow",
        '↩' => "\\hookleftarrow",
        '⇀' => "\\rightharpoonup",
        '↼' => "\\leftharpoonup",
        '⇁' => "\\rightharpoondown",
        '↽' => "\\leftharpoondown",
        '⇌' => "\\rightleftharpoons",
        '↗' => "\\nearrow",
        '↘' => "\\searrow",
        '↖' => "\\nwarrow",
        '↙' => "\\swarrow",
        '↕' => "\\updownarrow",
        '⇕' => "\\Updownarrow",
        '∞' => "\\infty",
        '∂' => "\\partial",
        '∇' => "\\nabla",
        '∀' => "\\forall",
        '∃' => "\\exists",
        '∄' => "\\nexists",
        '¬' => "\\neg",
        '∅' | '⌀' => "\\emptyset",
        'ℵ' => "\\aleph",
        'ℏ' => "\\hbar",
        'ℓ' => "\\ell",
        '℘' => "\\wp",
        '′' => "'",
        '″' => "''",
        '‴' => "'''",
        '∠' => "\\angle",
        '△' => "\\triangle",
        '▽' => "\\triangledown",
        '□' => "\\square",
        '◇' => "\\Diamond",
        '♠' => "\\spadesuit",
        '♡' | '♥' => "\\heartsuit",
        '♢' | '♦' => "\\diamondsuit",
        '♣' => "\\clubsuit",
        '⊤' => "\\top",
        '⋯' => "\\cdots",
        '…' => "\\ldots",
        '⋮' => "\\vdots",
        '⋱' => "\\ddots",
        '°' => "^{\\circ}",
        '∑' => "\\sum",
        '∏' => "\\prod",
        '∐' => "\\coprod",
        '∫' => "\\int",
        '∬' => "\\iint",
        '∭' => "\\iiint",
        '∮' => "\\oint",
        '⋃' => "\\bigcup",
        '⋂' => "\\bigcap",
        '⋁' => "\\bigvee",
        '⋀' => "\\bigwedge",
        '⨁' => "\\bigoplus",
        '⨂' => "\\bigotimes",
        '⨀' => "\\bigodot",
        '⨄' => "\\biguplus",
        '⨆' => "\\bigsqcup",
        '⟨' | '〈' => "\\langle",
        '⟩' | '〉' => "\\rangle",
        '⌊' => "\\lfloor",
        '⌋' => "\\rfloor",
        '⌈' => "\\lceil",
        '⌉' => "\\rceil",
        '‖' => "\\|",
        '{' => "\\{",
        '}' => "\\}",
        '\\' => "\\backslash",
        '#' => "\\#",
        '%' => "\\%",
        '&' => "\\&",
        '$' => "\\$",
        '_' => "\\_",
        '~' => "\\sim",
        '^' => "\\hat{}",
        'ı' => "\\imath",
        'ȷ' => "\\jmath",
        '∴' => "\\therefore",
        '∵' => "\\because",
        '⊲' => "\\triangleleft",
        '⊳' => "\\triangleright",
        '⊴' => "\\trianglelefteq",
        '⊵' => "\\trianglerighteq",
        '◦' => "\\circ",
        '•' => "\\bullet",
        '★' => "\\star",
        '\u{2061}' | '\u{2062}' | '\u{2063}' | '\u{2064}' | '\u{200b}' | '\u{2060}' => "",
        _ => return None,
    })
}

/// The LaTeX accent command for a combining or spacing accent character.
pub fn accent_command(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{302}' | 'ˆ' | '^' => "\\hat",
        '\u{303}' | '˜' | '~' => "\\tilde",
        '\u{304}' | '\u{305}' | '¯' | '‾' => "\\bar",
        '\u{307}' | '˙' => "\\dot",
        '\u{308}' | '¨' => "\\ddot",
        '\u{20db}' => "\\dddot",
        '\u{20d7}' | '\u{20d1}' | '→' => "\\vec",
        '\u{301}' | '´' => "\\acute",
        '\u{300}' | '`' => "\\grave",
        '\u{306}' | '˘' => "\\breve",
        '\u{30c}' | 'ˇ' => "\\check",
        '\u{30a}' | '˚' => "\\mathring",
        '⏞' | '⏜' | '⎴' => "\\overbrace",
        '⏟' | '⏝' | '⎵' => "\\underbrace",
        _ => return None,
    })
}

/// Upright multi-letter names that LaTeX spells as operators.
pub fn function_name(word: &str) -> Option<&'static str> {
    const NAMES: [&str; 33] = [
        "arccos", "arcsin", "arctan", "arg", "cos", "cosh", "cot", "coth", "csc", "deg", "det", "dim", "exp", "gcd",
        "hom", "inf", "ker", "lg", "lim", "liminf", "limsup", "ln", "log", "max", "min", "Pr", "sec", "sin", "sinh",
        "sup", "tan", "tanh", "mod",
    ];
    NAMES.iter().find(|n| **n == word).copied()
}

/// What a glyph is, from everything the record says: its font, glyph name, Unicode text and
/// where that text came from.
pub fn classify(role: FontRole, name: Option<&str>, text: &str, code: u32, src_is_fallback: bool) -> Sym {
    // 1. TeX glyph names (CMEX size variants and pieces, CMSY symbols) beat the text, which
    //    for these fonts is often a raw code or a private-use value
    if let Some(n) = name {
        if let Some(mut s) = tex_name(n) {
            s.style = Style::Default;
            return s;
        }
        // AGL-style uniXXXX names
        if let Some(hex) = n.strip_prefix("uni")
            && hex.len() == 4
            && let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        {
            return classify(role, None, &c.to_string(), code, false);
        }
    }
    let mut chars = text.chars();
    let first = chars.next();
    let rest_empty = chars.clone().next().is_none();
    // 2. Symbol-encoded fonts: raw codes and U+F0xx private-use values
    if role == FontRole::Symbol
        && let Some(c) = first
        && rest_empty
        && ((src_is_fallback && code < 256) || ('\u{F020}'..='\u{F0FF}').contains(&c))
    {
        let k = if ('\u{F020}'..='\u{F0FF}').contains(&c) { c as u32 - 0xF000 } else { code };
        if let Some(m) = symbol_encoding(k) {
            return classify(FontRole::UnicodeMath, None, &m.to_string(), code, false);
        }
    }
    let Some(c) = first else {
        return Sym {
            text: String::new(),
            class: Class::Ord,
            style: Style::Default,
            big: false,
            piece: None,
            known: false,
        };
    };
    // 3. size variants: TeX Live's ToUnicode adds a variation selector (U+FE00-U+FE0F) to the
    //    base character of a big delimiter or operator
    let without_vs: String = text.chars().filter(|c| !('\u{fe00}'..='\u{fe0f}').contains(c)).collect();
    let had_vs = without_vs.len() != text.len();
    if had_vs {
        let mut s = classify(role, None, &without_vs, code, false);
        s.big = true;
        return s;
    }
    // 4. delimiter pieces, Adobe's private-use pieces first
    let c = adobe_pua_piece(c).unwrap_or(c);
    if rest_empty && let Some((base, kind, class)) = unicode_piece(c) {
        let mut s = sym(base, if class == Class::Radical || class == Class::LargeOp { class } else { Class::Piece });
        s.piece = Some(kind);
        s.big = true;
        if class == Class::LargeOp && kind != PieceKind::Extender {
            // a whole integral drawn from pieces: the pieces still need merging
            s.class = Class::Piece;
        }
        return s;
    }
    let known = !(c == '\u{fffd}'
        || (c as u32) < 0x20
        || ('\u{e000}'..='\u{f8ff}').contains(&c)
        || ('\u{f0000}'..).contains(&c));
    // 5. styled letters: math alphanumerics, then the font's alphabet
    if rest_empty && let Some((plain, style)) = math_alnum(c) {
        let mut s = sym(plain.to_string(), Class::Ord);
        s.style = style;
        return s;
    }
    let mut s = sym(
        if rest_empty { c.to_string() } else { text.to_string() },
        if rest_empty { char_class(c) } else { Class::Ord },
    );
    s.known = known;
    // a caret or tilde in a math font is an accent (OpenSymbol draws hat and tilde this way)
    if rest_empty && role.is_math() && matches!(c, '^' | '\u{2dc}') {
        s.class = Class::Accent;
    }
    if rest_empty && c.is_ascii_alphabetic() {
        s.style = match role {
            FontRole::AmsB if c.is_ascii_uppercase() => Style::DoubleStruck,
            FontRole::MathSymbols if c.is_ascii_uppercase() => Style::Calligraphic,
            FontRole::Fraktur => Style::Fraktur,
            FontRole::Script => Style::Calligraphic,
            FontRole::EulerRoman => Style::Upright,
            FontRole::MathItalic => Style::Default,
            // letters of a Unicode math font that are not math alphanumerics are upright
            FontRole::UnicodeMath => Style::Upright,
            _ => Style::Default,
        };
    }
    s
}

/// Escape for `\text{...}`.
pub fn text_escape(s: &str) -> Cow<'_, str> {
    if !s.chars().any(|c| matches!(c, '\\' | '{' | '}' | '$' | '&' | '#' | '^' | '_' | '%' | '~')) {
        return Cow::Borrowed(s);
    }
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' | '}' | '$' | '&' | '#' | '_' | '%' => {
                out.push('\\');
                out.push(c);
            }
            '^' => out.push_str("\\textasciicircum{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            _ => out.push(c),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles() {
        assert_eq!(font_role("CMMI10"), FontRole::MathItalic);
        assert_eq!(font_role("CMSY7"), FontRole::MathSymbols);
        assert_eq!(font_role("CMEX10"), FontRole::MathExtension);
        assert_eq!(font_role("MSBM10"), FontRole::AmsB);
        assert_eq!(font_role("LatinModernMath-Regular"), FontRole::UnicodeMath);
        assert_eq!(font_role("CambriaMath"), FontRole::UnicodeMath);
        assert_eq!(font_role("CMR10"), FontRole::Text);
        assert_eq!(font_role("TimesNewRomanPSMT"), FontRole::Text);
        assert_eq!(font_role("SymbolMT"), FontRole::Symbol);
    }

    #[test]
    fn tex_names() {
        let s = classify(FontRole::MathExtension, Some("summationdisplay"), "X", 88, true);
        assert_eq!((s.text.as_str(), s.class, s.big), ("∑", Class::LargeOp, true));
        let s = classify(FontRole::MathExtension, Some("parenlefttp"), "\u{f8eb}", 48, false);
        assert_eq!((s.text.as_str(), s.class, s.piece), ("(", Class::Piece, Some(PieceKind::Top)));
        let s = classify(FontRole::MathExtension, Some("radicalbigg"), "r", 114, true);
        assert_eq!(s.class, Class::Radical);
        let s = classify(FontRole::MathSymbols, Some("negationslash"), "6", 54, true);
        assert_eq!(s.text, "\u{338}");
        let s = classify(FontRole::MathExtension, None, "∑\u{fe02}", 88, false);
        assert_eq!((s.text.as_str(), s.big), ("∑", true));
    }

    #[test]
    fn styles() {
        let s = classify(FontRole::AmsB, None, "R", 82, false);
        assert_eq!(s.style, Style::DoubleStruck);
        let s = classify(FontRole::UnicodeMath, None, "𝑥", 0, false);
        assert_eq!((s.text.as_str(), s.style), ("x", Style::Default));
        let s = classify(FontRole::UnicodeMath, None, "𝐯", 0, false);
        assert_eq!((s.text.as_str(), s.style), ("v", Style::Bold));
        let s = classify(FontRole::UnicodeMath, None, "𝔤", 0, false);
        assert_eq!((s.text.as_str(), s.style), ("g", Style::Fraktur));
        assert_eq!(math_alnum('𝜓'), Some(('ψ', Style::Default)));
        assert_eq!(math_alnum('𝟐'), Some(('2', Style::Bold)));
        let s = classify(FontRole::Symbol, None, "\u{f061}", 97, false);
        assert_eq!(s.text, "α");
    }

    #[test]
    fn symbol_encoding_is_complete() {
        for code in (0x20..=0x7e).chain(0xa0..=0xfe) {
            assert!(symbol_encoding(code).is_some(), "{code:x}");
        }
    }
}
