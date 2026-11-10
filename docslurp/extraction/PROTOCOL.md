# pdfregions ⇄ pdftomarkdown protocol (image and graphic extraction)

`pdftomarkdown --images-dir DIR` runs `pdftotext -images` (placement, ids) as today, converts,
then starts **one** `pdfregions` per document and sends it the regions to render. `pdfregions`
writes no files; `pdftomarkdown` decides, encodes and writes. Both run inside the
credential-free VM in production.

## Invocation

    pdfregions [-opw PW] [-upw PW] [-max-memory MiB] [-max-pixels N] [-timeout S] FILE.pdf

- Defaults: `-max-memory 2048` (address-space cap, POSIX, as pdfpagecount), `-max-pixels 16777216`
  (per crop, 4096 x 4096), `-timeout 120` (whole run, wall clock).
- Reads jobs from stdin until EOF; writes records to stdout; diagnostics are fixed strings on
  stderr, never document data.
- Exit: 0 every job answered (some may be `err`); 1 cannot open/decrypt; 2 bad usage or a
  malformed job line; 3 a limit hit (memory, timeout) — records already written stay valid;
  any signal is a failure.

## Jobs (stdin): one per line, tab-separated, ASCII

    <id>\t<page>\t<x0>\t<y0>\t<x1>\t<y1>\t<mode>\t<dpi>\n

- `id`: `p<N>.<M>` or `p<N>.g<K>` (regex `^p[0-9]{1,9}\.g?[0-9]{1,6}$`); echoed back verbatim.
- `page`: 1-based PDF page number; jobs are sorted by page (non-decreasing; else exit 2).
- `x0 y0 x1 y1`: the region in the same space as pdftotext `-bbox-layout` word coordinates
  (points, y down, page rotation applied, media box — neither tool uses the crop box).
  Finite, `x1 > x0`, `y1 > y0`; plain decimals (`-?[0-9]{1,9}(\.[0-9]{1,9})?`, no exponent).
  A region overhanging the page is first cut to the page (entirely outside: `err:empty`).
- `mode`: `raster` (Splash, RGB) or `svg` (cairo SVG of the region).
- `dpi`: 36..600 for `raster` (ignored for `svg`).
- At most 5000 jobs; more is exit 2.

## Records (stdout): per job, in job order

    <id>\t<status>\t<kind>\t<width>\t<height>\t<bytes>\n<payload of exactly <bytes> bytes>

- `status`: `ok`, or `err:<code>` with `kind` `-`, width/height 0, bytes 0. Codes:
  `pixels` (crop over -max-pixels at that dpi), `render` (renderer error),
  `empty` (region outside the page), `page` (page number invalid).
- `kind=rgb8`: `width*height*3` bytes, row-major, top row first, RGB 8-bit, composited on white.
  The crop starts at device pixel `floor(x0*dpi/72)` and is `max(1, round((x1-x0)*dpi/72))` wide
  (likewise y), kept on the page; the same pixels as `pdftoppm -r dpi -x -y -W -H` with those
  numbers. pdftomarkdown accepts the size within 1 px.
- `kind=svg`: UTF-8 SVG, width/height fields 0; the root `<svg>` has `width`/`height` in pt and
  `viewBox` equal to the (cut) region (`x0 y0 x1-x0 y1-y0`, at most 6 decimals), so the figure
  is the whole canvas. Content outside the region is clipped away (cairo culls it).
  Embedded images are `data:image/png` only (no source JPEG/JPX/JBIG2/CCITT mime data).
  Uncompacted; pdftomarkdown compacts and checks it.
- The document and page are opened once for all jobs and fonts are cached; each job then draws
  the page clipped to its region (so raster crops match pdftoppm exactly).

## Ownership

- `pdftotext`: placement and ids only (unchanged contract; plus `ref="num gen"` on `<image>`
  for an image XObject, so repeats of one object can be found).
- `pdfregions`: renders regions; no policy, no files.
- `pdftomarkdown` (Rust): what to extract (fates, min side, repeats, caps), checks records
  against the report (id, size), encodes WebP / compacts and allowlists SVG, writes DIR and
  `manifest.json`.
