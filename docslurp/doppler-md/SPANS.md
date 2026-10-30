# Conservative HTML ruled spans

HTML output can retain empty geometric rows and columns and emit rectangular
rowspan/colspan cells. GFM continues to use the existing text grid unchanged.
The optional HTML representation reads original rules, including partial rules
discarded by legacy row/column inference; it never infers merges from empty text.

Only existing ruled-table candidates qualify. Closed outer borders are mandatory.
Each atomic edge must be absent or completely covered (0.5 point tolerance);
partially covered edges, nonrectangular connected cells, internal walls, and
explicitly truncated rule streams reject the candidate. Stroke thickness up to
one point is accepted. Shaded rectangles are not boundary evidence.
Limits: 2048 page rules, 64 rows/columns, 1024 atomic cells.
Text is assigned once by word midpoint, in baseline order; bidi line handling
and HTML escaping are retained. External detected headers currently use the
legacy HTML renderer, preserving header semantics but disabling spans.

The reproducible synthetic source is tables.rs::span_tests::fixture: coordinates
0,100,200,300 on both axes, upper-left 2x2 merged, five remaining unit cells.
Expected topology is six cells, including one rowspan=2 colspan=2. Tests also
reject broken/missing borders, an internal wall, truncated evidence and rule limits.

Limitations: candidate discovery still uses legacy full-width-rule grouping and
text plausibility; sparse tables may therefore be missed. A completely missing
internal edge is indistinguishable from an intentional merge when the remaining
topology is rectangular. No claim of extraction-quality improvement is made;
real-PDF synthetic conversion coverage and DocSlurp comparison remain follow-up.
