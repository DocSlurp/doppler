# pdfregions ⇄ pdftomarkdown protocol (image and graphic extraction)

`pdftomarkdown --images-dir DIR` runs `pdftotext -images` (placement, ids) as today, converts,
writes the Markdown (whose links already name the files), closes stdout, then starts **one**
`pdfregions` per extraction batch (`--images-batch-pages`, default 64 consecutive pages) and
sends it that batch's regions to render, in page order. Each batch gets its own `-timeout`
(`--images-timeout`) and `-max-output-bytes` (8 x `--images-max-bytes`, 64 MiB..2 GiB), so one
document is never cut by a whole-document output cap. `pdfregions` writes no files;
`pdftomarkdown` decides, encodes and writes. Both run inside the credential-free VM in
production.

## Invocation

    pdfregions [-opw PW] [-upw PW] [-max-memory MiB] [-max-pixels N] [-timeout S]
               [-max-svg-bytes N] [-max-output-bytes N] [-no-images] [-confine require|auto|off] FILE.pdf

- Defaults: `-max-memory 2048` (address-space cap, POSIX, as pdfpagecount), `-max-pixels 16777216`
  (per crop, 4096 x 4096), `-timeout 120` (whole run, wall clock), `-max-svg-bytes 33554432`
  (per SVG record, else `err:svgsize`), `-max-output-bytes 536870912` (all of stdout; a record
  that would pass it is not written and the run exits 3), `-confine auto`.
- `-no-images`: images are not drawn (their data is never decoded), for vector-only renders.
- Confinement (Linux): after opening the PDF and loading fontconfig, pdfregions sets
  `PR_SET_NO_NEW_PRIVS`, a Landlock ruleset that denies every filesystem write, create, remove,
  rename and truncate (reads stay allowed for fonts) plus TCP and scoped unix sockets/signals,
  and a seccomp filter (execve, socket, ptrace, process_vm_*, open/openat with write or create
  flags: EPERM). It prints `pdfregions: confined=0|1 landlock=<abi> seccomp=0|1` on stderr.
  `auto` confines when the kernel allows, `require` exits 5 when it cannot, `off` skips it.
  **DocSlurp's VM should pass `-confine require`** once the guest kernel is confirmed to have
  Landlock (5.13+, `CONFIG_SECURITY_LANDLOCK`, `lsm=` including landlock) and seccomp.
- pdfregions dies with its parent (`PR_SET_PDEATHSIG SIGKILL`); the parent still owns a wall-clock
  deadline and should kill the process group.
- Reads jobs from stdin until EOF; writes records to stdout; diagnostics are fixed strings on
  stderr, never document data.
- Exit: 0 every job answered (some may be `err`); 1 cannot open/decrypt; 2 bad usage or a
  malformed job line; 3 a limit hit (memory, timeout, output bytes) — records already written
  stay valid; 4 stdout write failure; 5 `-confine require` could not confine; any signal is a
  failure. If the deadline passes while a record is being written, the record gets 2 more
  seconds; a write still blocked then is cut short and the run exits 3, so a reader must drop a
  truncated last record.

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
  `empty` (region outside the page), `page` (page number invalid), `svgsize` (SVG over
  -max-svg-bytes).
- `kind=rgb8`: `width*height*3` bytes, row-major, top row first, RGB 8-bit, composited on white.
  The crop starts at device pixel `floor(x0*dpi/72)` and is `max(1, round((x1-x0)*dpi/72))` wide
  (likewise y), kept on the page; the same pixels as `pdftoppm -r dpi -x -y -W -H` with those
  numbers. pdftomarkdown accepts the size within 1 px.
- `kind=svg`: UTF-8 SVG, width/height fields 0; the root `<svg>` has `width`/`height` in pt and
  `viewBox` equal to the (cut) region (`x0 y0 x1-x0 y1-y0`, at most 6 decimals), so the figure
  is the whole canvas. Content outside the region is clipped away (cairo culls it).
  Embedded images are `data:image/png` only (no source JPEG/JPX/JBIG2/CCITT mime data).
  Uncompacted; pdftomarkdown compacts and checks it.
- The document and page are opened once for all jobs and fonts are cached; each raster job then
  draws the page clipped to its region (so raster crops match pdftoppm exactly). The SVG jobs of
  a page (up to 16 at a time) share one pass over the page: every drawing call goes to each job's
  own cairo surface and clip. A page whose pass could differ from one pass per job is drawn one
  job at a time instead: an inline image (it cannot be read twice), devices that disagree, any
  parser message during the pass (a damaged font or stream, where cairo's glyph handling can
  depend on drawing order; the font cache is put back as it was before the pass), or memory: the
  pass may add at most min(64 MiB, `-max-memory`/16) to the resident size (`/proc/self/statm`;
  without it no pass is shared), counting the finished SVGs it keeps for later records. A job
  whose shared-pass result is not a clean SVG (`err:render`, `err:svgsize`) is drawn on its own
  as well, so every job gets the outcome it has alone. `PDFREGIONS_SVG_TOGETHER=0` always draws
  one job at a time.
- What is identical to one pass per job: every record's status and kind, raster payloads, and
  SVG payloads except cairo's `source-N` ids (a process-wide counter, so the numbers and with them
  the record length differ). pdftomarkdown renumbers ids by document order when it compacts an
  SVG, so its files and manifest are the same bytes either way. Diagnostics are not part of the
  contract: poppler's messages are not printed, and the `svg-together` stats line
  (`PDFREGIONS_SVG_STATS=1`) is for tests.

## Ownership

- `pdftotext`: placement and ids only (unchanged contract; plus `ref="num gen"` on `<image>`
  for an image XObject, so repeats of one object can be found).
- `pdfregions`: renders regions; no policy, no files.
- `pdftomarkdown` (Rust): what to extract (fates, min side, repeats, caps), checks records
  against the report (id, size), encodes WebP / compacts and allowlists SVG, writes DIR and
  `manifest.json`. It alone maps ids to paths (`page-NNNN/image-MMM.<ext>`,
  `page-NNNN/figure-KKK.<ext>`; `docslurp/doppler-md/src/extract/layout.rs`), so ids on this
  protocol never become paths here.

## Output folder (pdftomarkdown)

- DIR must not exist; it is created with `mkdirat` mode **0700**, and each page folder
  (`page-NNNN`) with `mkdirat` mode **0700** under DIR's descriptor and opened `O_NOFOLLOW`.
  Files are created **0600** (`O_CREAT|O_EXCL|O_NOFOLLOW` to a temporary name, renamed with
  `RENAME_NOREPLACE`). The modes hold whatever the umask (it can only remove bits): images can
  be tenant data in a shared VM tmp. Copy or `chmod` them to share.
- `manifest.json` (version 2) is written last, after the files are synced; every link of the
  Markdown has an entry and every file is listed. A DIR without a manifest is garbage.

## Security model (council review, 2026-11-10)

- **Decoders.** Region renders decode every image codec Poppler supports (DCT, JPX, JBIG2,
  CCITT, Flate), as page rendering already does in the same VM; there is no decoder
  allowlist. Accepted: the attack surface equals today's page rasterisation, and it is bounded
  by the VM (credential-free, network-blocked, one job per VM), `pdfregions`' own confinement
  (`-confine`: no_new_privs, Landlock write/network denial, seccomp), and its memory, pixel,
  output and time caps. `-no-images` renders without decoding images when a caller needs only
  vector drawing.
- **The SVG allowlist is a correctness filter, not the boundary.** pdfregions and pdftomarkdown
  share the VM, so a decoder compromise could rewrite an SVG after the allowlist ran. Whoever
  serves figures must keep them inert: a fixed `image/svg+xml` type, `nosniff`, and a CSP of
  `default-src 'none'; style-src 'unsafe-inline'; sandbox` (DocSlurp pins this with a test).
- **Figures never fail the Markdown.** The Markdown is written (and stdout closed) before
  extraction starts. A batch that hits a budget, times out or whose renderer fails keeps the
  files it wrote and the next batch goes on (two failed batches in a row stop the rest); the
  manifest says what is missing and why, and the exit status is 5 with the folder kept. Only a
  write failure or a renderer that cannot start fails the extraction: exit 6, folder removed.
- **Links cannot be forged.** Only the converter's sealed markers expand to image paths; text of
  the PDF that looks like `page-NNNN/image-` or `page-NNNN/figure-` (any case, any prefix) gets
  a zero-width space after `page` first.
- **Confinement in production.** Pass `--images-confine require` once the VM kernel is known to
  support Landlock and seccomp; with `auto`, the report's `images_confined` says whether it held.
