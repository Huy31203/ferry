//! PNG: quantise with imagequant (pngquant's engine), then recompress with oxipng.

use std::io::Cursor;

use imagequant::RGBA;
use oxipng::{BitDepth, ColorType, Options, RGBA8, RawImage};

use super::{Encoder, OptimiseError, OptimiseOptions, quality};
use crate::edit::raster::Raster;

/// Chunks that change how the picture looks, kept whatever the metadata setting.
const RENDERING: [[u8; 4]; 5] = [*b"iCCP", *b"sRGB", *b"gAMA", *b"cHRM", *b"pHYs"];
/// Chunks that describe the old pixel data or file layout; invalid after re-encoding.
/// `iDOT` (Apple's IDAT offsets) and `caBX` (a C2PA hash of the old pixels) are
/// ancillary but wrong after re-encoding, and Apple's decoders trust `iDOT`.
const REBUILT: [[u8; 4]; 9] = [
    *b"IHDR", *b"PLTE", *b"IDAT", *b"IEND", *b"tRNS", *b"bKGD", *b"hIST", *b"iDOT", *b"caBX",
];
/// The largest picture decoded to RGBA (about 600 MB). A header can claim far
/// more than its data holds, and the decode buffer is allocated from the claim.
pub(crate) const MAX_PIXELS: u64 = 150_000_000;

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let (width, height, rgba) = decode(input)?;
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    pack(
        width,
        height,
        rgba,
        kept_chunks(input, o.strip_metadata),
        None,
        o,
    )
}

/// Any PNG as straight RGBA, with its ICC profile.
pub(crate) fn decode_raster(input: &[u8]) -> Result<Raster, OptimiseError> {
    let (width, height, pixels) = decode(input)?;
    let icc = png::Decoder::new(Cursor::new(input))
        .read_info()
        .ok()
        .and_then(|reader| reader.info().icc_profile.as_ref().map(|p| p.to_vec()));
    let rgba = pixels.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
    Ok(Raster {
        width,
        height,
        rgba,
        icc,
    })
}

/// Edited pixels as an optimised PNG.
pub(crate) fn encode_raster(r: &Raster, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let rgba = r
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();
    pack(r.width, r.height, rgba, Vec::new(), r.icc.as_deref(), o)
}

/// Quantise with imagequant, then write with oxipng, adding `chunks` and the ICC profile.
fn pack(
    width: u32,
    height: u32,
    rgba: Vec<RGBA>,
    chunks: Vec<([u8; 4], Vec<u8>)>,
    icc: Option<&[u8]>,
    o: &OptimiseOptions,
) -> Result<Vec<u8>, OptimiseError> {
    let (speed, preset) = effort(o);
    // stream F: pipelines (visually lossless keeps every pixel, so no quantising)
    let mut raw = if o.encoder == Some(Encoder::VisuallyLossless) {
        let bytes = rgba.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
        RawImage::new(width, height, ColorType::RGBA, BitDepth::Eight, bytes).map_err(encode_err)?
    } else {
        let mut attr = imagequant::new();
        attr.set_speed(speed).map_err(encode_err)?;
        attr.set_quality(0, quality(o)).map_err(encode_err)?;
        let mut image = attr
            .new_image(rgba, width as usize, height as usize, 0.0)
            .map_err(encode_err)?;
        let mut quantised = attr.quantize(&mut image).map_err(encode_err)?;
        quantised.set_dithering_level(1.0).map_err(encode_err)?;
        let (palette, indexed) = quantised.remapped(&mut image).map_err(encode_err)?;
        if o.cancel.is_cancelled() {
            return Err(OptimiseError::Cancelled);
        }
        let palette = palette
            .iter()
            .map(|c| RGBA8::new(c.r, c.g, c.b, c.a))
            .collect();
        RawImage::new(
            width,
            height,
            ColorType::Indexed { palette },
            BitDepth::Eight,
            indexed,
        )
        .map_err(encode_err)?
    };
    for (name, data) in chunks {
        raw.add_png_chunk(name, data);
    }
    if let Some(icc) = icc {
        raw.add_icc_profile(icc);
    }
    raw.create_optimized_png(&Options::from_preset(preset))
        .map_err(encode_err)
}

/// imagequant speed and oxipng preset. Spec §4.2 asks for speed 4 and preset 2, but
/// that takes about 2.5 s on a 5 MB screenshot and NFR-P2 allows 1.5 s, so Normal uses
/// speed 8 and preset 1: the same palette size and output about 1% bigger. Aggressive
/// is a deliberate slower retry and keeps the spec's settings (speed 4, preset 4).
fn effort(o: &OptimiseOptions) -> (i32, u8) {
    match (o.encoder, o.level) {
        // stream F: pipelines
        (Some(Encoder::Fast), _) => (8, 1),
        (Some(Encoder::Small), _) => (4, 4),
        (Some(Encoder::VisuallyLossless), _) => (8, 2),
        (None, super::Level::Normal) => (8, 1),
        (None, super::Level::Aggressive) => (4, 4),
    }
}

fn encode_err(e: impl std::fmt::Display) -> OptimiseError {
    OptimiseError::Encode(e.to_string())
}

/// Decode any PNG to 8-bit RGBA.
pub(crate) fn decode(input: &[u8]) -> Result<(u32, u32, Vec<RGBA>), OptimiseError> {
    let decode_err = |e: png::DecodingError| OptimiseError::Decode(e.to_string());
    let mut decoder = png::Decoder::new(Cursor::new(input));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(decode_err)?;
    let (width, height) = reader.info().size();
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(OptimiseError::Unsupported(format!(
            "{width} x {height} pixels is too large to optimise"
        )));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| OptimiseError::Decode("image is too large".into()))?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(decode_err)?;
    let data = &buf[..info.buffer_size()];
    let pixels = match info.color_type {
        png::ColorType::Rgba => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| RGBA::new(p[0], p[1], p[2], p[3]))
            .collect(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| RGBA::new(p[0], p[1], p[2], 255))
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| RGBA::new(p[0], p[0], p[0], p[1]))
            .collect(),
        png::ColorType::Grayscale => data.iter().map(|&v| RGBA::new(v, v, v, 255)).collect(),
        png::ColorType::Indexed => {
            return Err(OptimiseError::Decode("palette was not expanded".into()));
        }
    };
    Ok((info.width, info.height, pixels))
}

/// The ancillary chunks of `input` to carry into the output.
fn kept_chunks(input: &[u8], strip_metadata: bool) -> Vec<([u8; 4], Vec<u8>)> {
    let mut kept = Vec::new();
    let mut pos = 8;
    while let Some(head) = input.get(pos..pos + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let name: [u8; 4] = head[4..8].try_into().expect("slice is 4 bytes");
        let Some(data) = input.get(pos + 8..pos + 8 + len) else {
            break;
        };
        let rendering = RENDERING.contains(&name);
        let ancillary = name[0] & 0x20 != 0; // lower-case first letter
        if rendering || (!strip_metadata && ancillary && !REBUILT.contains(&name)) {
            kept.push((name, data.to_vec()));
        }
        pos += 12 + len;
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    fn decoded_size(bytes: &[u8]) -> (u32, u32) {
        let (w, h, _) = decode(bytes).expect("output decodes");
        (w, h)
    }

    #[test]
    fn a_screenshot_shrinks_and_still_decodes_to_the_same_size() {
        let input = testutil::png(600, 400);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        assert_eq!(decoded_size(&out), (600, 400));
    }

    #[test]
    fn aggressive_is_no_bigger_than_normal() {
        let input = testutil::png(600, 400);
        let normal = optimise(&input, &OptimiseOptions::default()).unwrap();
        let aggressive = optimise(
            &input,
            &OptimiseOptions {
                level: super::super::Level::Aggressive,
                ..OptimiseOptions::default()
            },
        )
        .unwrap();
        assert!(aggressive.len() <= normal.len());
    }

    #[test]
    fn rendering_chunks_survive_and_text_is_stripped() {
        let base = testutil::png(64, 64);
        let input = testutil::with_png_chunk(&base, b"pHYs", &[0, 0, 11, 19, 0, 0, 11, 19, 1]);
        let input = testutil::with_png_chunk(&input, b"tEXt", b"Comment\0hello");
        let names =
            testutil::png_chunk_names(&optimise(&input, &OptimiseOptions::default()).unwrap());
        assert!(names.contains(&"pHYs".to_string()), "{names:?}");
        assert!(!names.contains(&"tEXt".to_string()), "{names:?}");
    }

    #[test]
    fn text_is_kept_when_metadata_is_not_stripped() {
        let input = testutil::with_png_chunk(&testutil::png(64, 64), b"tEXt", b"Comment\0hello");
        let o = OptimiseOptions {
            strip_metadata: false,
            ..OptimiseOptions::default()
        };
        let names = testutil::png_chunk_names(&optimise(&input, &o).unwrap());
        assert!(names.contains(&"tEXt".to_string()), "{names:?}");
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            optimise(b"\x89PNG\r\n\x1a\nnot really", &OptimiseOptions::default()),
            Err(OptimiseError::Decode(_))
        ));
    }

    #[test]
    fn apple_idot_and_c2pa_chunks_are_dropped_even_when_metadata_is_kept() {
        // iDOT holds IDAT offsets and caBX a hash of the old pixels; both are
        // wrong after re-encoding, and Apple's decoders trust iDOT.
        let input = testutil::with_png_chunk(&testutil::png(64, 64), b"iDOT", &[0; 28]);
        let input = testutil::with_png_chunk(&input, b"caBX", b"manifest");
        let input = testutil::with_png_chunk(&input, b"tEXt", b"Comment\0hello");
        let o = OptimiseOptions {
            strip_metadata: false,
            ..OptimiseOptions::default()
        };
        let names = testutil::png_chunk_names(&optimise(&input, &o).unwrap());
        assert!(names.contains(&"tEXt".to_string()), "{names:?}");
        for stale in ["iDOT", "caBX"] {
            assert!(!names.contains(&stale.to_string()), "{stale}: {names:?}");
        }
    }

    #[test]
    fn a_png_declaring_an_enormous_size_is_refused_before_allocating() {
        // About 100 bytes that claim 65535 x 65535 pixels would otherwise ask for 17 GB.
        let input = testutil::png_claiming_size(65535, 65535);
        assert!(matches!(
            optimise(&input, &OptimiseOptions::default()),
            Err(OptimiseError::Unsupported(_))
        ));
    }

    // stream F: pipelines
    #[test]
    fn visually_lossless_png_keeps_every_pixel() {
        let input = testutil::png(64, 64);
        let o = OptimiseOptions {
            encoder: Some(Encoder::VisuallyLossless),
            ..OptimiseOptions::default()
        };
        let out = optimise(&input, &o).unwrap();
        assert_eq!(
            decode_raster(&out).unwrap().rgba,
            decode_raster(&input).unwrap().rgba
        );
    }
}
