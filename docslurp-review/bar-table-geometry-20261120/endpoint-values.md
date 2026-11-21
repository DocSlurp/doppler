# Printed endpoint values for rectangular bar charts

## Candidate

Retain the existing baseline stroke, bar shape, and populated-cell checks. When the normal detector declines a run, fit a linear scale using three aligned tick labels outside the bar lanes. Test the closest numeric word near each bar tip in its own lane. At least three tips and two thirds of the run must agree with the fitted scale. Require stable bar thickness. Charge discovery and fitting against the existing detector budgets.

## Original PDF replay

The paired comparison starts from private Doppler 240b44d16. Ten public PDFs supplied 220 pages, with up to the first 32 pages per file. The comparison ran in local QEMU guests with no network device or host mounts.

- June 2024 CDC ACIP Stokely deck, PDF page 7. The baseline emitted four table blocks from two chart panels and their captions. The endpoint candidate emitted both panels as figures, and PageFacts went from zero to two figure clusters. The candidate crops cover the bars and printed values. The trailing 100.0 tick is outside each crop, and the source panel legends sit below the crops.
- The other nine PDFs yielded no new Markdown figure blocks or PageFacts figure-cluster count changes. Their HTML table blocks were byte-identical. This includes the final three-PDF replay of the February and October CDC meeting decks and the Indiana survey report.
- The June deck was an exploratory source for the geometry. The subsequent decks provide a transfer check for regressions. The sample is targeted and small.

## Original synthetic PDFs

The 37-case replay adds two gridless positives covering both orientations, an off-scale negative, and a ruled table with calibrated in-cell values. Markdown and PageFacts recovered both positives, rejected the two negatives as figures, and retained the calibrated table. The previous 33 fixtures kept their classifications.

Focused Rust integration and vector figure tests passed, alongside the native PageFacts suite. Encrypted PageFacts fixtures require the external Poppler test checkout and were skipped.

## Follow-up

Bind the accepted tick series to the exported region so a short series still includes the full labeled scale. Treat this as an extent experiment. Test a nearby second panel, ticks beyond the longest bar, and a footnote or legend outside the scale before extending figure bounds.

[June CDC ACIP source PDF](https://www.cdc.gov/acip/downloads/slides-2024-06-26-28/07-COVID-Stokely-508.pdf)
