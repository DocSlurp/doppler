# pdfregions ⇄ pdftomarkdown protocol (image and graphic extraction)

`pdftomarkdown --images-dir DIR` runs `pdftotext -images` (placement, ids) as today, converts,
writes the Markdown (whose links already name the files), closes stdout, then starts **one**
`pdfregions` per extraction batch (`--images-batch-pages`, default 64 consecutive pages) and
sends it that batch's regions to render, and its page scans to copy, in page order. Each batch
gets its own `-timeout` (`--images-timeout`) and `-max-output-bytes` (8 x `--images-max-bytes`
+ 4 x `--scan-max-bytes`, 64 MiB..64 GiB), so one document is never cut by a whole-document
output cap. `pdfregions` writes no files;
`pdftomarkdown` decides, encodes and writes. Both run inside the credential-free VM in
production.

## Invocation

    pdfregions [-opw PW] [-upw PW] [-max-memory MiB] [-max-pixels N] [-timeout S]
               [-max-svg-bytes N] [-max-output-bytes N] [-max-copy-bytes N] [-max-copy-pixels N]
               [-no-images] [-confine require|auto|off] FILE.pdf

- Defaults: `-max-memory 2048` (address-space cap, POSIX, as pdfpagecount), `-max-pixels 16777216`
  (per crop, 4096 x 4096), `-timeout 120` (whole run, wall clock), `-max-svg-bytes 33554432`
  (per SVG record, else `err:svgsize`), `-max-output-bytes 536870912` (all of stdout; a record
  that would pass it is not written and the run exits 3), `-max-copy-bytes 67108864` (largest
  copied JPEG, else `err:copysize`), `-max-copy-pixels 16777216` (largest image a copy job
  decodes, 8 times that for 1-bit images, else `err:pixels`; pdftomarkdown passes it), `-confine auto`.
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
    <id>\t<page>\t<x0>\t<y0>\t<x1>\t<y1>\tcopy\t<num>.<gen>\n

- `id`: `p<N>.<M>` or `p<N>.g<K>` (regex `^p[0-9]{1,9}\.g?[0-9]{1,6}$`); echoed back verbatim.
- `page`: 1-based PDF page number; jobs are sorted by page (non-decreasing; else exit 2).
- `x0 y0 x1 y1`: the region in the same space as pdftotext `-bbox-layout` word coordinates
  (points, y down, page rotation applied, media box — neither tool uses the crop box).
  Finite, `x1 > x0`, `y1 > y0`; plain decimals (`-?[0-9]{1,9}(\.[0-9]{1,9})?`, no exponent).
  A region overhanging the page is first cut to the page (entirely outside: `err:empty`).
- `mode`: `raster` (Splash, RGB), `svg` (cairo SVG of the region), or `copy` (an image
  XObject's own pixels, no rendering; the box is checked as for the others, then unused).
- `dpi`: 36..600 for `raster` (ignored for `svg`). For `copy` the last field is the image
  XObject's reference instead: `num` `[1-9][0-9]{0,8}`, `gen` 0..65535 (`[0-9]{1,5}`), as
  pdftotext `-images` writes `ref="num gen"`; anything else is exit 2.
- At most 5000 jobs; more is exit 2.

## Records (stdout): per job, in job order

    <id>\t<status>\t<kind>\t<width>\t<height>\t<bytes>\n<payload of exactly <bytes> bytes>

- `status`: `ok`, or `err:<code>` with `kind` `-`, width/height 0, bytes 0. Codes:
  `pixels` (crop over -max-pixels at that dpi; a copy over -max-copy-pixels), `render`
  (renderer error), `empty` (region outside the page), `page` (page number invalid), `svgsize`
  (SVG over -max-svg-bytes); for copy jobs `copy` (not an image XObject named in that page's
  resources, or its dictionary gives no copy class), `copysize` (a JPEG over -max-copy-bytes),
  `jpeg` (the DCT stream is not a JPEG to copy), `decode` (its samples cannot be decoded).
- `kind=rgb8`: `width*height*3` bytes, row-major, top row first, RGB 8-bit, composited on white.
  The crop starts at device pixel `floor(x0*dpi/72)` and is `max(1, round((x1-x0)*dpi/72))` wide
  (likewise y), kept on the page; the same pixels as `pdftoppm -r dpi -x -y -W -H` with those
  numbers. pdftomarkdown accepts the size within 1 px.
- Copy jobs (`copy`): the object must be an image XObject (`/Subtype /Image`) named in the
  `/XObject` resources of that page (inherited resources included), or of a form XObject named
  there, forms nested at most 4 deep, at most 10,000 resource entries searched. Only
  dictionaries are read to find it. Its dictionary is then classified again with the function
  behind pdftotext's `copy=` (`classifyImageCopy` over `Gfx::readImageDictInfo`), so a job
  cannot ask for more than the producer reported. Then the **page must pass the copy
  allow-list** (`poppler/ScanCopyAudit.h`), which pdfregions runs itself: the page's content,
  forms and annotations are run as the raster renders run them (media box, page `/Rotate`, not
  printing) through a device that draws nothing and decodes no image, and the job is answered
  only when the one thing that paints on the page is that image, drawn once (counted where Gfx
  would draw it, so an image Gfx refuses to draw, whose render is blank, never passes), shown,
  upright, plainly (no mask, soft mask, group, blend mode, opacity, transfer function or
  overprint; device colour only) and wholly inside the clip in force and the CropBox (a rounding
  slack of at most a point and less than half an image pixel on each axis); everything else on
  the page must be an operator known not to paint (graphics and text state, path construction
  ended by `n`, rectangular clips containing the image, marked content, colour, `gs` with
  harmless keys, forms without a group) or text in render mode 3 (7 after the image) in a font
  that is not Type 3. Anything else at any depth (a fill, stroke, shading, inline image, second
  image, visible text, Type 3 font, other clip, page `/Group`, an annotation that can draw an
  appearance or border in view or print mode, ...) is `err:copy`. A class also needs an image
  Gfx would draw (a boolean `/ImageMask` if any, an integer `/BitsPerComponent` in range, ...):
  otherwise it is `render`. pdftotext's `copyable="1"` is the same allow-list on its own pass, so a copy needs
  both. The page is audited before the image object is fetched, and a document whose
  cross-reference table was ever rebuilt from a scan of the file (at open, or when a fetch met
  a broken entry, the audit's included), or whose table changes during the job, is `err:copy`:
  a rebuilt table may resolve the job's reference to another object body than the one the page
  draws. Width and height in the record are the image's `/Width` and `/Height`:
  - `kind=jpeg` (class `jpeg`): the DCT filter's input (the stored bytes, decrypted), after
    checks: SOI first, one frame header SOF0, SOF1 or SOF2 (Huffman baseline, extended or
    progressive) of 8-bit samples with the image's size and as many components as its colour
    space (1 or 3), scans, and EOI within the last 66 bytes (padding after it is cut); no other
    image inside. It is rewritten without metadata: tables (DQT, DHT, DRI), the frame, the scans
    and their entropy-coded data stay; one canonical segment keeps the colour transform poppler
    decodes with: an Adobe APP14 (its transform as 0 or 1) when the stream has one, with no
    JFIF beside it (a viewer's libjpeg would let JFIF win), else a JFIF APP0 (1.01, 1:1, no
    thumbnail) when the stream has one libjpeg recognises (14 data bytes), else neither (the
    component ids decide, the same way in both); every other APPn (Exif,
    its orientation and thumbnail, ICC, XMP, Photoshop) and every COM goes, before the frame
    and between scans. Viewers then show the pixels as the PDF draws them, and no document
    string reaches the file. A stream without metadata comes back byte for byte. Nothing is
    decoded.
  - `kind=gray1` (class `bilevel`: CCITT, JBIG2 or grey with an integer `/BitsPerComponent 1`,
    or a stencil mask, no `/Decode`; never a JPEG 2000 stream, which decodes to 8 bits a
    component whatever its dictionary says, so its `/BitsPerComponent` is not read and it is
    at most class `decode`):
    the decoded samples, `ceil(width/8) * height` bytes, rows top first, most significant bit
    first, 0 black (the PDF's own sense); a short stream ends in white rows.
  - `kind=gray8` or `kind=rgb8` (class `decode`: another single image): its samples decoded once
    at native size through its colour space and `/Decode` (`gray8` for a one-component grey
    space, else `rgb8`), as rendering would read them.
  - Before a `gray1`, `gray8` or `rgb8` copy is decoded, the size the codec really decodes must
    be the image's `/Width` x `/Height`: a JPEG 2000 codestream's SIZ (raw, or in the jp2c box
    of a JP2 file; image size minus offset), a JPEG's frame header (a DCT stream decoded, not
    copied: `/Decode`, a calibrated or ICC space), a JBIG2 page information segment (an unknown,
    striped height is refused), CCITT `/Columns` and `/Rows`. Otherwise `err:pixels`. A codec
    under another filter (`[/JPXDecode /FlateDecode]`) is `err:decode`. JBIG2 symbol and region
    bitmaps, and the codecs on the render path, stay bounded by `-max-memory` only.
  - Copy jobs use the decoders page rendering uses, inside the same confinement and limits;
    pdftomarkdown sends them only for page scans whose `copy=` is `jpeg` or `bilevel` (and,
    with `--scan-pages image`, `decode`), whose page passed the allow-list (`copyable="1"`),
    that are upright (`orient="0"`), that the overpaint estimate also clears (`overlaid="0"`,
    an extra refusal) and single (not tiled or layered); with `--scan-pages image` a `decode`
    scan over 16,777,216 pixels is rendered instead.
  - With `--scan-pages copy` (the default) these copy jobs are the only work pdfregions does for
    scans: a scan that is not copyable as a `jpeg` or `bilevel` stream (`decode`, tiled,
    layered, turned, covered or overlaid, on or after the page where the cross-reference table
    was found rebuilt) gets no page-scan marker and no job at all; it is the page's
    `background`, unlinked in the manifest (`reason: scan-not-copyable`), and the host's own
    page rendering pictures that page. A scan judged copyable before a rebuild found on a later
    page keeps its page-scan marker (the links are final at conversion), but its copy is refused
    and nothing is rendered in its place: the manifest says `not-extracted`, `reason: copy`,
    `error: xref`, with no file. The host must render the page for such entries as well as for
    `scan-not-copyable` ones. On a tiled or layered scan every other image of at least 1% of
    the page is grouped with it, so under `copy` a real photo on such a page also becomes
    backdrop with no Doppler image; the host's page rendering shows it. An image the producer
    does not make a scan (a stencil mask, or a page whose image list was truncated) is placed
    as any picture and may be rendered as one. `--scan-pages image` restores the decode and
    render jobs for such scans; `--scan-pages render` renders every scan.
  - Manifest: `reason` is set on `not-extracted` entries and, with the single value
    `scan-not-copyable`, on the `unlinked` `background` entries of a scan `--scan-pages copy`
    left to the host; the header's `scan_pages` is `copy`, `image`, `skip` or `render`. Both are
    new values under the unchanged `doppler-images/3` schema: a host that checks them strictly
    (DocSlurp core does) needs the matching contract change before it reads `copy` output, and
    hosts should pass `--scan-pages` explicitly rather than rely on the default.
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
  for an image XObject, so repeats of one object can be found; `filter= bpc= cs= smask=
  decode= inline= copy=` from its dictionary, then `orient=` and `overlaid=` on every image,
  `copyable="1"` on a single scan's image whose page passed the copy allow-list; and
  `scan= scanImage=` on `<page>` for a full-page scan; `xrefRebuilt="1"` on `<page>`, and no
  `copyable="1"`, once the cross-reference table was rebuilt, and `<xref rebuilt="1"/>` after
  the last page when it was rebuilt at all: pdftomarkdown then copies no scan).
- `pdfregions`: renders regions and copies images it can verify; no policy, no files.
- `pdftomarkdown` (Rust): what to extract (fates, min side, repeats, caps), checks records
  against the report (id, size), encodes WebP / compacts and allowlists SVG, writes DIR and
  `manifest.jsonl`. It alone maps ids to paths (`page-NNNN/image-MMM.<ext>`,
  `page-NNNN/figure-KKK.<ext>`; `docslurp/doppler-md/src/extract/layout.rs`), so ids on this
  protocol never become paths here.

## Output folder (pdftomarkdown)

- DIR must not exist; it is created with `mkdirat` mode **0700**, and each page folder
  (`page-NNNN`) with `mkdirat` mode **0700** under DIR's descriptor and opened `O_NOFOLLOW`.
  Files are created **0600** (`O_CREAT|O_EXCL|O_NOFOLLOW` to a temporary name, renamed with
  `RENAME_NOREPLACE`). The modes hold whatever the umask (it can only remove bits): images can
  be tenant data in a shared VM tmp. Copy or `chmod` them to share.
- `manifest.jsonl` (JSON Lines, schema `doppler-images/3`; docslurp/doppler-md/README.md) is
  written last, after the files are synced, to a temporary name that is synced and renamed into
  place without replacing anything (a resume renames its new one over the old). A header line,
  one line per image, one per batch, and an end line with the line counts and the SHA-256 of
  every byte before it; ASCII, at most 64 KiB per line, so it is read one line at a time with
  fixed memory. Every link of the Markdown has an entry and every file is listed. A DIR without
  a manifest, or with one that has no valid end line, is garbage.

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
  files it wrote and the next batch goes on (two failed batches in a row, or three in all,
  stop the rest; scan-budget stops count, a stop by the figures' budget alone does not, and
  `pdftomarkdown --images-resume` finishes those later); the whole extraction has a deadline (`--images-deadline`, by
  default 120 s + 0.5 s per converted page, at most 3600 s); the
  manifest says what is missing and why, and the exit status is 5 with the folder kept.
- **Copy jobs move stored bytes out of the parser.** A copied JPEG is the document's own
  bytes: checked only for its markers, never decoded by pdftomarkdown. A copy job reaches only
  images its page names (no arbitrary object of the file), and only when the dictionary-only
  classifier agrees, so the job line cannot widen what is copied. Whoever serves `.jpg` files
  must treat them as attacker bytes: a fixed `image/jpeg` type and `nosniff`. Only a
  write failure or a renderer that cannot start fails the extraction: exit 6, folder removed.
- **Links cannot be forged.** Only the converter's sealed markers expand to image paths; text of
  the PDF that looks like `page-NNNN/image-` or `page-NNNN/figure-` (any case, any prefix) gets
  a zero-width space after `page` first.
- **Confinement in production.** Pass `--images-confine require` once the VM kernel is known to
  support Landlock and seccomp; with `auto`, the report's `images_confined` says whether it held.
