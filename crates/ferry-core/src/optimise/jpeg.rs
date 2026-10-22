//! JPEG: decode and re-encode with mozjpeg (progressive, optimised Huffman).
//! The ICC profile and pixel density always survive; EXIF follows the
//! metadata setting but never loses the orientation (spec §5.8).

use std::os::raw::c_int;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use mozjpeg::{ColorSpace, Compress, Decompress, Marker};
use mozjpeg_sys::{
    JWRN_HIT_MARKER, JWRN_HUFF_BAD_CODE, JWRN_JPEG_EOF, JWRN_MUST_RESYNC, jpeg_common_struct,
    jpeg_error_mgr, jpeg_std_error,
};

use super::{OptimiseError, OptimiseOptions, quality};

const EXIF_PREFIX: &[u8] = b"Exif\0\0";
const ICC_PREFIX: &[u8] = b"ICC_PROFILE\0";

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    if !reaches_eoi(input) {
        return Err(OptimiseError::Decode("the JPEG is incomplete".into()));
    }
    // mozjpeg reports corrupt data by unwinding; turn that into an error.
    catch_unwind(AssertUnwindSafe(|| run(input, o)))
        .map_err(|_| OptimiseError::Decode("the JPEG is corrupt".into()))?
}

/// Whether the marker structure reaches the end-of-image marker. libjpeg pads a
/// truncated picture with grey and only warns, so without this check a
/// half-downloaded file would be "optimised" into a damaged one.
fn reaches_eoi(data: &[u8]) -> bool {
    let mut pos = 2; // past SOI
    loop {
        let (Some(&0xFF), Some(&marker)) = (data.get(pos), data.get(pos + 1)) else {
            return false;
        };
        match marker {
            0xD9 => return true,
            0xFF => pos += 1,                      // fill byte
            0x00 | 0x01 | 0xD0..=0xD8 => pos += 2, // markers without a length
            _ => {
                let Some(len) = data.get(pos + 2..pos + 4) else {
                    return false;
                };
                pos += 2 + usize::from(u16::from_be_bytes([len[0], len[1]]));
                if marker == 0xDA {
                    // The entropy-coded scan runs to the next marker that is not
                    // byte stuffing (FF00) or a restart (FFD0..FFD7).
                    let rest = data.get(pos..).unwrap_or_default();
                    let Some(end) = rest.windows(2).position(|w| {
                        w[0] == 0xFF && w[1] != 0x00 && !(0xD0..=0xD7).contains(&w[1])
                    }) else {
                        return false;
                    };
                    pos += end;
                }
            }
        }
    }
}

/// libjpeg only warns about damaged entropy data and fills the lost pixels in,
/// and mozjpeg's default error manager discards the warnings. This one unwinds
/// on the warnings that mean lost pixels, like a fatal error. Harmless ones, such
/// as extra bytes before the end marker, which cameras leave, stay silent.
fn strict_error_mgr() -> jpeg_error_mgr {
    // SAFETY: `jpeg_std_error` fills in a zeroed manager, as libjpeg expects.
    unsafe {
        let mut err: jpeg_error_mgr = std::mem::zeroed();
        jpeg_std_error(&mut err);
        err.error_exit = Some(fatal);
        err.emit_message = Some(warning);
        err
    }
}

extern "C-unwind" fn fatal(_: &mut jpeg_common_struct) {
    resume_unwind(Box::new("libjpeg fatal error"));
}

extern "C-unwind" fn warning(cinfo: &mut jpeg_common_struct, level: c_int) {
    // Level -1 is a warning; the rest are traces.
    if level >= 0 {
        return;
    }
    // SAFETY: libjpeg points `err` at the live manager before it reports anything.
    let code = unsafe { (*cinfo.err).msg_code };
    if matches!(
        code,
        JWRN_HIT_MARKER | JWRN_HUFF_BAD_CODE | JWRN_JPEG_EOF | JWRN_MUST_RESYNC
    ) {
        resume_unwind(Box::new("damaged JPEG data"));
    }
}

fn run(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let decode_err = |e: std::io::Error| OptimiseError::Decode(e.to_string());
    let encode_err = |e: std::io::Error| OptimiseError::Encode(e.to_string());

    let mut decoder = Decompress::with_err(strict_error_mgr())
        .with_markers(&[Marker::APP(1), Marker::APP(2)])
        .from_mem(input)
        .map_err(decode_err)?;
    let (width, height) = decoder.size();
    let density = decoder.pixel_density();
    let mut app1 = Vec::new();
    let mut icc = Vec::new();
    for m in decoder.markers() {
        match m.marker {
            Marker::APP(1) => app1.push(m.data.to_vec()),
            Marker::APP(2) if m.data.starts_with(ICC_PREFIX) => icc.push(m.data.to_vec()),
            _ => {}
        }
    }

    let (pixels, space) = match decoder.color_space() {
        ColorSpace::JCS_GRAYSCALE => (
            decoder
                .grayscale()
                .and_then(|mut d| d.read_scanlines::<u8>())
                .map_err(decode_err)?,
            ColorSpace::JCS_GRAYSCALE,
        ),
        ColorSpace::JCS_CMYK | ColorSpace::JCS_YCCK => {
            return Err(OptimiseError::Unsupported("CMYK JPEG".into()));
        }
        _ => (
            decoder
                .rgb()
                .and_then(|mut d| d.read_scanlines::<u8>())
                .map_err(decode_err)?,
            ColorSpace::JCS_RGB,
        ),
    };
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }

    let mut encoder = Compress::new(space);
    encoder.set_size(width, height);
    encoder.set_quality(f32::from(quality(o)));
    encoder.set_progressive_mode();
    encoder.set_optimize_coding(true);
    if let Some(density) = density {
        encoder.set_pixel_density(density);
    }
    let mut encoder = encoder.start_compress(Vec::new()).map_err(encode_err)?;
    for segment in &icc {
        encoder.write_marker(Marker::APP(2), segment);
    }
    for segment in exif_segments(&app1, o.strip_metadata) {
        encoder.write_marker(Marker::APP(1), &segment);
    }
    encoder.write_scanlines(&pixels).map_err(encode_err)?;
    encoder.finish().map_err(encode_err)
}

/// The APP1 segments to write: all of them when keeping metadata, otherwise a
/// minimal EXIF that holds only a non-default orientation.
fn exif_segments(original: &[Vec<u8>], strip_metadata: bool) -> Vec<Vec<u8>> {
    if !strip_metadata {
        return original.to_vec();
    }
    original
        .iter()
        .find_map(|segment| exif_orientation(segment))
        .filter(|&orientation| orientation != 1)
        .map(|orientation| vec![minimal_exif(orientation)])
        .unwrap_or_default()
}

/// The Orientation tag (0x0112) of an EXIF APP1 segment, if it has one.
fn exif_orientation(segment: &[u8]) -> Option<u16> {
    let tiff = segment.strip_prefix(EXIF_PREFIX)?;
    let big_endian = match tiff.get(..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let b: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let ifd = u32_at(4)? as usize;
    (0..usize::from(u16_at(ifd)?))
        .map(|i| ifd + 2 + i * 12)
        .find(|&entry| u16_at(entry) == Some(0x0112))
        .and_then(|entry| u16_at(entry + 8)) // a SHORT value sits in the first two value bytes
}

/// An EXIF APP1 payload with a single Orientation entry (big-endian TIFF).
fn minimal_exif(orientation: u16) -> Vec<u8> {
    let mut out = EXIF_PREFIX.to_vec();
    out.extend_from_slice(b"MM\0\x2A\0\0\0\x08"); // byte order, magic, IFD0 at offset 8
    out.extend_from_slice(&[0, 1]); // one entry
    out.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1]); // Orientation, SHORT, count 1
    out.extend_from_slice(&orientation.to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // value padding, no next IFD
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    fn markers(jpeg: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let d = Decompress::with_markers(mozjpeg::ALL_MARKERS)
            .from_mem(jpeg)
            .unwrap();
        d.markers()
            .map(|m| {
                let n = match m.marker {
                    Marker::APP(n) => n,
                    Marker::COM => 255,
                };
                (n, m.data.to_vec())
            })
            .collect()
    }

    #[test]
    fn a_high_quality_jpeg_shrinks_and_keeps_its_size() {
        let input = testutil::jpeg(400, 300, 95, &testutil::JpegExtras::default());
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        let d = Decompress::new_mem(&out).unwrap();
        assert_eq!(d.size(), (400, 300));
    }

    #[test]
    fn aggressive_is_no_bigger_than_normal() {
        let input = testutil::jpeg(400, 300, 95, &testutil::JpegExtras::default());
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
    fn stripping_exif_keeps_only_the_orientation() {
        let exif = testutil::exif_with_orientation(6, 4000);
        let input = testutil::jpeg(
            200,
            100,
            95,
            &testutil::JpegExtras {
                exif: Some(exif),
                ..Default::default()
            },
        );
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        let app1: Vec<_> = markers(&out).into_iter().filter(|(n, _)| *n == 1).collect();
        assert_eq!(app1.len(), 1);
        assert_eq!(exif_orientation(&app1[0].1), Some(6));
        assert!(
            app1[0].1.len() < 40,
            "kept {} bytes of EXIF",
            app1[0].1.len()
        );
    }

    #[test]
    fn no_exif_is_written_for_the_default_orientation() {
        let exif = testutil::exif_with_orientation(1, 100);
        let input = testutil::jpeg(
            200,
            100,
            95,
            &testutil::JpegExtras {
                exif: Some(exif),
                ..Default::default()
            },
        );
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(markers(&out).iter().all(|(n, _)| *n != 1));
    }

    #[test]
    fn exif_is_copied_untouched_when_metadata_is_kept() {
        let exif = testutil::exif_with_orientation(3, 500);
        let input = testutil::jpeg(
            200,
            100,
            95,
            &testutil::JpegExtras {
                exif: Some(exif.clone()),
                ..Default::default()
            },
        );
        let o = OptimiseOptions {
            strip_metadata: false,
            ..OptimiseOptions::default()
        };
        let out = optimise(&input, &o).unwrap();
        assert!(markers(&out).contains(&(1, exif)));
    }

    #[test]
    fn the_icc_profile_and_density_survive() {
        let icc = {
            let mut p = ICC_PREFIX.to_vec();
            p.extend_from_slice(&[1, 1]);
            p.extend_from_slice(&[7; 300]);
            p
        };
        let input = testutil::jpeg(
            200,
            100,
            95,
            &testutil::JpegExtras {
                icc: Some(icc.clone()),
                dpi: Some(144),
                ..Default::default()
            },
        );
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(markers(&out).contains(&(2, icc)));
        let mut d = Decompress::new_mem(&out).unwrap();
        let density = d.pixel_density().unwrap();
        assert_eq!((density.x, density.y), (144, 144));
    }

    #[test]
    fn a_grayscale_jpeg_stays_grayscale() {
        let input = testutil::jpeg_gray(200, 100, 95);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        let d = Decompress::new_mem(&out).unwrap();
        assert_eq!(d.color_space(), ColorSpace::JCS_GRAYSCALE);
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            optimise(
                &[0xFF, 0xD8, 0xFF, 0x00, 0x01, 0x02],
                &OptimiseOptions::default()
            ),
            Err(OptimiseError::Decode(_))
        ));
    }

    #[test]
    fn a_truncated_jpeg_is_refused_not_padded_with_grey() {
        let full = testutil::jpeg(300, 200, 90, &testutil::JpegExtras::default());
        for cut in [full.len() / 2, full.len() - 3, full.len() - 1] {
            let err = optimise(&full[..cut], &OptimiseOptions::default()).unwrap_err();
            assert!(
                matches!(&err, OptimiseError::Decode(m) if m.contains("incomplete")),
                "cut at {cut}: {err}"
            );
        }
    }

    #[test]
    fn bytes_after_the_end_marker_do_not_matter() {
        let mut full = testutil::jpeg(300, 200, 95, &testutil::JpegExtras::default());
        full.extend_from_slice(&[0; 64]);
        assert!(optimise(&full, &OptimiseOptions::default()).is_ok());
    }

    #[test]
    fn an_end_marker_inside_a_segment_does_not_count() {
        // SOI, then an APP1 whose payload contains FF D9 (like an EXIF thumbnail), then nothing.
        let truncated = [0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x06, 0xFF, 0xD9, 0x00, 0x00];
        assert!(!reaches_eoi(&truncated));
    }

    #[test]
    fn our_own_multi_scan_output_reaches_the_end_marker() {
        let full = testutil::jpeg(300, 200, 95, &testutil::JpegExtras::default());
        let out = optimise(&full, &OptimiseOptions::default()).unwrap();
        assert!(reaches_eoi(&out));
    }

    #[test]
    fn damage_inside_the_scan_is_refused_even_when_an_end_marker_follows() {
        let full = testutil::jpeg(300, 200, 90, &testutil::JpegExtras::default());
        // The first half of the picture plus a closing FF D9: the marker walk
        // reaches the end, but libjpeg would fill the rest with grey.
        let mut halved = full[..full.len() / 2].to_vec();
        halved.extend_from_slice(&[0xFF, 0xD9]);
        // A run of zeroed bytes in the middle of the scan throws the decoder out of step.
        let mut zeroed = full.clone();
        let mid = full.len() / 2;
        zeroed[mid..mid + 2000].fill(0);
        for (what, damaged) in [
            ("half plus an end marker", halved),
            ("zeroed bytes", zeroed),
        ] {
            let result = optimise(&damaged, &OptimiseOptions::default());
            assert!(
                matches!(result, Err(OptimiseError::Decode(_))),
                "{what}: {:?}",
                result.map(|out| out.len())
            );
        }
    }

    #[test]
    fn extra_bytes_before_the_end_marker_are_tolerated() {
        // Cameras and some editors leave stray bytes there; the picture is intact.
        let full = testutil::jpeg(300, 200, 90, &testutil::JpegExtras::default());
        let mut padded = full[..full.len() - 2].to_vec();
        padded.extend_from_slice(&[0x12, 0x34, 0x56, 0xFF, 0xD9]);
        assert!(optimise(&padded, &OptimiseOptions::default()).is_ok());
    }

    #[test]
    fn a_progressive_jpeg_is_still_accepted() {
        let full = testutil::jpeg(300, 200, 90, &testutil::JpegExtras::default());
        let progressive = optimise(&full, &OptimiseOptions::default()).unwrap();
        assert!(optimise(&progressive, &OptimiseOptions::default()).is_ok());
    }
}
