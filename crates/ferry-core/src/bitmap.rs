//! Windows clipboard bitmaps (spec §4.4). A copy that holds only a DIB becomes
//! a lossless PNG that Ferry can optimise, and an optimised image also goes
//! back as a `CF_DIBV5` for apps that paste only bitmaps. Pure byte work, so it
//! is tested on every OS even though only Windows uses it.

use std::io::Cursor;

use crate::sniff::Format;

const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const V5_HEADER: usize = 124;
/// `LCS_sRGB`, the colour space tag of a V5 header.
const LCS_SRGB: u32 = 0x7352_4742;
/// `LCS_GM_IMAGES`, the rendering intent for photos and screenshots.
const LCS_GM_IMAGES: u32 = 4;

/// Width, height and straight (not premultiplied) RGBA pixels, top row first.
pub type Rgba = (u32, u32, Vec<u8>);

/// A packed DIB (`CF_DIB` or `CF_DIBV5`) as a lossless PNG.
pub fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>, String> {
    let (width, height, rgba) = dib_to_rgba(dib)?;
    encode_png(width, height, &rgba)
}

/// Read a packed DIB: a BITMAPINFOHEADER, V4 or V5 header, then masks or a
/// palette, then 24- or 32-bit rows (bottom-up unless the height is negative).
pub fn dib_to_rgba(dib: &[u8]) -> Result<Rgba, String> {
    let bad = |what: &str| format!("unsupported bitmap: {what}");
    let header = le32(dib, 0).ok_or_else(|| bad("no header"))? as usize;
    let width = le32(dib, 4).ok_or_else(|| bad("no width"))? as i32;
    let height = le32(dib, 8).ok_or_else(|| bad("no height"))? as i32;
    let bits = le16(dib, 14).ok_or_else(|| bad("no bit count"))?;
    let compression = le32(dib, 16).ok_or_else(|| bad("no compression"))?;
    let colours_used = le32(dib, 32).ok_or_else(|| bad("no palette size"))? as usize;
    if header < 40 || width <= 0 || height == 0 {
        return Err(bad("size"));
    }
    if !matches!(bits, 24 | 32) || !matches!(compression, BI_RGB | BI_BITFIELDS) {
        return Err(bad(&format!("{bits}-bit, compression {compression}")));
    }
    let (w, h) = (width as u32, height.unsigned_abs());
    let top_down = height < 0;

    // BI_BITFIELDS puts three masks after a 40-byte header; V4 and V5 headers
    // hold all four masks themselves.
    let mut masks = [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0];
    let mut offset = header;
    if compression == BI_BITFIELDS {
        let count = if header == 40 { 3 } else { 4 };
        for (i, mask) in masks.iter_mut().enumerate().take(count) {
            *mask = le32(dib, 40 + i * 4).ok_or_else(|| bad("masks"))?;
        }
        if header == 40 {
            offset += 12;
        }
    }
    offset += colours_used * 4;

    let stride = (w as usize * bits as usize).div_ceil(32) * 4;
    let end = stride
        .checked_mul(h as usize)
        .and_then(|n| n.checked_add(offset))
        .ok_or_else(|| bad("size"))?;
    let pixels = dib.get(offset..end).ok_or_else(|| bad("cut short"))?;

    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h as usize {
        let row = if top_down { y } else { h as usize - 1 - y };
        let row = &pixels[row * stride..][..stride];
        for x in 0..w as usize {
            if bits == 24 {
                let p = &row[x * 3..x * 3 + 3];
                rgba.extend_from_slice(&[p[2], p[1], p[0], 255]);
            } else {
                let v = u32::from_le_bytes(row[x * 4..x * 4 + 4].try_into().expect("4 bytes"));
                let alpha = if masks[3] == 0 {
                    255
                } else {
                    channel(v, masks[3])
                };
                rgba.extend_from_slice(&[
                    channel(v, masks[0]),
                    channel(v, masks[1]),
                    channel(v, masks[2]),
                    alpha,
                ]);
            }
        }
    }
    // Most apps leave the fourth byte of a 32-bit bitmap at zero; that is an
    // unused byte, not a fully transparent picture.
    if rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        rgba.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
    }
    Ok((w, h, rgba))
}

/// A `CF_DIBV5` of `rgba`: a V5 header with BI_BITFIELDS masks and the sRGB
/// colour space, then 32-bit BGRA rows, bottom-up.
pub fn rgba_to_dibv5(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(V5_HEADER + rgba.len());
    let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    put(V5_HEADER as u32);
    put(width);
    put(height); // positive: bottom-up
    put(1 | (32 << 16)); // planes, bit count
    put(BI_BITFIELDS);
    put(width * height * 4);
    put(0); // x pixels per metre
    put(0); // y pixels per metre
    put(0); // colours used
    put(0); // colours important
    for mask in [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
        put(mask);
    }
    put(LCS_SRGB);
    for _ in 0..12 {
        put(0); // endpoints (9) and gamma (3), unused for sRGB
    }
    put(LCS_GM_IMAGES);
    put(0); // profile data
    put(0); // profile size
    put(0); // reserved
    for row in rgba.chunks_exact(width as usize * 4).rev() {
        for p in row.as_chunks::<4>().0 {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out
}

/// Decode an optimised image for the bitmap copy. GIF is not decoded (Ferry has
/// no GIF decoder), so a GIF goes on the clipboard in its own format only.
pub fn decode_rgba(bytes: &[u8], format: Format) -> Result<Rgba, String> {
    match format {
        Format::Png => {
            let (w, h, pixels) = crate::optimise::png::decode(bytes).map_err(|e| e.to_string())?;
            Ok((
                w,
                h,
                pixels.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect(),
            ))
        }
        Format::Jpeg => {
            let decoder = mozjpeg::Decompress::new_mem(bytes).map_err(|e| e.to_string())?;
            let (w, h) = decoder.size();
            let rgb = decoder
                .rgb()
                .and_then(|mut d| d.read_scanlines::<[u8; 3]>())
                .map_err(|e| e.to_string())?;
            let rgba = rgb.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
            Ok((w as u32, h as u32, rgba))
        }
        // sE pdf-formats
        Format::Tiff | Format::Heic | Format::Avif | Format::Pdf => {
            Err(format!("no bitmap for {}", format.label()))
        }
        // stream B: audio never reaches the image engine
        Format::Mp3
        | Format::M4a
        | Format::Flac
        | Format::Wav
        | Format::Aiff
        | Format::Ogg
        | Format::Opus => Err("not an image".into()),
        Format::WebP => {
            let image = webp::Decoder::new(bytes)
                .decode()
                .ok_or("not a still WebP")?;
            let (w, h) = (image.width(), image.height());
            let rgba = if image.is_alpha() {
                image.to_vec()
            } else {
                image
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .flat_map(|p| [p[0], p[1], p[2], 255])
                    .collect()
            };
            Ok((w, h, rgba))
        }
        Format::Gif => Err("GIF has no bitmap copy".into()),
        // stream A: video
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => {
            Err("video is not an image".into())
        }
    }
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// One channel of a packed pixel, scaled to 8 bits.
fn channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let max = mask >> mask.trailing_zeros();
    let value = (pixel & mask) >> mask.trailing_zeros();
    (u64::from(value) * 255 / u64::from(max)) as u8
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sniff, testutil};

    /// A 40-byte BITMAPINFOHEADER.
    fn info_header(width: i32, height: i32, bits: u16, compression: u32) -> Vec<u8> {
        let mut h = Vec::new();
        h.extend_from_slice(&40_u32.to_le_bytes());
        h.extend_from_slice(&width.to_le_bytes());
        h.extend_from_slice(&height.to_le_bytes());
        h.extend_from_slice(&1_u16.to_le_bytes());
        h.extend_from_slice(&bits.to_le_bytes());
        h.extend_from_slice(&compression.to_le_bytes());
        h.extend_from_slice(&[0; 20]); // image size, resolution, palette counts
        h
    }

    #[test]
    fn rgba_survives_a_round_trip_through_a_v5_bitmap() {
        let (w, h) = (5, 3);
        let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i * 7) as u8).collect();
        let dib = rgba_to_dibv5(w, h, &rgba);
        assert_eq!(dib.len(), 124 + rgba.len());
        assert_eq!(dib_to_rgba(&dib).unwrap(), (w, h, rgba));
    }

    #[test]
    fn a_bottom_up_24_bit_bitmap_is_read_top_row_first_with_row_padding() {
        // 3×2 pixels; each 9-byte row is padded to 12. Bottom row first in the file.
        let mut dib = info_header(3, 2, 24, BI_RGB);
        dib.extend_from_slice(&[255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0, 0]); // bottom: blue
        dib.extend_from_slice(&[0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0]); // top: red
        let (w, h, rgba) = dib_to_rgba(&dib).unwrap();
        assert_eq!((w, h), (3, 2));
        assert_eq!(&rgba[..4], &[255, 0, 0, 255], "top-left is red");
        assert_eq!(&rgba[12..16], &[0, 0, 255, 255], "bottom-left is blue");
    }

    #[test]
    fn a_32_bit_bitmap_with_an_unused_alpha_byte_is_opaque() {
        let mut dib = info_header(2, -1, 32, BI_RGB); // top-down
        dib.extend_from_slice(&[10, 20, 30, 0, 40, 50, 60, 0]);
        let (_, _, rgba) = dib_to_rgba(&dib).unwrap();
        assert_eq!(rgba, vec![30, 20, 10, 255, 60, 50, 40, 255]);
    }

    #[test]
    fn bitfield_masks_after_a_short_header_are_honoured() {
        let mut dib = info_header(1, 1, 32, BI_BITFIELDS);
        // Masks for an RGBX layout in memory: red in the low byte.
        for mask in [0x0000_00FF_u32, 0x0000_FF00, 0x00FF_0000] {
            dib.extend_from_slice(&mask.to_le_bytes());
        }
        dib.extend_from_slice(&[1, 2, 3, 0]);
        assert_eq!(dib_to_rgba(&dib).unwrap().2, vec![1, 2, 3, 255]);
    }

    #[test]
    fn a_bitmap_becomes_a_png_of_the_same_size() {
        let rgba = testutil::screenshot_rgba(40, 30);
        let png = dib_to_png(&rgba_to_dibv5(40, 30, &rgba)).unwrap();
        assert_eq!(sniff::sniff(&png), Some(Format::Png));
        assert_eq!(sniff::dimensions(&png, Format::Png), Some((40, 30)));
        assert_eq!(decode_rgba(&png, Format::Png).unwrap().2, rgba, "lossless");
    }

    #[test]
    fn broken_or_unsupported_bitmaps_are_errors() {
        assert!(dib_to_rgba(&[]).is_err());
        let mut short = info_header(100, 100, 24, BI_RGB);
        short.extend_from_slice(&[0; 10]);
        assert!(dib_to_rgba(&short).is_err());
        let mut eight_bit = info_header(1, 1, 8, BI_RGB);
        eight_bit.extend_from_slice(&[0; 8]);
        assert!(dib_to_rgba(&eight_bit).is_err());
        assert!(dib_to_rgba(&info_header(0, 1, 24, BI_RGB)).is_err());
    }

    #[test]
    fn every_format_but_gif_decodes_for_the_bitmap_copy() {
        let png = testutil::png(12, 7);
        assert_eq!(decode_rgba(&png, Format::Png).unwrap().0, 12);
        let jpeg = testutil::jpeg(12, 7, 80, &Default::default());
        let (w, h, rgba) = decode_rgba(&jpeg, Format::Jpeg).unwrap();
        assert_eq!((w, h, rgba.len()), (12, 7, 12 * 7 * 4));
        let webp = testutil::webp_with_transparent_corner(12, 8);
        let (_, _, rgba) = decode_rgba(&webp, Format::WebP).unwrap();
        assert_eq!(rgba[3], 0, "the transparent corner keeps its alpha");
        assert!(decode_rgba(&testutil::gif(1), Format::Gif).is_err());
    }
}
