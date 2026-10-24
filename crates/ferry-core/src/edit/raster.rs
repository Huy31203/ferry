//! Pixels in and out for edits (spec §4.2): decode any supported still image
//! to RGBA with its orientation applied and its ICC profile kept, cut and
//! resize it, and encode RGBA through the same encoders `optimise` uses.

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

use super::geometry::Rect;
use crate::optimise::{OptimiseError, OptimiseOptions, jpeg, png, webp};
use crate::sniff::{self, Format};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// Straight (not premultiplied) RGBA, top row first.
    pub rgba: Vec<u8>,
    pub icc: Option<Vec<u8>>,
}

/// A still picture as it is displayed: JPEG orientation applied, a GIF's
/// first frame. Headers claiming more than `png::MAX_PIXELS` are refused
/// before any buffer is allocated.
pub fn decode(bytes: &[u8], format: Format) -> Result<Raster, OptimiseError> {
    if let Some((w, h)) = sniff::dimensions(bytes, format)
        && u64::from(w) * u64::from(h) > png::MAX_PIXELS
    {
        return Err(OptimiseError::Unsupported(format!(
            "{w} x {h} pixels is too large to edit"
        )));
    }
    let mut r = match format {
        Format::Png => png::decode_raster(bytes),
        Format::Jpeg => jpeg::decode_raster(bytes).map(|(r, o)| orient(r, o)),
        Format::WebP => webp::decode_raster(bytes),
        Format::Gif => first_gif_frame(bytes),
        // stream A: video
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => {
            Err(OptimiseError::Unsupported("video is not an image".into()))
        }
        // stream B: audio never reaches the image engine
        Format::Mp3
        | Format::M4a
        | Format::Flac
        | Format::Wav
        | Format::Aiff
        | Format::Ogg
        | Format::Opus => Err(OptimiseError::Unsupported("audio has no pixels".into())),
    }?;
    r.icc = rgb_profile(r.icc);
    Ok(r)
}

/// The pixels are always RGBA, so a grey profile (colour space `GRAY` at bytes
/// 16..20 of the header) would mislabel them.
fn rgb_profile(icc: Option<Vec<u8>>) -> Option<Vec<u8>> {
    icc.filter(|p| p.get(16..20) != Some(b"GRAY".as_slice()))
}

/// The size a card shows for the original: the header's, turned for JPEG
/// orientations 5–8.
pub fn display_dims(bytes: &[u8], format: Format) -> Option<(u32, u32)> {
    let (w, h) = sniff::dimensions(bytes, format)?;
    let turned = format == Format::Jpeg && matches!(jpeg::orientation(bytes), 5..=8);
    Some(if turned { (h, w) } else { (w, h) })
}

/// Apply an EXIF orientation (1–8) to the pixels.
pub fn orient(r: Raster, orientation: u16) -> Raster {
    if !(2..=8).contains(&orientation) {
        return r;
    }
    let (w, h) = (r.width, r.height);
    let (ow, oh) = if orientation >= 5 { (h, w) } else { (w, h) };
    let mut out = vec![0; r.rgba.len()];
    for dy in 0..oh {
        for dx in 0..ow {
            let (sx, sy) = match orientation {
                2 => (w - 1 - dx, dy),
                3 => (w - 1 - dx, h - 1 - dy),
                4 => (dx, h - 1 - dy),
                5 => (dy, dx),
                6 => (dy, h - 1 - dx),
                7 => (w - 1 - dy, h - 1 - dx),
                _ => (w - 1 - dy, dx),
            };
            let s = ((sy * w + sx) * 4) as usize;
            let d = ((dy * ow + dx) * 4) as usize;
            out[d..d + 4].copy_from_slice(&r.rgba[s..s + 4]);
        }
    }
    Raster {
        width: ow,
        height: oh,
        rgba: out,
        icc: r.icc,
    }
}

/// Cut `crop` out of `r` and resize it to `out` (Lanczos3, alpha-aware). A
/// cut that keeps the size copies rows and never resamples.
pub fn cut(r: &Raster, crop: Rect, out: (u32, u32)) -> Result<Raster, OptimiseError> {
    if (crop.width, crop.height) == out {
        let row = (crop.width * 4) as usize;
        let mut rgba = Vec::with_capacity(row * crop.height as usize);
        for y in crop.y..crop.y + crop.height {
            let start = ((y * r.width + crop.x) * 4) as usize;
            rgba.extend_from_slice(&r.rgba[start..start + row]);
        }
        return Ok(Raster {
            width: out.0,
            height: out.1,
            rgba,
            icc: r.icc.clone(),
        });
    }
    let resize_err = |e: &dyn std::fmt::Display| OptimiseError::Encode(format!("resize: {e}"));
    let src =
        ImageRef::new(r.width, r.height, &r.rgba, PixelType::U8x4).map_err(|e| resize_err(&e))?;
    let mut dst = Image::new(out.0, out.1, PixelType::U8x4);
    let options = ResizeOptions::new()
        .crop(
            f64::from(crop.x),
            f64::from(crop.y),
            f64::from(crop.width),
            f64::from(crop.height),
        )
        .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|e| resize_err(&e))?;
    Ok(Raster {
        width: out.0,
        height: out.1,
        rgba: dst.into_vec(),
        icc: r.icc.clone(),
    })
}

/// Pixels as PNG, JPEG or WebP through the optimising encoders.
pub fn encode(r: &Raster, format: Format, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    match format {
        Format::Png => png::encode_raster(r, o),
        Format::Jpeg => jpeg::encode_raster(r, o),
        Format::WebP => webp::encode_raster(r, o),
        Format::Gif => Err(OptimiseError::Unsupported(
            "GIF output comes only from gifsicle".into(),
        )),
        // stream A: video
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => {
            Err(OptimiseError::Unsupported("video is not an image".into()))
        }
        // stream B: audio never reaches the image engine
        Format::Mp3
        | Format::M4a
        | Format::Flac
        | Format::Wav
        | Format::Aiff
        | Format::Ogg
        | Format::Opus => Err(OptimiseError::Unsupported("audio has no pixels".into())),
    }
}

/// The first frame of a GIF, drawn onto its transparent logical screen.
fn first_gif_frame(bytes: &[u8]) -> Result<Raster, OptimiseError> {
    let decode_err = |e: gif::DecodingError| OptimiseError::Decode(e.to_string());
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes).map_err(decode_err)?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let frame = decoder
        .read_next_frame()
        .map_err(decode_err)?
        .ok_or_else(|| OptimiseError::Decode("the GIF has no frames".into()))?;
    let (left, top) = (u32::from(frame.left), u32::from(frame.top));
    let (fw, fh) = (u32::from(frame.width), u32::from(frame.height));
    let cols = fw.min(width.saturating_sub(left));
    let mut rgba = vec![0; (width * height * 4) as usize];
    // A frame wholly outside the screen (cols == 0) draws nothing.
    let rows = if cols == 0 {
        0
    } else {
        fh.min(height.saturating_sub(top))
    };
    for row in 0..rows {
        let src = (row * fw * 4) as usize;
        let dst = (((top + row) * width + left) * 4) as usize;
        rgba[dst..dst + (cols * 4) as usize]
            .copy_from_slice(&frame.buffer[src..src + (cols * 4) as usize]);
    }
    Ok(Raster {
        width,
        height,
        rgba,
        icc: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimise::OptimiseOptions;
    use crate::optimise::webp::Extras;
    use crate::sniff::{dimensions, sniff};
    use crate::testutil;

    fn raster(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) -> Raster {
        let px = &px;
        let rgba = (0..h)
            .flat_map(|y| (0..w).flat_map(move |x| px(x, y)))
            .collect();
        Raster {
            width: w,
            height: h,
            rgba,
            icc: None,
        }
    }

    /// A 16-bit RGB PNG, which the decoder must narrow to 8 bits.
    fn png16(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = ::png::Encoder::new(&mut out, w, h);
        enc.set_color(::png::ColorType::Rgb);
        enc.set_depth(::png::BitDepth::Sixteen);
        let mut writer = enc.write_header().unwrap();
        writer
            .write_image_data(&vec![0x80; (w * h * 6) as usize])
            .unwrap();
        writer.finish().unwrap();
        out
    }

    /// A JPEG whose left half is black and right half white, tagged with `orientation`.
    fn half_dark_jpeg(w: u32, h: u32, orientation: u16) -> Vec<u8> {
        let rgb: Vec<u8> = (0..h)
            .flat_map(|_| (0..w).flat_map(move |x| if x < w / 2 { [0; 3] } else { [255; 3] }))
            .collect();
        let mut enc = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
        enc.set_size(w as usize, h as usize);
        enc.set_quality(95.0);
        let mut enc = enc.start_compress(Vec::new()).unwrap();
        enc.write_marker(
            mozjpeg::Marker::APP(1),
            &testutil::exif_with_orientation(orientation, 0),
        );
        enc.write_scanlines(&rgb).unwrap();
        enc.finish().unwrap()
    }

    fn pixel(r: &Raster, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * r.width + x) * 4) as usize;
        r.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn every_still_format_round_trips_with_its_size() {
        let o = OptimiseOptions::default();
        let sources = [
            (Format::Png, testutil::png(120, 80), (120, 80)),
            (Format::Png, png16(40, 30), (40, 30)),
            (
                Format::Jpeg,
                testutil::jpeg(120, 80, 90, &testutil::JpegExtras::default()),
                (120, 80),
            ),
            (Format::Jpeg, testutil::jpeg_gray(120, 80, 90), (120, 80)),
            (
                Format::WebP,
                testutil::webp(120, 80, 90, &Extras::default()),
                (120, 80),
            ),
            (Format::Gif, testutil::gif(3), (64, 48)),
        ];
        for (format, bytes, size) in sources {
            let r = decode(&bytes, format).unwrap();
            assert_eq!((r.width, r.height), size, "{format:?}");
            for target in [Format::Png, Format::Jpeg, Format::WebP] {
                let out = encode(&r, target, &o).unwrap();
                assert_eq!(sniff(&out), Some(target), "{format:?} → {target:?}");
                assert_eq!(
                    dimensions(&out, target),
                    Some(size),
                    "{format:?} → {target:?}"
                );
            }
        }
    }

    #[test]
    fn transparency_becomes_white_in_a_jpeg() {
        let clear = raster(16, 16, |_, _| [0, 0, 0, 0]);
        let jpeg = encode(&clear, Format::Jpeg, &OptimiseOptions::default()).unwrap();
        let back = decode(&jpeg, Format::Jpeg).unwrap();
        assert!(
            pixel(&back, 8, 8)[..3].iter().all(|&c| c >= 250),
            "{:?}",
            pixel(&back, 8, 8)
        );
    }

    #[test]
    fn the_icc_profile_survives_every_target() {
        let profile: Vec<u8> = (0..600_u32).map(|i| (i % 251) as u8).collect();
        let mut r = raster(32, 32, |x, y| [x as u8 * 8, y as u8 * 8, 90, 255]);
        r.icc = Some(profile.clone());
        for target in [Format::Png, Format::Jpeg, Format::WebP] {
            let out = encode(&r, target, &OptimiseOptions::default()).unwrap();
            assert_eq!(
                decode(&out, target).unwrap().icc,
                Some(profile.clone()),
                "{target:?}"
            );
        }
    }

    /// An APP2 payload: `ICC_PROFILE\0`, chunk 1 of 1, then a 128-byte profile
    /// whose colour space is `space`.
    fn app2_profile(space: &[u8; 4]) -> (Vec<u8>, Vec<u8>) {
        let mut profile = vec![0_u8; 128];
        profile[16..20].copy_from_slice(space);
        let mut payload = b"ICC_PROFILE\0".to_vec();
        payload.extend([1, 1]);
        payload.extend(&profile);
        (payload, profile)
    }

    #[test]
    fn a_grey_profile_is_dropped_once_the_pixels_are_rgb() {
        let (payload, _) = app2_profile(b"GRAY");
        let mut enc = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_GRAYSCALE);
        enc.set_size(16, 16);
        enc.set_quality(90.0);
        let mut enc = enc.start_compress(Vec::new()).unwrap();
        enc.write_marker(mozjpeg::Marker::APP(2), &payload);
        enc.write_scanlines(&[0x80; 16 * 16]).unwrap();
        let jpeg = enc.finish().unwrap();
        let r = decode(&jpeg, Format::Jpeg).unwrap();
        assert_eq!(r.icc, None);
        for target in [Format::Png, Format::Jpeg, Format::WebP] {
            let out = encode(&r, target, &OptimiseOptions::default()).unwrap();
            assert_eq!(decode(&out, target).unwrap().icc, None, "{target:?}");
        }
    }

    #[test]
    fn an_rgb_profile_survives_decoding() {
        let (payload, profile) = app2_profile(b"RGB ");
        let jpeg = testutil::jpeg(
            32,
            32,
            90,
            &testutil::JpegExtras {
                icc: Some(payload),
                ..Default::default()
            },
        );
        assert_eq!(decode(&jpeg, Format::Jpeg).unwrap().icc, Some(profile));
    }

    #[test]
    fn a_long_icc_profile_is_split_across_app2_segments() {
        let profile = vec![7_u8; 70_000];
        let mut r = raster(8, 8, |_, _| [10, 20, 30, 255]);
        r.icc = Some(profile.clone());
        let jpeg = encode(&r, Format::Jpeg, &OptimiseOptions::default()).unwrap();
        assert_eq!(decode(&jpeg, Format::Jpeg).unwrap().icc, Some(profile));
    }

    #[test]
    fn jpeg_orientation_is_applied_to_the_pixels() {
        // Orientation 6 means "rotate 90° clockwise to display": the dark left
        // half of the stored picture becomes the top half.
        let r = decode(&half_dark_jpeg(40, 20, 6), Format::Jpeg).unwrap();
        assert_eq!((r.width, r.height), (20, 40));
        assert!(pixel(&r, 10, 5)[0] < 60, "top is dark");
        assert!(pixel(&r, 10, 35)[0] > 200, "bottom is light");
        assert_eq!(
            display_dims(&half_dark_jpeg(40, 20, 6), Format::Jpeg),
            Some((20, 40))
        );
        // Re-encoded, it is upright and carries no orientation.
        let out = encode(&r, Format::Jpeg, &OptimiseOptions::default()).unwrap();
        assert_eq!(jpeg::orientation(&out), 1);
    }

    #[test]
    fn every_orientation_maps_the_corners() {
        // A 3×2 picture whose top-left pixel is red; orientations move it.
        let r = raster(3, 2, |x, y| {
            if (x, y) == (0, 0) {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 255]
            }
        });
        let red = |r: &Raster| {
            (0..r.height)
                .flat_map(|y| (0..r.width).map(move |x| (x, y)))
                .find(|&(x, y)| pixel(r, x, y)[0] == 255)
                .unwrap()
        };
        let cases = [
            (1, (3, 2), (0, 0)),
            (2, (3, 2), (2, 0)),
            (3, (3, 2), (2, 1)),
            (4, (3, 2), (0, 1)),
            (5, (2, 3), (0, 0)),
            (6, (2, 3), (1, 0)),
            (7, (2, 3), (1, 2)),
            (8, (2, 3), (0, 2)),
        ];
        for (orientation, size, at) in cases {
            let turned = orient(r.clone(), orientation);
            assert_eq!(
                ((turned.width, turned.height), red(&turned)),
                (size, at),
                "orientation {orientation}"
            );
        }
    }

    #[test]
    fn a_plain_crop_copies_pixels_without_resampling() {
        let r = raster(4, 2, |x, y| [x as u8, y as u8, 0, 255]);
        let cut = cut(
            &r,
            Rect {
                x: 1,
                y: 0,
                width: 2,
                height: 2,
            },
            (2, 2),
        )
        .unwrap();
        assert_eq!(
            cut.rgba,
            vec![1, 0, 0, 255, 2, 0, 0, 255, 1, 1, 0, 255, 2, 1, 0, 255]
        );
    }

    #[test]
    fn a_cut_can_also_resize() {
        let r = raster(100, 50, |x, _| [x as u8, 0, 0, 255]);
        let out = cut(
            &r,
            Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            },
            (50, 25),
        )
        .unwrap();
        assert_eq!(
            (out.width, out.height, out.rgba.len()),
            (50, 25, 50 * 25 * 4)
        );
    }

    #[test]
    fn a_header_claiming_a_huge_picture_is_refused_before_decoding() {
        assert!(matches!(
            decode(&testutil::png_claiming_size(20_000, 20_000), Format::Png),
            Err(OptimiseError::Unsupported(_))
        ));
    }

    #[test]
    fn gif_output_is_left_to_gifsicle() {
        let r = raster(2, 2, |_, _| [0, 0, 0, 255]);
        assert!(matches!(
            encode(&r, Format::Gif, &OptimiseOptions::default()),
            Err(OptimiseError::Unsupported(_))
        ));
    }

    #[test]
    fn a_gif_frame_outside_its_screen_does_not_panic() {
        let mut bytes = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut bytes, 10, 10, &[0, 0, 0, 255, 255, 255]).unwrap();
            let frame = gif::Frame {
                left: 20,
                top: 9,
                width: 1,
                height: 1,
                buffer: std::borrow::Cow::Owned(vec![1]),
                ..gif::Frame::default()
            };
            enc.write_frame(&frame).unwrap();
        }
        let r = decode(&bytes, Format::Gif).unwrap();
        assert_eq!((r.width, r.height), (10, 10));
        assert!(
            r.rgba.iter().all(|&b| b == 0),
            "nothing of the frame is on screen"
        );
    }
}
