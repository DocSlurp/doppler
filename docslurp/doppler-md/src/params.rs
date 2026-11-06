//! Tuning parameters (`PARAMS` in doppler_md.py), overridable from a JSON object.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseBlocks {
    Auto,
    On,
    /// any other value, as Python's `mode == 'on' or mode == 'auto' and ...` reads it
    Off,
}

macro_rules! params {
    ($( $(#[doc = $doc:literal])* $name:ident : $ty:tt = $def:expr ),* $(,)?) => {
        /// Converter parameters. Defaults match `PARAMS` in doppler_md.py.
        #[derive(Clone, Debug)]
        pub struct Params {
            $( $(#[doc = $doc])* pub $name: params!(@ty $ty), )*
            pub use_blocks: UseBlocks,
        }

        impl Default for Params {
            fn default() -> Self {
                Params { $( $name: $def, )* use_blocks: UseBlocks::Auto }
            }
        }

        impl Params {
            /// Apply `{**PARAMS, **overrides}` from a JSON object. Unknown keys are ignored, as
            /// they are in Python; values of the wrong type are an error.
            pub fn apply_json(&mut self, json: &str) -> Result<(), String> {
                let v: Value = serde_json::from_str(json).map_err(|e| format!("--params: {e}"))?;
                let obj = v.as_object().ok_or("--params must be a JSON object")?;
                for (k, v) in obj {
                    match k.as_str() {
                        $( stringify!($name) => self.$name = params!(@get $ty, k, v)?, )*
                        "use_blocks" => {
                            self.use_blocks = match v.as_str() {
                                Some("auto") => UseBlocks::Auto,
                                Some("on") => UseBlocks::On,
                                _ => UseBlocks::Off,
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
        }
    };
    (@ty num) => { f64 };
    (@ty flag) => { bool };
    (@get num, $k:expr, $v:expr) => { number($k, $v) };
    (@get flag, $k:expr, $v:expr) => { truthy($k, $v) };
}

fn number(k: &str, v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => n.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k}: not a finite number")),
        Value::Bool(b) => Ok(*b as u8 as f64),
        _ => Err(format!("{k}: expected a number")),
    }
}

/// Python truthiness of a JSON value.
fn truthy(_k: &str, v: &Value) -> Result<bool, String> {
    Ok(match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    })
}

params! {
    /// fraction of page height treated as header/footer band
    furniture_band: num = 0.09,
    /// share of pages a band line must repeat on
    furniture_min_share: num = 0.4,
    /// keep prominent top-band headings repeated only after digit normalization
    furniture_preserve_headings: flag = true,
    /// size / body size for a heading
    heading_ratio: num = 1.15,
    h1_ratio: num = 1.6,
    h2_ratio: num = 1.25,
    heading_max_chars: num = 150.0,
    bold_headings: flag = true,
    /// slightly larger than body text: a minor heading
    minor_heading_ratio: num = 1.05,
    minor_heading_chars: num = 90.0,
    /// a minor heading must introduce a paragraph at least this long (or a list)
    minor_heading_body: num = 60.0,
    /// a short line in a non-body colour is a minor heading
    color_headings: flag = true,
    /// size change / size that breaks a paragraph
    style_size: num = 0.05,
    color_breaks: flag = true,
    /// minor headings next to a table become *captions*
    captions: flag = true,
    /// a short line on top of a data-first table becomes its header row
    table_titles: flag = true,
    title_max_chars: num = 60.0,
    title_max_words: num = 8.0,
    /// max gap between title and table / title font size
    title_gap: num = 1.6,
    /// vertical gap / median line pitch that starts a paragraph
    para_gap: num = 1.45,
    /// line ending before this share of block width ends a paragraph
    short_line: num = 0.8,
    dehyphenate: flag = true,
    tables: flag = true,
    /// horizontal gap / font size that splits a line into cells
    cell_gap: num = 0.9,
    /// rows with >= 2 cells
    table_min_rows: num = 3.0,
    /// a row with >= 2 cells of this many words is two-column prose
    long_cell_words: num = 5.0,
    /// baseline tolerance / font size for one visual row
    row_tol: num = 0.45,
    /// max baseline gap / font size inside a table
    table_row_gap: num = 2.2,
    /// share of table rows allowed to cross a column gutter
    gutter_tol: num = 0.1,
    /// a sparser row closer than this share of the row pitch continues the row above
    wrap_pitch: num = 0.75,
    /// narrower gaps count when word edges align on them across rows
    narrow_gutter: num = 0.3,
    narrow_align: num = 0.8,
    /// minimum column gutter width / font size
    min_gutter: num = 0.5,
    /// a two-column table needs this share of numeric cells in one column
    two_col_numeric: num = 0.5,
    list_value_words: num = 8.0,
    kv_min_rows: num = 3.0,
    kv_key_chars: num = 24.0,
    kv_value_words: num = 8.0,
    /// minimum page column gutter / body size
    column_gutter: num = 1.0,
    /// bands with text on both sides of a gutter before it is a column split
    column_min_bands: num = 2.0,
    /// median words per line on both sides of a table gutter
    table_cell_words: num = 4.0,
    /// share of right-side lines sharing a baseline with the left side
    table_align: num = 0.7,
    /// second gutter-table pass over the rows the dense pass left (typically rows more than
    /// `table_row_gap` apart; there is no lower bound): a run of at least `sparse_min_rows`
    /// consecutive multi-cell rows becomes a table only whole (see `tables::sparse_tables`)
    sparse_tables: flag = true,
    sparse_min_rows: num = 3.0,
    /// max baseline gap / font size between two rows of such a run
    sparse_row_gap: num = 5.0,
    /// max ratio between neighbouring row gaps in a run; the first (header) gap may differ by
    /// twice this
    sparse_pitch_ratio: num = 1.35,
    /// read the lines of one rotation (a table printed sideways) in their own reading frame
    rotated_frames: flag = true,
    /// fewer kept lines of a rotation than this stay in the upright layout
    rotated_min_lines: num = 3.0,
    /// other lines (share of the rotated lines, at least one) allowed inside a rotated region;
    /// they stay in the upright layout
    rotated_stray_share: num = 0.1,
    /// use doppler -rules table borders when present
    rules: flag = true,
    /// horizontal rule length / body size
    rule_min_len: num = 3.0,
    /// lighter rules (white separators) are ignored
    rule_max_lum: num = 0.97,
    /// max gap between consecutive rules of one table / body size
    rule_row_gap: num = 8.0,
    /// rule bands are rows when there are this many per visual row
    band_rows: num = 0.6,
    /// a rule gap this many times the group's median splits two tables
    rule_split: num = 2.5,
    /// pages with more lines or rules than this skip layout and table analysis and are read
    /// as plain lines in poppler order (the analysis is superlinear on hostile pages)
    max_page_lines: num = 20000.0,
    max_page_rules: num = 20000.0,
    /// layout work units (elements visited, see `layout::Budget`) one page may use before it
    /// falls back to plain lines
    max_page_work: num = 1.0e7,
    /// lines plus layout work units over the whole document; once spent, the remaining pages
    /// fall back to plain lines
    max_doc_work: num = 5.0e7,
}
