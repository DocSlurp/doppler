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

/// libwebp's lossless effort (`method` 0..=6 and `quality` 0..=100): 4 and 50 are within 2-3%
/// of the size of the slowest setting at about a third of its time on figure crops (README).
const LOSSLESS_METHOD: i32 = 4;
const LOSSLESS_QUALITY: f32 = 50.0;
/// libwebp's lossy effort: its default.
const LOSSY_METHOD: i32 = 4;

/// Encodes `rgb` (`width * height * 3` bytes).
pub fn encode(rgb: &[u8], width: u32, height: u32, format: Raster) -> Result<Vec<u8>, String> {
    let need = (width as usize).checked_mul(height as usize).and_then(|n| n.checked_mul(3));
    if width == 0 || height == 0 || need != Some(rgb.len()) {
        return Err("pixel buffer does not match its size".into());
    }
    match format {
        Raster::Png => png(rgb, width, height),
        Raster::WebpLossy(q) => webp(rgb, width, height, Some(q)),
        Raster::WebpLossless => webp(rgb, width, height, None),
    }
}

fn webp(rgb: &[u8], width: u32, height: u32, quality: Option<u8>) -> Result<Vec<u8>, String> {
    // libwebp's limit
    if width > 16383 || height > 16383 {
        return Err("too large for WebP".into());
    }
    let mut config = webp::WebPConfig::new().map_err(|_| "libwebp version mismatch".to_string())?;
    match quality {
        Some(q) => {
            config.lossless = 0;
            config.quality = f32::from(q.min(100));
            config.method = LOSSY_METHOD;
        }
        None => {
            config.lossless = 1;
            config.quality = LOSSLESS_QUALITY;
            config.method = LOSSLESS_METHOD;
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
