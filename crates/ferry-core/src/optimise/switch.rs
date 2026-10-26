//! PNG to JPEG and back (`images.switch_png_jpeg`): a plain run takes the other
//! format when it comes out at least 10% smaller than the same-format result.

use super::{OptimiseError, OptimiseOptions, Outcome, adaptive};
use crate::{edit::raster, sniff::Format};

/// `super::optimise`, except that with `enabled` a still PNG (every pixel
/// opaque) or JPEG may come back as the other format.
pub fn optimise(
    input: &[u8],
    format: Format,
    o: &OptimiseOptions,
    enabled: bool,
) -> Result<(Outcome, Format), OptimiseError> {
    let own = super::optimise(input, format, o)?;
    let other = match format {
        Format::Png => Format::Jpeg,
        Format::Jpeg => Format::Png,
        _ => return Ok((own, format)),
    };
    if !enabled || crate::sniff::is_animated(input, format) {
        return Ok((own, format));
    }
    let Ok(pixels) = raster::decode(input, format) else {
        return Ok((own, format));
    };
    // JPEG has no alpha: a PNG with any transparency stays a PNG.
    if format == Format::Png && pixels.rgba.chunks_exact(4).any(|p| p[3] != 255) {
        return Ok((own, format));
    }
    let bytes = if adaptive::applies(other, o) {
        adaptive::search(&pixels, other, o, |opts| {
            raster::encode(&pixels, other, opts)
        })?
        .bytes
    } else {
        raster::encode(&pixels, other, o)?
    };
    let own_len = match &own {
        Outcome::Smaller { bytes } => bytes.len(),
        _ => input.len(),
    };
    // At least 10% under the same-format result, and 1% under the input.
    if bytes.len() * 100 <= own_len * 90 && bytes.len() * 100 <= input.len() * 99 {
        return Ok((Outcome::Smaller { bytes }, other));
    }
    Ok((own, format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{edit::raster::Raster, testutil};

    #[test]
    fn an_opaque_photo_png_becomes_jpeg() {
        let (outcome, format) = optimise(
            &testutil::photo_png(256, 256),
            Format::Png,
            &OptimiseOptions::default(),
            true,
        )
        .unwrap();
        assert!(matches!(outcome, Outcome::Smaller { .. }));
        assert_eq!(format, Format::Jpeg);
    }

    #[test]
    fn a_png_with_any_transparency_stays_png() {
        let mut rgba = testutil::photo_rgba(128, 128);
        // A lossy palette would round a lone 254 up to opaque, so a whole corner goes translucent.
        for y in 0..16 {
            for x in 0..16 {
                rgba[(y * 128 + x) * 4 + 3] = 200;
            }
        }
        let png = raster::encode(
            &Raster {
                width: 128,
                height: 128,
                rgba,
                icc: None,
            },
            Format::Png,
            &OptimiseOptions {
                compression: 100,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            optimise(&png, Format::Png, &OptimiseOptions::default(), true)
                .unwrap()
                .1,
            Format::Png
        );
    }

    #[test]
    fn a_flat_four_colour_jpeg_becomes_png() {
        let flat: Vec<u8> = (0..256 * 256)
            .flat_map(|i| {
                let c = [[255, 0, 0], [0, 128, 255], [255, 255, 255], [20, 20, 20]]
                    [(i % 256 / 64) as usize];
                [c[0], c[1], c[2], 255]
            })
            .collect();
        let jpeg = raster::encode(
            &Raster {
                width: 256,
                height: 256,
                rgba: flat,
                icc: None,
            },
            Format::Jpeg,
            &OptimiseOptions {
                compression: 95,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            optimise(&jpeg, Format::Jpeg, &OptimiseOptions::default(), true)
                .unwrap()
                .1,
            Format::Png
        );
    }

    #[test]
    fn off_never_changes_the_format_and_gif_webp_never_switch() {
        let png = testutil::photo_png(256, 256);
        assert_eq!(
            optimise(&png, Format::Png, &OptimiseOptions::default(), false)
                .unwrap()
                .1,
            Format::Png
        );
        let webp = testutil::webp(200, 200, 95, &Default::default());
        assert_eq!(
            optimise(&webp, Format::WebP, &OptimiseOptions::default(), true)
                .unwrap()
                .1,
            Format::WebP
        );
    }
}
