//! HEIC through ImageIO on macOS. Apple licenses HEVC for its own codec, so
//! this reads every HEIC the system opens (grids, `irot`/`imir` transforms) and
//! writes HEIC when this Mac has an encoder.

use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::OnceLock;

use objc2_core_foundation::{
    CFArray, CFBoolean, CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString,
    CFType, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGColorSpaceModel, CGContext,
    CGDataProvider, CGImage, CGImageAlphaInfo, kCGColorSpaceSRGB,
};
use objc2_image_io::{
    CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
    kCGImageMetadataShouldExcludeGPS, kCGImageMetadataShouldExcludeXMP,
    kCGImagePropertyOrientation, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceThumbnailMaxPixelSize,
};

use crate::edit::raster::Raster;
use crate::i18n::tf;
use crate::optimise::OptimiseError;
use crate::optimise::png::MAX_PIXELS;
use crate::sniff::Format;

const HEIC_UTI: &str = "public.heic";

// SAFETY: CoreGraphics is linked by objc2-core-graphics; the signature is
// CGBitmapContextCreate's. The crate does not bind it.
unsafe extern "C-unwind" {
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: *const CGColorSpace,
        bitmap_info: u32,
    ) -> *mut CGContext;
}

fn unsupported_read(format: Format) -> OptimiseError {
    OptimiseError::Unsupported(tf("pdf.read_unsupported", &[("format", format.label())]))
}

fn unsupported_write(format: Format) -> OptimiseError {
    OptimiseError::Unsupported(tf("pdf.write_unsupported", &[("format", format.label())]))
}

fn unreadable() -> OptimiseError {
    OptimiseError::Decode("ImageIO could not read the HEIC".into())
}

/// Whether this Mac can write HEIC (its encoder is there on Apple Silicon and
/// most Intel Macs).
pub fn can_encode(format: Format) -> bool {
    static HEIC: OnceLock<bool> = OnceLock::new();
    format == Format::Heic
        && *HEIC.get_or_init(|| {
            // SAFETY: no arguments; the array holds CFStrings.
            let types = unsafe { CGImageDestination::type_identifiers() };
            // SAFETY: CGImageDestinationCopyTypeIdentifiers returns an array of CFStrings.
            let types = unsafe { CFRetained::cast_unchecked::<CFArray<CFString>>(types) };
            types.iter().any(|t| t.to_string() == HEIC_UTI)
        })
}

fn source(bytes: &[u8]) -> Option<CFRetained<CGImageSource>> {
    let data = CFData::from_bytes(bytes);
    // SAFETY: no options.
    unsafe { CGImageSource::with_data(&data, None) }.filter(|s| {
        // SAFETY: a live source.
        unsafe { s.count() > 0 }
    })
}

/// The stored size (before orientation) of image `index`.
fn pixel_size(source: &CGImageSource, index: usize) -> Option<(u32, u32)> {
    // SAFETY: no options.
    let props = unsafe { source.properties_at_index(index, None) }?;
    // SAFETY: ImageIO property dictionaries map CFString keys to CFType values.
    let props = unsafe { CFRetained::cast_unchecked::<CFDictionary<CFString, CFNumber>>(props) };
    // SAFETY: the keys are ImageIO's own constants; the values are CFNumbers.
    let side = |key: &CFString| unsafe { props.get_unchecked(key) }?.as_i64();
    // SAFETY: reading ImageIO's constants.
    let (w, h) = unsafe {
        (
            side(kCGImagePropertyPixelWidth)?,
            side(kCGImagePropertyPixelHeight)?,
        )
    };
    Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?))
}

fn dictionary(entries: &[(&CFString, &CFType)]) -> CFRetained<CFDictionary<CFString, CFType>> {
    let (keys, values): (Vec<&CFString>, Vec<&CFType>) = entries.iter().copied().unzip();
    CFDictionary::from_slices(&keys, &values)
}

fn srgb() -> Option<CFRetained<CGColorSpace>> {
    // SAFETY: reading a CoreGraphics constant.
    CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
}

/// The picture of the HEIC's primary image, oriented, with its colour profile.
pub fn decode(bytes: &[u8], format: Format) -> Result<Raster, OptimiseError> {
    if format != Format::Heic {
        return Err(unsupported_read(format));
    }
    let source = source(bytes).ok_or_else(unreadable)?;
    // SAFETY: a live source.
    let index = unsafe { source.primary_image_index() };
    let (w, h) = pixel_size(&source, index).ok_or_else(unreadable)?;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(OptimiseError::Unsupported(format!(
            "{w} x {h} pixels is too large to edit"
        )));
    }
    let max_side = CFNumber::new_i64(i64::from(w.max(h)));
    // SAFETY: reading ImageIO's constants.
    let options = unsafe {
        dictionary(&[
            (
                kCGImageSourceCreateThumbnailFromImageAlways,
                CFBoolean::new(true).as_ref(),
            ),
            (
                kCGImageSourceCreateThumbnailWithTransform,
                CFBoolean::new(true).as_ref(),
            ),
            (kCGImageSourceThumbnailMaxPixelSize, max_side.as_ref()),
        ])
    };
    // SAFETY: `options` holds ImageIO's documented keys and values. The thumbnail at
    // the full size is the whole picture with its orientation applied.
    let image = unsafe { source.thumbnail_at_index(index, Some(options.as_opaque())) }
        .ok_or_else(unreadable)?;
    draw(&image)
}

/// The image as straight RGBA8, in its own RGB colour space when it has one.
fn draw(image: &CGImage) -> Result<Raster, OptimiseError> {
    let (w, h) = (CGImage::width(Some(image)), CGImage::height(Some(image)));
    if w == 0 || h == 0 {
        return Err(unreadable());
    }
    let own = CGImage::color_space(Some(image))
        .filter(|s| CGColorSpace::model(Some(s)) == CGColorSpaceModel::RGB);
    let mut rgba = vec![0_u8; w * h * 4];
    let mut paint = |space: &CGColorSpace| -> bool {
        // SAFETY: `rgba` outlives the context and has `w * h * 4` bytes.
        let raw = unsafe {
            CGBitmapContextCreate(
                rgba.as_mut_ptr().cast(),
                w,
                h,
                8,
                w * 4,
                space,
                CGImageAlphaInfo::PremultipliedLast.0,
            )
        };
        // SAFETY: a Create function returns a retained context, or null.
        let Some(context) = NonNull::new(raw).map(|c| unsafe { CFRetained::from_raw(c) }) else {
            return false;
        };
        let rect = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: w as f64,
                height: h as f64,
            },
        };
        CGContext::draw_image(Some(&context), rect, Some(image));
        true
    };
    // Some spaces (extended range, HDR) cannot back a bitmap: sRGB takes over.
    let used = match own {
        Some(space) if paint(&space) => Some(space),
        _ => {
            let space = srgb().ok_or_else(unreadable)?;
            if !paint(&space) {
                return Err(unreadable());
            }
            None
        }
    };
    for p in rgba.as_chunks_mut::<4>().0 {
        match p[3] {
            255 => {}
            0 => *p = [0; 4],
            a => {
                for c in &mut p[..3] {
                    *c = ((u16::from(*c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8;
                }
            }
        }
    }
    let icc = used
        .and_then(|space| CGColorSpace::icc_data(Some(&space)))
        .map(|data| data.to_vec());
    Ok(Raster {
        width: w as u32,
        height: h as u32,
        rgba,
        icc,
    })
}

/// Pixels as a HEIC at `quality` (5..=100).
pub fn encode(r: &Raster, format: Format, quality: u8) -> Result<Vec<u8>, OptimiseError> {
    encode_oriented(r, format, quality, None)
}

/// `encode`, tagging the picture with an EXIF `orientation` (tests use it).
pub(crate) fn encode_oriented(
    r: &Raster,
    format: Format,
    quality: u8,
    orientation: Option<u16>,
) -> Result<Vec<u8>, OptimiseError> {
    if format != Format::Heic || !can_encode(format) {
        return Err(unsupported_write(format));
    }
    let encode_err =
        |why: &str| OptimiseError::Encode(format!("ImageIO could not write the HEIC: {why}"));
    let space = r
        .icc
        .as_deref()
        .and_then(|icc| {
            let data = CFData::from_bytes(icc);
            // SAFETY: a CFData of ICC bytes; ImageIO returns null for a bad profile.
            unsafe { CGColorSpace::with_icc_data(Some(data.as_ref())) }
        })
        .or_else(srgb)
        .ok_or_else(|| encode_err("no colour space"))?;
    let provider = CGDataProvider::with_cf_data(Some(&CFData::from_bytes(&r.rgba)))
        .ok_or_else(|| encode_err("no pixels"))?;
    let (w, h) = (r.width as usize, r.height as usize);
    // SAFETY: straight RGBA8, `w * 4` bytes a row, no decode array.
    let image = unsafe {
        CGImage::new(
            w,
            h,
            8,
            32,
            w * 4,
            Some(&space),
            CGBitmapInfo(CGImageAlphaInfo::Last.0),
            Some(&provider),
            ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(|| encode_err("no image"))?;
    let quality = CFNumber::new_f64(f64::from(quality) / 100.0);
    let turned = orientation.map(|o| CFNumber::new_i32(i32::from(o)));
    // SAFETY: reading ImageIO's constants.
    let props = unsafe {
        let mut entries = vec![(kCGImageDestinationLossyCompressionQuality, quality.as_ref())];
        if let Some(turned) = &turned {
            entries.push((kCGImagePropertyOrientation, turned.as_ref()));
        }
        dictionary(&entries)
    };
    let out = CFMutableData::new(None, 0).ok_or_else(|| encode_err("no buffer"))?;
    let dest = destination(&out).ok_or_else(|| encode_err("no encoder"))?;
    // SAFETY: a live destination expecting one image.
    unsafe { dest.add_image(&image, Some(props.as_opaque())) };
    // SAFETY: the image was added.
    if !unsafe { dest.finalize() } {
        return Err(encode_err("the encoder failed"));
    }
    Ok(out.to_vec())
}

/// The HEIC again at `quality`, from its own primary image, so orientation,
/// colour and (unless `strip_metadata`) the other metadata stay.
pub fn reencode(
    bytes: &[u8],
    format: Format,
    quality: u8,
    strip_metadata: bool,
) -> Result<Vec<u8>, OptimiseError> {
    if format != Format::Heic || !can_encode(format) {
        return Err(unsupported_write(format));
    }
    let encode_err =
        |why: &str| OptimiseError::Encode(format!("ImageIO could not write the HEIC: {why}"));
    let source = source(bytes).ok_or_else(unreadable)?;
    // SAFETY: a live source.
    let index = unsafe { source.primary_image_index() };
    let quality = CFNumber::new_f64(f64::from(quality) / 100.0);
    let strip = CFBoolean::new(strip_metadata);
    // SAFETY: reading ImageIO's constants.
    let props = unsafe {
        dictionary(&[
            (kCGImageDestinationLossyCompressionQuality, quality.as_ref()),
            (kCGImageMetadataShouldExcludeGPS, strip.as_ref()),
            (kCGImageMetadataShouldExcludeXMP, strip.as_ref()),
        ])
    };
    let out = CFMutableData::new(None, 0).ok_or_else(|| encode_err("no buffer"))?;
    let dest = destination(&out).ok_or_else(|| encode_err("no encoder"))?;
    // SAFETY: `index` is the source's primary image.
    unsafe { dest.add_image_from_source(&source, index, Some(props.as_opaque())) };
    // SAFETY: the image was added.
    if !unsafe { dest.finalize() } {
        return Err(encode_err("the encoder failed"));
    }
    Ok(out.to_vec())
}

fn destination(out: &CFMutableData) -> Option<CFRetained<CGImageDestination>> {
    // SAFETY: one image of type HEIC, no options.
    unsafe { CGImageDestination::with_data(out, &CFString::from_static_str(HEIC_UTI), 1, None) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Edits;
    use crate::optimise::OptimiseOptions;
    use crate::testutil;

    #[test]
    fn a_heic_round_trips_with_its_size() {
        let Some(bytes) = testutil::heic(120, 80, 1) else {
            return;
        };
        assert_eq!(crate::sniff::sniff(&bytes), Some(Format::Heic));
        let r = decode(&bytes, Format::Heic).unwrap();
        assert_eq!((r.width, r.height, r.rgba.len()), (120, 80, 120 * 80 * 4));
        assert!(r.icc.is_some(), "an RGB HEIC carries its colour profile");
    }

    #[test]
    fn orientation_is_applied_to_the_pixels() {
        let Some(bytes) = testutil::heic(120, 80, 6) else {
            return;
        };
        let r = decode(&bytes, Format::Heic).unwrap();
        assert_eq!((r.width, r.height), (80, 120));
    }

    #[test]
    fn a_heic_converts_to_jpeg_through_the_recipe() {
        let Some(bytes) = testutil::heic(120, 80, 1) else {
            return;
        };
        let edits = Edits {
            format: Some(Format::Jpeg),
            ..Edits::default()
        };
        let a =
            crate::edit::apply(&bytes, Format::Heic, &edits, &OptimiseOptions::default()).unwrap();
        assert_eq!(
            (a.format, a.dims, crate::sniff::sniff(&a.bytes)),
            (Format::Jpeg, (120, 80), Some(Format::Jpeg))
        );
    }

    #[test]
    fn a_lower_quality_heic_is_smaller() {
        let Some(bytes) = testutil::heic(400, 300, 1) else {
            return;
        };
        let high = reencode(&bytes, Format::Heic, 95, true).unwrap();
        let low = reencode(&bytes, Format::Heic, 30, true).unwrap();
        assert!(low.len() < high.len());
    }

    #[test]
    fn transparency_survives_a_round_trip() {
        if !can_encode(Format::Heic) {
            return;
        }
        let mut rgba = [200, 30, 30, 255].repeat(32 * 32);
        for p in rgba.as_chunks_mut::<4>().0.iter_mut().take(32 * 8) {
            *p = [0, 0, 0, 0];
        }
        let r = Raster {
            width: 32,
            height: 32,
            rgba,
            icc: None,
        };
        let back = decode(&encode(&r, Format::Heic, 90).unwrap(), Format::Heic).unwrap();
        assert!(
            back.rgba[3] < 128,
            "the top row stays clear: {:?}",
            &back.rgba[..4]
        );
        assert!(back.rgba[(32 * 20 * 4) + 3] > 200);
    }

    #[test]
    fn garbage_is_a_decode_error_not_a_crash() {
        assert!(matches!(
            decode(&testutil::ftyp(b"heic", &[b"mif1"]), Format::Heic),
            Err(OptimiseError::Decode(_))
        ));
    }

    #[test]
    fn other_formats_are_not_this_codecs() {
        assert!(matches!(
            decode(b"x", Format::Avif),
            Err(OptimiseError::Unsupported(_))
        ));
        assert!(!can_encode(Format::Avif));
    }

    #[test]
    fn macos_ci_can_encode_heic() {
        if std::env::var_os("CI").is_some() {
            assert!(
                can_encode(Format::Heic),
                "GitHub's macOS runners have the HEVC encoder"
            );
        }
    }
}
