//! Encoders for rendered crops (RGB 8-bit, rows top first): WebP (libwebp, lossy or lossless)
//! and PNG. Both write the image data only: WebP a RIFF with one `VP8 ` or `VP8L` chunk, PNG
//! IHDR, IDAT and IEND.

/// A raster format of the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raster {
    /// lossy WebP at this quality (0..=100)
    WebpLossy(u8),
    WebpLossless,
    Png,
}

impl Raster {
    pub fn extension(self) -> &'static str {
        match self {
            Raster::WebpLossy(_) | Raster::WebpLossless => "webp",
            Raster::Png => "png",
        }
    }
}

/// libwebp's encoder effort. More effort is slower and gives smaller files; it never changes
/// what a lossless file decodes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effort {
    /// lossy `method`, 0..=6 (`--raster-method`)
    pub lossy_method: u8,
    /// lossless `method`, 0..=6 (`--lossless-method`)
    pub lossless_method: u8,
    /// lossless `quality` (effort, not fidelity), 0..=100 (`--lossless-quality`)
    pub lossless_quality: u8,
}

impl Default for Effort {
    /// Lossy: libwebp's default method 4. Lossless: method 4 and quality 50 are within 2-3% of
    /// the size of the slowest setting at about a third of its time on figure crops (README).
    fn default() -> Self {
        Effort { lossy_method: 4, lossless_method: 4, lossless_quality: 50 }
    }
}

/// Encodes `rgb` (`width * height * 3` bytes) at the default effort.
pub fn encode(rgb: &[u8], width: u32, height: u32, format: Raster) -> Result<Vec<u8>, String> {
    encode_with(rgb, width, height, format, Effort::default())
}

/// Encodes `rgb` (`width * height * 3` bytes) at `effort`.
pub fn encode_with(rgb: &[u8], width: u32, height: u32, format: Raster, effort: Effort) -> Result<Vec<u8>, String> {
    let need = (width as usize).checked_mul(height as usize).and_then(|n| n.checked_mul(3));
    if width == 0 || height == 0 || need != Some(rgb.len()) {
        return Err("pixel buffer does not match its size".into());
    }
    match format {
        Raster::Png => png(rgb, width, height),
        Raster::WebpLossy(q) => webp(rgb, width, height, Some(q), effort),
        Raster::WebpLossless => webp(rgb, width, height, None, effort),
    }
}

fn webp(rgb: &[u8], width: u32, height: u32, quality: Option<u8>, effort: Effort) -> Result<Vec<u8>, String> {
    // libwebp's limit
    if width > 16383 || height > 16383 {
        return Err("too large for WebP".into());
    }
    let mut config = webp::WebPConfig::new().map_err(|_| "libwebp version mismatch".to_string())?;
    match quality {
        Some(q) => {
            config.lossless = 0;
            config.quality = f32::from(q.min(100));
            config.method = i32::from(effort.lossy_method.min(6));
        }
        None => {
            config.lossless = 1;
            config.quality = f32::from(effort.lossless_quality.min(100));
            config.method = i32::from(effort.lossless_method.min(6));
            // keep the RGB values exactly
            config.exact = 1;
        }
    }
    let enc = webp::Encoder::from_rgb(rgb, width, height);
    let out = enc.encode_advanced(&config).map_err(|e| format!("WebP: {e:?}"))?;
    Ok(out.to_vec())
}

fn png(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(rgb.len() / 4 + 64);
    {
        let mut enc = ::png::Encoder::new(&mut out, width, height);
        enc.set_color(::png::ColorType::Rgb);
        enc.set_depth(::png::BitDepth::Eight);
        enc.set_compression(::png::Compression::Balanced);
        enc.set_filter(::png::Filter::Adaptive);
        let mut w = enc.write_header().map_err(|e| format!("PNG: {e}"))?;
        w.write_image_data(rgb).map_err(|e| format!("PNG: {e}"))?;
        w.finish().map_err(|e| format!("PNG: {e}"))?;
    }
    Ok(out)
}

/// A 1-bit greyscale PNG of packed rows (`ceil(width / 8) * height` bytes, most significant
/// bit first, 0 black), as a bilevel scan arrives from the renderer.
pub fn png_gray1(rows: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let row = (width as usize).div_ceil(8);
    if width == 0 || height == 0 || row.checked_mul(height as usize) != Some(rows.len()) {
        return Err("pixel buffer does not match its size".into());
    }
    let mut out = Vec::with_capacity(rows.len() / 8 + 64);
    {
        let mut enc = ::png::Encoder::new(&mut out, width, height);
        enc.set_color(::png::ColorType::Grayscale);
        enc.set_depth(::png::BitDepth::One);
        enc.set_compression(::png::Compression::Fast);
        let mut w = enc.write_header().map_err(|e| format!("PNG: {e}"))?;
        w.write_image_data(rows).map_err(|e| format!("PNG: {e}"))?;
        w.finish().map_err(|e| format!("PNG: {e}"))?;
    }
    Ok(out)
}

/// RGB from 8-bit grey (each sample three times).
pub fn gray_to_rgb(gray: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(gray.len() * 3);
    for &g in gray {
        out.extend_from_slice(&[g, g, g]);
    }
    out
}

/// `pixels` (`width * height * channels` bytes) scaled down by area averaging to
/// `new_w * new_h` (each at most the old size, at least 1); the same buffer when the size is
/// unchanged.
pub fn downscale(pixels: Vec<u8>, width: u32, height: u32, channels: usize, new_w: u32, new_h: u32) -> Vec<u8> {
    let (w, h, nw, nh) = (
        width as usize,
        height as usize,
        (new_w as usize).clamp(1, width as usize),
        (new_h as usize).clamp(1, height as usize),
    );
    if (nw, nh) == (w, h) || pixels.len() != w * h * channels {
        return pixels;
    }
    // per output column, its source columns [x0, x1)
    let cols: Vec<(usize, usize)> = (0..nw).map(|x| (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1))).collect();
    let mut out = vec![0u8; nw * nh * channels];
    let mut acc = vec![0u64; nw * channels];
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        acc.iter_mut().for_each(|a| *a = 0);
        for sy in y0..y1 {
            let src = &pixels[sy * w * channels..(sy + 1) * w * channels];
            for (x, &(x0, x1)) in cols.iter().enumerate() {
                for sx in x0..x1 {
                    for c in 0..channels {
                        acc[x * channels + c] += u64::from(src[sx * channels + c]);
                    }
                }
            }
        }
        for (x, &(x0, x1)) in cols.iter().enumerate() {
            let n = ((x1 - x0) * (y1 - y0)) as u64;
            for c in 0..channels {
                out[(y * nw + x) * channels + c] = ((acc[x * channels + c] + n / 2) / n) as u8;
            }
        }
    }
    out
}

/// Whether `data` looks like a whole JPEG of `width` x `height`: SOI, a frame header (SOF0..2)
/// of that size before the first scan, and EOI last. Cheap (headers only); the bytes are never
/// decoded here.
pub fn jpeg_ok(data: &[u8], width: u32, height: u32) -> bool {
    let n = data.len();
    if n < 4 || data[..2] != [0xFF, 0xD8] || data[n - 2..] != [0xFF, 0xD9] {
        return false;
    }
    let mut i = 2;
    while i + 4 <= n {
        if data[i] != 0xFF {
            return false;
        }
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        if len < 2 || i + 2 + len > n {
            return false;
        }
        match marker {
            0xC0..=0xC2 if len >= 8 => {
                let h = u32::from(u16::from_be_bytes([data[i + 5], data[i + 6]]));
                let w = u32::from(u16::from_be_bytes([data[i + 7], data[i + 8]]));
                return data[i + 4] == 8 && (w, h) == (width, height);
            }
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF | 0xDA => return false,
            _ => i += 2 + len,
        }
    }
    false
}

/// The chunk types of a RIFF/WebP file, or `None` when it is not one.
pub fn webp_chunks(data: &[u8]) -> Option<Vec<[u8; 4]>> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }
    let size = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    if size + 8 != data.len() {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 12;
    while i + 8 <= data.len() {
        let tag: [u8; 4] = data[i..i + 4].try_into().ok()?;
        let n = u32::from_le_bytes(data[i + 4..i + 8].try_into().ok()?) as usize;
        out.push(tag);
        i += 8 + n + (n & 1);
    }
    (i == data.len()).then_some(out)
}

/// The chunk types of a PNG file, or `None` when it is not one.
pub fn png_chunks(data: &[u8]) -> Option<Vec<[u8; 4]>> {
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 8;
    while i + 12 <= data.len() {
        let n = u32::from_be_bytes(data[i..i + 4].try_into().ok()?) as usize;
        out.push(data[i + 4..i + 8].try_into().ok()?);
        i += 12 + n;
    }
    (i == data.len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray1_png_decodes_to_its_bits() {
        let (w, h) = (13u32, 3u32);
        let rows = vec![0b1010_1010, 0b1111_1000, 0xFF, 0x00, 0x00, 0x0F, 0xF0];
        assert!(png_gray1(&rows[..5], w, h).is_err());
        let rows = [0b1010_1010u8, 0b1111_1000, 0xFF, 0x00, 0x00, 0x00];
        let png = png_gray1(&rows, w, h).unwrap();
        let mut dec = ::png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
        let mut buf = vec![0u8; dec.output_buffer_size().unwrap()];
        let info = dec.next_frame(&mut buf).unwrap();
        assert_eq!(
            (info.width, info.height, info.bit_depth, info.color_type),
            (13, 3, ::png::BitDepth::One, ::png::ColorType::Grayscale)
        );
        // padding bits past the width are not pixels
        assert_eq!(buf[0], 0b1010_1010);
        assert_eq!(buf[1] & 0b1111_1000, 0b1111_1000);
        assert_eq!(buf[2], 0xFF);
        assert_eq!(&buf[4..6], &[0, 0]);
    }

    #[test]
    fn downscale_averages_areas() {
        // 4x2 grey to 2x1: each output pixel the mean of a 2x2 block
        let px = vec![0, 10, 100, 200, 20, 30, 50, 50];
        assert_eq!(downscale(px.clone(), 4, 2, 1, 2, 1), vec![15, 100]);
        assert_eq!(downscale(px.clone(), 4, 2, 1, 4, 2), px);
        let rgb = gray_to_rgb(&[7, 9]);
        assert_eq!(rgb, vec![7, 7, 7, 9, 9, 9]);
        assert_eq!(downscale(rgb, 2, 1, 3, 1, 1), vec![8, 8, 8]);
    }

    #[test]
    fn jpeg_headers_are_checked() {
        // SOI, SOF0 8-bit 3x2, SOS, data, EOI
        let mut j = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x02, 0x00, 0x03, 0x01, 0x01, 0x11, 0x00];
        j.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x12, 0x34, 0xFF, 0xD9]);
        assert!(jpeg_ok(&j, 3, 2));
        assert!(!jpeg_ok(&j, 2, 3));
        assert!(!jpeg_ok(&j[..j.len() - 1], 3, 2));
        let mut lossless = j.clone();
        lossless[3] = 0xC3;
        assert!(!jpeg_ok(&lossless, 3, 2));
        assert!(!jpeg_ok(b"\x89PNG\r\n", 3, 2));
    }

    fn crop() -> (Vec<u8>, u32, u32) {
        let (w, h) = (40u32, 30u32);
        let mut rgb = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgb.extend_from_slice(&[(x * 6) as u8, (y * 8) as u8, if (x / 5 + y / 5) % 2 == 0 { 255 } else { 0 }]);
            }
        }
        (rgb, w, h)
    }

    #[test]
    fn webp_has_only_image_data() {
        let (rgb, w, h) = crop();
        let lossy = encode(&rgb, w, h, Raster::WebpLossy(90)).unwrap();
        assert_eq!(webp_chunks(&lossy).unwrap(), vec![*b"VP8 "]);
        let lossless = encode(&rgb, w, h, Raster::WebpLossless).unwrap();
        assert_eq!(webp_chunks(&lossless).unwrap(), vec![*b"VP8L"]);
        // deterministic
        assert_eq!(lossless, encode(&rgb, w, h, Raster::WebpLossless).unwrap());
        assert_eq!(lossy, encode(&rgb, w, h, Raster::WebpLossy(90)).unwrap());
    }

    /// A crop with gradients, edges and noise-like texture, so effort changes the bytes.
    fn textured() -> (Vec<u8>, u32, u32) {
        let (w, h) = (97u32, 61u32);
        let mut rgb = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgb.extend_from_slice(&[
                    ((x * 7 + y * 3) % 256) as u8,
                    ((x * x + y * 11) % 251) as u8,
                    if (x / 7 + y / 5) % 2 == 0 { 230 } else { 20 },
                ]);
            }
        }
        (rgb, w, h)
    }

    fn sha256(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn decode_rgb(data: &[u8]) -> (Vec<u8>, u32, u32) {
        let img = webp::Decoder::new(data).decode().expect("decodes");
        assert!(!img.is_alpha());
        (img.to_vec(), img.width(), img.height())
    }

    #[test]
    fn default_effort_is_the_previous_fixed_encoding() {
        // sha256 of these encodings before the effort became configurable (lossy method 4,
        // lossless method 4 quality 50): the defaults must not change a byte
        let (rgb, w, h) = textured();
        let lossy = encode(&rgb, w, h, Raster::WebpLossy(90)).unwrap();
        let lossless = encode(&rgb, w, h, Raster::WebpLossless).unwrap();
        assert_eq!(
            (lossy.len(), sha256(&lossy).as_str()),
            (4758, "437d5693c658e0f072d26b1dabfbea9df395e317d795d28d41ca508a03365850")
        );
        assert_eq!(
            (lossless.len(), sha256(&lossless).as_str()),
            (666, "a505fea76c23034c76929afb498fffb11773ed63e612b01a91fd7a0ae9029160")
        );
        let explicit = Effort { lossy_method: 4, lossless_method: 4, lossless_quality: 50 };
        assert_eq!(Effort::default(), explicit);
        assert_eq!(encode_with(&rgb, w, h, Raster::WebpLossy(90), explicit).unwrap(), lossy);
        assert_eq!(encode_with(&rgb, w, h, Raster::WebpLossless, explicit).unwrap(), lossless);
    }

    #[test]
    fn faster_effort_is_valid_webp_and_lossless_stays_exact() {
        let fast = Effort { lossy_method: 2, lossless_method: 1, lossless_quality: 25 };
        for (rgb, w, h) in [textured(), crop()] {
            let lossy = encode_with(&rgb, w, h, Raster::WebpLossy(90), fast).unwrap();
            assert_eq!(webp_chunks(&lossy).unwrap(), vec![*b"VP8 "]);
            let (px, dw, dh) = decode_rgb(&lossy);
            assert_eq!((dw, dh, px.len()), (w, h, rgb.len()));
            // deterministic
            assert_eq!(lossy, encode_with(&rgb, w, h, Raster::WebpLossy(90), fast).unwrap());

            let lossless = encode_with(&rgb, w, h, Raster::WebpLossless, fast).unwrap();
            assert_eq!(webp_chunks(&lossless).unwrap(), vec![*b"VP8L"]);
            assert_eq!(decode_rgb(&lossless), (rgb.clone(), w, h), "pixel-exact");
            assert_eq!(decode_rgb(&encode(&rgb, w, h, Raster::WebpLossless).unwrap()), (rgb.clone(), w, h));
        }
        // the extremes are accepted too
        let (rgb, w, h) = textured();
        for e in [
            Effort { lossy_method: 0, lossless_method: 0, lossless_quality: 0 },
            Effort { lossy_method: 6, lossless_method: 6, lossless_quality: 100 },
        ] {
            assert!(webp_chunks(&encode_with(&rgb, w, h, Raster::WebpLossy(90), e).unwrap()).is_some());
            assert_eq!(decode_rgb(&encode_with(&rgb, w, h, Raster::WebpLossless, e).unwrap()).0, rgb);
        }
    }

    #[test]
    fn png_has_only_ihdr_idat_iend() {
        let (rgb, w, h) = crop();
        let png = encode(&rgb, w, h, Raster::Png).unwrap();
        let chunks = png_chunks(&png).unwrap();
        assert_eq!(chunks.first(), Some(b"IHDR"));
        assert_eq!(chunks.last(), Some(b"IEND"));
        assert!(chunks[1..chunks.len() - 1].iter().all(|c| c == b"IDAT"), "{chunks:?}");
        // lossless round trip
        let mut dec = ::png::Decoder::new(std::io::Cursor::new(&png)).read_info().unwrap();
        let mut buf = vec![0; dec.output_buffer_size().unwrap()];
        dec.next_frame(&mut buf).unwrap();
        assert_eq!(buf, rgb);
    }

    #[test]
    fn a_wrong_buffer_size_is_an_error() {
        assert!(encode(&[0; 10], 2, 2, Raster::Png).is_err());
        assert!(encode(&[], 0, 0, Raster::WebpLossless).is_err());
    }
}
