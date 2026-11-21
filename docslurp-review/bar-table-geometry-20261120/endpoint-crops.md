# Full scales and shared legends on endpoint-detected bar charts

The endpoint fit follows the calibrated tick cadence beyond the longest bar. Tick labels of unequal width use the widest pair of corroborating ticks to limit drift. The crop also keeps the painted grid ends beside the bar lanes.

A swatch-backed legend below the axis belongs to the crop. Adjacent endpoint panels with aligned plots, identical titled scales, and exactly one legend use a shared crop. Independently labeled panels stay separate. The joint crop has two points of padding to protect glyphs at its edges. Native PageFacts continues to record each panel individually.

## Matched public-PDF replay

- 220 pages from the same ten PDFs replayed in a local VM with no network device or host mounts.
- June 2024 CDC ACIP PDF page 7 exports one two-panel figure at PDF bounds `[43.48, 74.56, 694.33, 376.99]`. Both scales reach the final `100.0` label. Both axis titles, the shared vaccine swatch and legend, and the adjacent abbreviation note are visible. The separate source footer is outside the crop.
- The actual PNG was visually checked. SVG export succeeded with the same bounds.
- All HTML table blocks and PageFacts document/page records matched endpoint baseline `4a575fabe`. The other nine PDFs' Markdown and vector reports matched. The CDC deck's other vector figures also matched.

The focused Rust suite passed 29 tests, including a regression with trailing ticks and a shared legend plus a control with independent legends. The native PageFacts suite passed with its optional encrypted-corpus check skipped. The authored-fixture integrity gate was run separately.

[Original CDC ACIP slide deck](https://www.cdc.gov/acip/downloads/slides-2024-06-26-28/07-COVID-Stokely-508.pdf)
