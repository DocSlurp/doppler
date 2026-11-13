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

impl Params {
    /// The document work budget at the `pages`-th page of the document (1-based, counted from
    /// the PDF's first page): `max_doc_work + doc_work_per_page * pages`, at most
    /// `max_doc_work_total`.
    pub fn doc_work_limit(&self, pages: usize) -> f64 {
        let grown = self.max_doc_work + self.doc_work_per_page * pages as f64;
        if grown.is_nan() { self.max_doc_work_total } else { grown.min(self.max_doc_work_total) }
    }
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
    /// a line that is a display formula ($$...$$, from Options::math) is its own paragraph
    display_math_paragraphs: flag = false,
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
    /// borderless table rows whose cells wrap onto several lines, with padding between rows: a
    /// line stacked under a cell of a multi-cell row, at line pitch and aligned with it, is part
    /// of that row (see `tables::fold_wraps`), and a column gutter whose right side is such
    /// rows does not split the page into reading columns
    wrapped_rows: flag = true,
    /// max baseline step / font size from a cell's line to its wrapped next line
    wrap_lead: num = 1.6,
    /// the gaps between such rows must be at least this many times the wrap step (visible row
    /// padding); otherwise the rows are left to the passes below as before
    wrap_padding: num = 1.15,
    /// read the lines of one rotation (a table printed sideways) in their own reading frame
    rotated_frames: flag = true,
    /// fewer kept lines of a rotation than this stay in the upright layout
    rotated_min_lines: num = 3.0,
    /// other lines allowed inside a rotated region, as a share of its lines rounded up (any
    /// positive share allows one; 0 allows none); they stay in the upright layout
    rotated_stray_share: num = 0.1,
    /// use doppler -rules table borders when present
    rules: flag = true,
    /// horizontal rule length / body size
    rule_min_len: num = 3.0,
    /// lighter rules (white separators) are ignored
    rule_max_lum: num = 0.97,
    /// max gap between consecutive rules of one table / body size
    rule_row_gap: num = 8.0,
    /// read tables drawn as filled cell boxes (white or shaded cells between thin borders) from
    /// the boxes: each cell's lines stay in one cell (see `box_cells`)
    box_cell_tables: flag = true,
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
    /// fall back to plain lines. The budget a page sees is `max_doc_work` plus
    /// `doc_work_per_page` for every page of the document up to and including it (counted from
    /// the PDF's first page, so a range or a stream sees the same budget at a page as a whole
    /// run), at most `max_doc_work_total`
    max_doc_work: num = 5.0e7,
    /// added to the document budget per page; above the measured average (about 800 to 11,400
    /// units per page on long PDFs, see PERF-ALGO.md) and far below `max_page_work`
    doc_work_per_page: num = 2.0e4,
    /// the document budget never exceeds this, however long the document (bounds the layout
    /// CPU of a hostile document: see PERF-ALGO.md)
    max_doc_work_total: num = 2.5e8,
    /// a header printed on several stacked lines becomes one header row
    merge_header_rows: flag = true,
    /// stacked header lines are at most this many font sizes apart
    header_stack_gap: num = 1.9,
    /// and the spacing below the header, and of the body's rows, is at least this many times
    /// the largest gap inside it
    header_stack_open: num = 1.3,
    /// a paragraph that runs out of one column and goes on at the top of the next is one paragraph
    join_column_breaks: flag = true,
    /// a paragraph interrupted by a figure, its caption or a footnote goes on after it, in the
    /// same column or (with `join_column_breaks`) the next: one paragraph, the figure after it
    join_interrupted_paragraphs: flag = true,
    /// a short label in the margin beside a paragraph, read as the start of one of its lines,
    /// becomes a paragraph of its own before it
    margin_labels: flag = true,
    /// with markers, small print or footnotes under a page's last table (or above the next
    /// page's first) do not hide that the table goes on
    stitch_past_notes: flag = true,
    /// the label ends at least this many sizes before the rest of its line
    label_gap: num = 0.5,
    /// and has at most this many characters
    label_max_chars: num = 16.0,
    /// print at most this many times a paragraph's size can interrupt it (captions, footnotes)
    aside_size: num = 0.92,
    /// and print at most this many times its size is figure labelling by itself
    aside_tiny: num = 0.7,
    /// a page with more pictures than this inside its lines has text drawn as pictures: they
    /// get no markers
    image_inline_max: num = 12.0,
    /// remove watermarks: large pale or diagonal words, and large words and pictures that repeat
    /// at the same place on enough pages
    watermarks: flag = true,
    /// a watermark word is at least this many times the page's body size
    watermark_size: num = 1.5,
    /// a pale word is at least this luminance (white, the text of banners, is not pale)
    watermark_min_lum: num = 0.72,
    /// a pale word is a grey: its channels differ by at most this much (0 to 1); a light colour
    /// (a gold or light-blue heading) is not pale
    watermark_max_chroma: num = 0.12,
    /// a minor heading after a table that starts with a multi-level section number (4.6.3) and
    /// opens body text is a heading, not the table's caption
    section_headings_after_tables: flag = true,
    /// images this many body sizes apart or less are one figure and get one marker
    image_group_gap: num = 0.75,
    /// an image needs both sides at least this many points to get a marker (pictures)
    image_min_size: num = 6.0,
    /// and a stencil mask both sides at least this many (bullets and glyph bitmaps are masks)
    image_min_mask: num = 12.0,
    /// an image no taller than this many body sizes that sits in a line of text is marked
    /// inside that line instead of between paragraphs
    image_inline_height: num = 2.5,
    /// an image covering this share of the page, under text, is the page's background (a scan
    /// with a text layer) and gets no marker
    image_background_share: num = 0.85,
    /// vector drawing clusters this many body sizes apart or less are one drawing
    vector_group_gap: num = 1.0,
    /// a drawing is a figure with at least this many diagonal or curved segments
    vector_min_segments: num = 20.0,
    /// and at least this many curves, or diagonal segments in this many of 8 directions
    /// (hatching and a lone slash point one way)
    vector_min_curves: num = 24.0,
    vector_min_directions: num = 3.0,
    /// and a drawing at least this many body sizes long, a figure this many on both sides
    vector_min_extent: num = 2.5,
    /// and a drawing at least this many body sizes thick, unless the rules it touches make it
    /// `vector_min_extent` thick (dotted leaders and braces are thin; a row of arrows between
    /// the boxes of a diagram is not, with the boxes); a thin one also joins a figure it touches
    vector_min_thickness: num = 1.0,
    /// a thin drawing needs at least this many painted paths to become a figure that way
    vector_thin_min_paths: num = 6.0,
    /// and a figure covers at least this share of the page
    vector_min_share: num = 0.005,
    /// short lines (at most this many words) within this many body sizes of a figure are its
    /// labels
    vector_label_words: num = 4.0,
    vector_label_gap: num = 1.0,
    /// a figure whose words cover more than this share of it, or with more lines than
    /// `vector_max_prose_lines` of at least `vector_prose_words` words, is drawing among text
    /// (a form, a worksheet): no marker
    vector_max_text_share: num = 0.35,
    vector_max_prose_lines: num = 1.0,
    vector_prose_words: num = 8.0,
}
