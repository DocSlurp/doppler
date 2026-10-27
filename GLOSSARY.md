# PDF text extraction

These terms describe the evidence extracted from a PDF and its interpretation as structured text or Markdown.

## Language

**Structured text**:
Words and their logical text, geometry, reading groups, and text styles, before their representation in a particular output format.

**Page layout**:
The assignment and reading order of a page's content as prose, lists, headings, captions, and tables.

**Content ownership**:
The assignment of source text to layout content, including the relationship between a table and its title or header. Ownership determines which text belongs in each emitted item.

**Rule evidence**:
The axis-aligned strokes and filled rectangles collected from a page to inform table and cell interpretation. Rule evidence is a selected subset of page graphics, not a complete record of everything drawn.

**Rule completeness**:
Whether all otherwise eligible rule evidence was retained within the requested collection scope. Truncation means a collection limit omitted evidence; completeness does not imply support for every drawing operation.

**Layout degradation**:
The use of plain line order when a layout limit prevents the usual interpretation of a page. This differs from upstream rule truncation, which describes missing evidence rather than a layout decision.
