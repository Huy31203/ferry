//! AVIF as a source: decoded with `avif-decode` (rav1d, a Rust port of dav1d).
//! Ferry cannot write AVIF yet, so an AVIF card converts to PNG, JPEG or WebP.

use super::OptimiseError;
use super::png::MAX_PIXELS;
use crate::edit::raster::Raster;

fn decode_err(e: avif_decode::Error) -> OptimiseError {
    match e {
        avif_decode::Error::Unsupported(why) => OptimiseError::Unsupported(why.into()),
        e => OptimiseError::Decode(e.to_string()),
    }
}

/// The picture as straight RGBA8. Samples of 16 bits are narrowed with `>> 8`;
/// the colour profile is not read (AVIF colour is taken as sRGB).
pub fn decode_raster(bytes: &[u8]) -> Result<Raster, OptimiseError> {
    if let Some((w, h)) = crate::isobmff::primary_size(bytes)
        && u64::from(w) * u64::from(h) > MAX_PIXELS
    {
        return Err(OptimiseError::Unsupported(format!(
            "{w} x {h} pixels is too large to edit"
        )));
    }
    let image = avif_decode::Decoder::from_avif(bytes)
        .and_then(avif_decode::Decoder::to_image)
        .map_err(decode_err)?;
    // Each variant holds an `ImgVec` of its own pixel type.
    macro_rules! flat {
        ($i:expr, $px:expr) => {
            (
                $i.width() as u32,
                $i.height() as u32,
                $i.pixels().flat_map($px).collect(),
            )
        };
    }
    let (width, height, rgba): (u32, u32, Vec<u8>) = match image {
        avif_decode::Image::Rgb8(i) => flat!(i, |p| [p.r, p.g, p.b, 255]),
        avif_decode::Image::Rgb16(i) => {
            flat!(i, |p| [
                (p.r >> 8) as u8,
                (p.g >> 8) as u8,
                (p.b >> 8) as u8,
                255
            ])
        }
        avif_decode::Image::Rgba8(i) => flat!(i, |p| [p.r, p.g, p.b, p.a]),
        avif_decode::Image::Rgba16(i) => flat!(i, |p| [
            (p.r >> 8) as u8,
            (p.g >> 8) as u8,
            (p.b >> 8) as u8,
            (p.a >> 8) as u8
        ]),
        avif_decode::Image::Gray8(i) => flat!(i, |p| [p.value(), p.value(), p.value(), 255]),
        avif_decode::Image::Gray16(i) => flat!(i, |p| {
            let v = (p.value() >> 8) as u8;
            [v, v, v, 255]
        }),
    };
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
    use crate::sniff::{self, Format};
    use crate::testutil;

    #[test]
    fn an_avif_decodes_with_its_size_and_alpha() {
        let bytes = testutil::avif(true);
        assert_eq!(sniff::sniff(&bytes), Some(Format::Avif));
        assert_eq!(sniff::dimensions(&bytes, Format::Avif), Some((32, 24)));
        let r = decode_raster(&bytes).unwrap();
        assert_eq!((r.width, r.height, r.rgba.len()), (32, 24, 32 * 24 * 4));
        assert!(
            r.rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 128),
            "the transparent corner survives"
        );
        assert!(r.rgba.as_chunks::<4>().0.iter().any(|p| p[3] == 255));
    }

    #[test]
    fn an_opaque_avif_has_no_transparency() {
        let r = decode_raster(&testutil::avif(false)).unwrap();
        assert!(r.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        // The sidebar is light grey, the content almost white.
        assert!(r.rgba[..3].iter().all(|&c| c > 200), "{:?}", &r.rgba[..4]);
    }

    #[test]
    fn junk_is_a_decode_error() {
        assert!(matches!(
            decode_raster(&testutil::ftyp(b"avif", &[b"mif1"])),
            Err(OptimiseError::Decode(_))
        ));
    }
}
