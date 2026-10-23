//! WebP (still images): decode and re-encode lossy with libwebp, method 4,
//! alpha kept. The ICC profile always survives; EXIF and XMP follow the
//! metadata setting (spec §5.8). Animations never reach this module.

use webp::{Decoder, Encoder, WebPConfig};

use super::{OptimiseError, OptimiseOptions, quality};
use crate::edit::raster::Raster;

const ICC_FLAG: u8 = 0x20;
const ALPHA_FLAG: u8 = 0x10;
const EXIF_FLAG: u8 = 0x08;
const XMP_FLAG: u8 = 0x04;

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let image = Decoder::new(input)
        .decode()
        .ok_or_else(|| OptimiseError::Decode("not a still WebP".into()))?;
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }

    let config = config(o)?;
    let encoded = Encoder::new(&image, image.layout(), image.width(), image.height())
        .encode_advanced(&config)
        .map_err(|e| OptimiseError::Encode(format!("{e:?}")))?;

    let original = chunks(input);
    let find = |name: &[u8; 4]| {
        original
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, data)| data.to_vec())
    };
    let keep_metadata = !o.strip_metadata;
    let extras = Extras {
        icc: find(b"ICCP"),
        exif: find(b"EXIF").filter(|_| keep_metadata),
        xmp: find(b"XMP ").filter(|_| keep_metadata),
    };
    Ok(assemble(
        &encoded,
        image.width(),
        image.height(),
        image.is_alpha(),
        &extras,
    ))
}

/// Lossy, method 4, alpha kept at full quality.
fn config(o: &OptimiseOptions) -> Result<WebPConfig, OptimiseError> {
    let mut config =
        WebPConfig::new().map_err(|()| OptimiseError::Encode("libwebp config".into()))?;
    config.lossless = 0;
    config.quality = f32::from(quality(o));
    config.method = 4;
    config.alpha_compression = 1;
    config.alpha_quality = 100;
    Ok(config)
}

/// A still WebP as straight RGBA, with its ICC profile.
pub(crate) fn decode_raster(input: &[u8]) -> Result<Raster, OptimiseError> {
    let image = Decoder::new(input)
        .decode()
        .ok_or_else(|| OptimiseError::Decode("not a still WebP".into()))?;
    let data: &[u8] = &image;
    let rgba = if image.is_alpha() {
        data.to_vec()
    } else {
        data.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    };
    let icc = chunks(input)
        .iter()
        .find(|(name, _)| name == b"ICCP")
        .map(|(_, data)| data.to_vec());
    Ok(Raster {
        width: image.width(),
        height: image.height(),
        rgba,
        icc,
    })
}

/// Edited pixels as a lossy WebP with the ICC profile.
pub(crate) fn encode_raster(r: &Raster, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let encoded = Encoder::from_rgba(&r.rgba, r.width, r.height)
        .encode_advanced(&config(o)?)
        .map_err(|e| OptimiseError::Encode(format!("{e:?}")))?;
    let alpha = r.rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 255);
    let extras = Extras {
        icc: r.icc.clone(),
        ..Extras::default()
    };
    Ok(assemble(&encoded, r.width, r.height, alpha, &extras))
}

#[derive(Default)]
pub(crate) struct Extras {
    pub icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

/// The chunks of a RIFF/WebP file as `(fourcc, payload)`; empty for anything else.
pub(crate) fn chunks(bytes: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return out;
    }
    let mut pos = 12;
    while let Some(head) = bytes.get(pos..pos + 8) {
        let name: [u8; 4] = head[0..4].try_into().expect("slice is 4 bytes");
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let Some(data) = bytes.get(pos + 8..pos + 8 + len) else {
            break;
        };
        out.push((name, data));
        pos += 8 + len + (len & 1); // chunks are padded to even length
    }
    out
}

/// Wrap libwebp's output in a container that carries `extras`. Without any
/// extras the encoder's own output is returned unchanged.
pub(crate) fn assemble(
    encoded: &[u8],
    width: u32,
    height: u32,
    alpha: bool,
    extras: &Extras,
) -> Vec<u8> {
    if extras.icc.is_none() && extras.exif.is_none() && extras.xmp.is_none() {
        return encoded.to_vec();
    }
    let mut flags = 0;
    if extras.icc.is_some() {
        flags |= ICC_FLAG;
    }
    if alpha {
        flags |= ALPHA_FLAG;
    }
    if extras.exif.is_some() {
        flags |= EXIF_FLAG;
    }
    if extras.xmp.is_some() {
        flags |= XMP_FLAG;
    }
    let mut vp8x = vec![flags, 0, 0, 0];
    vp8x.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    vp8x.extend_from_slice(&(height - 1).to_le_bytes()[..3]);

    let mut body = b"WEBP".to_vec();
    push_chunk(&mut body, b"VP8X", &vp8x);
    if let Some(icc) = &extras.icc {
        push_chunk(&mut body, b"ICCP", icc);
    }
    for (name, data) in chunks(encoded) {
        if &name != b"VP8X" {
            push_chunk(&mut body, &name, data); // ALPH, VP8, VP8L
        }
    }
    if let Some(exif) = &extras.exif {
        push_chunk(&mut body, b"EXIF", exif);
    }
    if let Some(xmp) = &extras.xmp {
        push_chunk(&mut body, b"XMP ", xmp);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn push_chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(name);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    fn names(webp: &[u8]) -> Vec<String> {
        chunks(webp)
            .iter()
            .map(|(n, _)| String::from_utf8_lossy(n).into_owned())
            .collect()
    }

    #[test]
    fn a_high_quality_webp_shrinks_and_keeps_its_size() {
        let input = testutil::webp(400, 300, 95, &Extras::default());
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        let decoded = Decoder::new(&out).decode().unwrap();
        assert_eq!((decoded.width(), decoded.height()), (400, 300));
    }

    #[test]
    fn aggressive_is_no_bigger_than_normal() {
        let input = testutil::webp(400, 300, 95, &Extras::default());
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
    fn transparency_survives() {
        let input = testutil::webp_with_transparent_corner(64, 64);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        let decoded = Decoder::new(&out).decode().unwrap();
        assert!(decoded.is_alpha());
        assert_eq!(decoded[3], 0, "top-left pixel stays transparent");
        assert_eq!(decoded[(32 * 64 + 32) * 4 + 3], 255, "centre stays opaque");
    }

    #[test]
    fn the_icc_profile_survives_and_exif_is_dropped_when_stripping() {
        let extras = Extras {
            icc: Some(vec![9; 101]),
            exif: Some(b"Exif\0\0II*\0".to_vec()),
            xmp: Some(b"<x:xmpmeta/>".to_vec()),
        };
        let input = testutil::webp(128, 96, 95, &extras);
        assert_eq!(names(&input), ["VP8X", "ICCP", "VP8 ", "EXIF", "XMP "]);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert_eq!(names(&out), ["VP8X", "ICCP", "VP8 "]);
        let icc = chunks(&out)
            .into_iter()
            .find(|(n, _)| n == b"ICCP")
            .unwrap()
            .1;
        assert_eq!(icc, &[9; 101][..]);
        assert!(Decoder::new(&out).decode().is_some(), "still decodes");
    }

    #[test]
    fn exif_and_xmp_are_kept_when_metadata_is_kept() {
        let extras = Extras {
            exif: Some(b"Exif\0\0II*\0".to_vec()),
            xmp: Some(b"<x:xmpmeta/>".to_vec()),
            ..Extras::default()
        };
        let input = testutil::webp(128, 96, 95, &extras);
        let o = OptimiseOptions {
            strip_metadata: false,
            ..OptimiseOptions::default()
        };
        let out = optimise(&input, &o).unwrap();
        assert_eq!(names(&out), ["VP8X", "VP8 ", "EXIF", "XMP "]);
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            optimise(b"RIFF\x04\0\0\0WEBP", &OptimiseOptions::default()),
            Err(OptimiseError::Decode(_))
        ));
    }
}
