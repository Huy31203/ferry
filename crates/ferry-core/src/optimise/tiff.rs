//! TIFF through the `tiff` crate: every page decoded to RGBA with its
//! orientation applied and its ICC profile kept.

use std::io::Cursor;

use tiff::ColorType;
use tiff::TiffError;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;

use super::OptimiseError;
use super::png::MAX_PIXELS;
use crate::edit::raster::{Raster, orient};
use crate::i18n::t;

fn unsupported() -> OptimiseError {
    OptimiseError::Unsupported(t("pdf.tiff.unsupported"))
}

fn decode_err(e: TiffError) -> OptimiseError {
    match e {
        TiffError::UnsupportedError(_) => unsupported(),
        e => OptimiseError::Decode(e.to_string()),
    }
}

fn open(bytes: &[u8]) -> Result<Decoder<Cursor<&[u8]>>, OptimiseError> {
    let mut limits = Limits::default();
    // A CMYK page of MAX_PIXELS at 8 bytes a pixel is the biggest we accept.
    limits.decoding_buffer_size = MAX_PIXELS as usize * 8;
    Decoder::new(Cursor::new(bytes))
        .map(|d| d.with_limits(limits))
        .map_err(decode_err)
}

/// Every page as straight RGBA8, oriented by its tag 274, carrying its ICC
/// profile (tag 34675).
#[allow(dead_code)] // the multi-page edits of the next commit
pub fn decode_pages(bytes: &[u8]) -> Result<Vec<Raster>, OptimiseError> {
    let mut decoder = open(bytes)?;
    let mut pages = vec![decode_page(&mut decoder)?];
    while decoder.more_images() {
        decoder.next_image().map_err(decode_err)?;
        pages.push(decode_page(&mut decoder)?);
    }
    Ok(pages)
}

/// The first page only; the others are not decoded.
pub fn decode_raster(bytes: &[u8]) -> Result<Raster, OptimiseError> {
    decode_page(&mut open(bytes)?)
}

/// The current page. Samples of 16 bits are narrowed with `>> 8`; grey
/// becomes RGB; CMYK is converted naively (no colour management).
fn decode_page(decoder: &mut Decoder<Cursor<&[u8]>>) -> Result<Raster, OptimiseError> {
    let (width, height) = decoder.dimensions().map_err(decode_err)?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(unsupported());
    }
    let colour = decoder.colortype().map_err(decode_err)?;
    let orientation = decoder
        .find_tag_unsigned::<u16>(Tag::Orientation)
        .ok()
        .flatten()
        .unwrap_or(1);
    let icc = decoder
        .find_tag_unsigned_vec::<u8>(Tag::IccProfile)
        .ok()
        .flatten();
    let samples = match decoder.read_image().map_err(decode_err)? {
        DecodingResult::U8(v) => v,
        DecodingResult::U16(v) => v.into_iter().map(|s| (s >> 8) as u8).collect(),
        _ => return Err(unsupported()),
    };
    let rgba = to_rgba(colour, &samples).ok_or_else(unsupported)?;
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(OptimiseError::Decode("the TIFF page is incomplete".into()));
    }
    Ok(orient(
        Raster {
            width,
            height,
            rgba,
            icc,
        },
        orientation,
    ))
}

/// 8-bit samples (16-bit ones already narrowed) as RGBA.
fn to_rgba(colour: ColorType, s: &[u8]) -> Option<Vec<u8>> {
    Some(match colour {
        ColorType::Gray(8 | 16) => s.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        ColorType::GrayA(8 | 16) => s
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        ColorType::RGB(8 | 16) => s
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        ColorType::RGBA(8 | 16) => s.to_vec(),
        ColorType::CMYK(8) => s
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let k = u16::from(p[3]);
                let channel = |c: u8| 255 - (u16::from(c) + k).min(255) as u8;
                [channel(p[0]), channel(p[1]), channel(p[2]), 255]
            })
            .collect(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, TiffColour, TiffPage};

    #[test]
    fn every_supported_colour_type_reads_as_rgba() {
        for colour in [
            TiffColour::Rgb8,
            TiffColour::Rgba8,
            TiffColour::Gray8,
            TiffColour::Gray16,
            TiffColour::Cmyk8,
        ] {
            let r = decode_raster(&testutil::tiff(&[TiffPage {
                colour,
                ..TiffPage::default()
            }]))
            .unwrap();
            assert_eq!(
                (r.width, r.height, r.rgba.len()),
                (64, 48, 64 * 48 * 4),
                "{colour:?}"
            );
        }
    }

    #[test]
    fn orientation_and_icc_are_honoured_per_page() {
        let icc = vec![7_u8; 300];
        let bytes = testutil::tiff(&[
            TiffPage {
                orientation: 6,
                icc: Some(icc.clone()),
                ..TiffPage::default()
            },
            TiffPage {
                size: (10, 20),
                ..TiffPage::default()
            },
        ]);
        let pages = decode_pages(&bytes).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(
            (pages[0].width, pages[0].height, pages[0].icc.clone()),
            (48, 64, Some(icc))
        );
        assert_eq!((pages[1].width, pages[1].height), (10, 20));
    }

    #[test]
    fn a_cut_short_tiff_is_a_decode_error() {
        let bytes = testutil::tiff(&[TiffPage::default()]);
        assert!(matches!(
            decode_raster(&bytes[..bytes.len() / 2]),
            Err(OptimiseError::Decode(_))
        ));
    }

    #[test]
    fn only_the_first_page_is_decoded_for_a_single_raster() {
        let bytes = testutil::tiff(&[
            TiffPage::default(),
            TiffPage {
                size: (10, 20),
                ..TiffPage::default()
            },
        ]);
        let r = decode_raster(&bytes).unwrap();
        assert_eq!((r.width, r.height), (64, 48));
    }

    #[test]
    fn cmyk_black_is_black_and_no_ink_is_white() {
        assert_eq!(
            to_rgba(ColorType::CMYK(8), &[0, 0, 0, 255, 0, 0, 0, 0]).unwrap(),
            [0, 0, 0, 255, 255, 255, 255, 255]
        );
    }
}
