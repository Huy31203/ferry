//! TIFF through the `tiff` crate (which cannot read grey-with-alpha pages).
//! Reading gives every page as RGBA8 with its orientation applied and its ICC
//! profile kept. Optimising is lossless:
//! every page keeps its colour type, bit depth, ICC profile, resolution and
//! orientation and is written again with Deflate and a horizontal predictor.

use std::io::{Cursor, Seek, Write};

use tiff::ColorType;
use tiff::TiffError;
use tiff::decoder::{Decoder, DecodingResult, Limits, ifd::Value};
use tiff::encoder::colortype::{self, ColorType as Encodable};
use tiff::encoder::compression::DeflateLevel;
use tiff::encoder::{Compression, Predictor, Rational, TiffEncoder, TiffKindStandard, TiffValue};
use tiff::tags::{ExtraSamples, Tag};

use super::png::MAX_PIXELS;
use super::{OptimiseError, OptimiseOptions};
use crate::edit::raster::{Raster, orient, rgb_profile};
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

fn encode_err(e: impl std::fmt::Display) -> OptimiseError {
    OptimiseError::Encode(e.to_string())
}

type Reader<'a> = Decoder<Cursor<&'a [u8]>>;

fn open(bytes: &[u8]) -> Result<Reader<'_>, OptimiseError> {
    let mut limits = Limits::default();
    // A CMYK page of MAX_PIXELS at 8 bytes a pixel is the biggest we accept.
    limits.decoding_buffer_size = MAX_PIXELS as usize * 8;
    Decoder::new(Cursor::new(bytes))
        .map(|d| d.with_limits(limits))
        .map_err(decode_err)
}

/// X and Y resolution as fractions, and the unit.
type Resolution = ((u32, u32), (u32, u32), u16);

/// A page as stored: its own colour type and bit depth, and the tags the
/// optimiser keeps.
struct Native {
    width: u32,
    height: u32,
    colour: ColorType,
    samples: Samples,
    icc: Option<Vec<u8>>,
    orientation: Option<u16>,
    resolution: Option<Resolution>,
    alpha: ExtraSamples,
}

enum Samples {
    Eight(Vec<u8>),
    Sixteen(Vec<u16>),
}

/// The current page, as stored.
fn read_native(decoder: &mut Reader) -> Result<Native, OptimiseError> {
    let (width, height) = decoder.dimensions().map_err(decode_err)?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(unsupported());
    }
    let colour = decoder.colortype().map_err(decode_err)?;
    let rational = |decoder: &mut Reader, tag| match decoder.find_tag(tag) {
        Ok(Some(Value::Rational(n, d))) => Some((n, d)),
        _ => None,
    };
    let resolution = match (
        rational(decoder, Tag::XResolution),
        rational(decoder, Tag::YResolution),
    ) {
        (Some(x), Some(y)) => Some((
            x,
            y,
            decoder
                .find_tag_unsigned::<u16>(Tag::ResolutionUnit)
                .ok()
                .flatten()
                .unwrap_or(2),
        )),
        _ => None,
    };
    let alpha = match decoder
        .find_tag_unsigned_vec::<u16>(Tag::ExtraSamples)
        .ok()
        .flatten()
        .and_then(|v| v.first().copied())
    {
        Some(1) => ExtraSamples::AssociatedAlpha,
        Some(0) => ExtraSamples::Unspecified,
        _ => ExtraSamples::UnassociatedAlpha,
    };
    Ok(Native {
        width,
        height,
        colour,
        icc: decoder
            .find_tag_unsigned_vec::<u8>(Tag::IccProfile)
            .ok()
            .flatten(),
        orientation: decoder
            .find_tag_unsigned::<u16>(Tag::Orientation)
            .ok()
            .flatten(),
        resolution,
        alpha,
        samples: match decoder.read_image().map_err(decode_err)? {
            DecodingResult::U8(v) => Samples::Eight(v),
            DecodingResult::U16(v) => Samples::Sixteen(v),
            _ => return Err(unsupported()),
        },
    })
}

/// Every page as straight RGBA8, oriented by its tag 274, carrying its ICC
/// profile (tag 34675).
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
fn decode_page(decoder: &mut Reader) -> Result<Raster, OptimiseError> {
    let page = read_native(decoder)?;
    let samples = match page.samples {
        Samples::Eight(v) => v,
        Samples::Sixteen(v) => v.into_iter().map(|s| (s >> 8) as u8).collect(),
    };
    let rgba = to_rgba(page.colour, &samples).ok_or_else(unsupported)?;
    if rgba.len() != page.width as usize * page.height as usize * 4 {
        return Err(OptimiseError::Decode("the TIFF page is incomplete".into()));
    }
    Ok(orient(
        Raster {
            width: page.width,
            height: page.height,
            rgba,
            icc: rgb_profile(page.icc),
        },
        page.orientation.unwrap_or(1),
    ))
}

/// 8-bit samples (16-bit ones already narrowed) as RGBA.
fn to_rgba(colour: ColorType, s: &[u8]) -> Option<Vec<u8>> {
    Some(match colour {
        ColorType::Gray(8 | 16) => s.iter().flat_map(|&v| [v, v, v, 255]).collect(),
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

/// Lossless: every page is written again, in its own colour type, with
/// Deflate and a horizontal predictor. Pages are handled one at a time.
pub fn optimise(input: &[u8], _o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let mut decoder = open(input)?;
    let mut out = Cursor::new(Vec::new());
    let mut enc = encoder(&mut out)?;
    write_native(&mut enc, &read_native(&mut decoder)?)?;
    while decoder.more_images() {
        decoder.next_image().map_err(decode_err)?;
        write_native(&mut enc, &read_native(&mut decoder)?)?;
    }
    Ok(out.into_inner())
}

/// RGBA8 pages, each written as RGB8 when no pixel is transparent. Each keeps
/// its ICC profile.
pub fn encode_rasters(pages: &[Raster]) -> Result<Vec<u8>, OptimiseError> {
    let mut out = Cursor::new(Vec::new());
    let mut enc = encoder(&mut out)?;
    for r in pages {
        let opaque = r.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255);
        if opaque {
            let rgb: Vec<u8> = r
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            write_page::<colortype::RGB8, _>(
                &mut enc,
                r.width,
                r.height,
                &rgb,
                None,
                &Tags::of(r),
            )?;
        } else {
            let tags = Tags::of(r);
            let alpha = Some(ExtraSamples::UnassociatedAlpha);
            write_page::<colortype::RGBA8, _>(&mut enc, r.width, r.height, &r.rgba, alpha, &tags)?;
        }
    }
    Ok(out.into_inner())
}

fn encoder<W: Write + Seek>(w: W) -> Result<TiffEncoder<W, TiffKindStandard>, OptimiseError> {
    Ok(TiffEncoder::new(w)
        .map_err(encode_err)?
        .with_compression(Compression::Deflate(DeflateLevel::Best))
        .with_predictor(Predictor::Horizontal))
}

/// The tags a page keeps.
#[derive(Default)]
struct Tags<'a> {
    icc: Option<&'a [u8]>,
    orientation: Option<u16>,
    resolution: Option<Resolution>,
}

impl<'a> Tags<'a> {
    fn of(r: &'a Raster) -> Self {
        Tags {
            icc: r.icc.as_deref(),
            ..Tags::default()
        }
    }
}

/// One page of `T` samples. `alpha` says what the colour type's last sample
/// means (the type itself already counts it).
fn write_page<T, W>(
    enc: &mut TiffEncoder<W, TiffKindStandard>,
    width: u32,
    height: u32,
    data: &[T::Inner],
    alpha: Option<ExtraSamples>,
    tags: &Tags,
) -> Result<(), OptimiseError>
where
    T: Encodable,
    [T::Inner]: TiffValue,
    W: Write + Seek,
{
    let mut image = enc.new_image::<T>(width, height).map_err(encode_err)?;
    if let Some(alpha) = alpha {
        image
            .encoder()
            .write_tag(Tag::ExtraSamples, &[alpha.to_u16()][..])
            .map_err(encode_err)?;
    }
    if let Some(((xn, xd), (yn, yd), unit)) = tags.resolution {
        let dir = image.encoder();
        dir.write_tag(Tag::ResolutionUnit, unit)
            .map_err(encode_err)?;
        dir.write_tag(Tag::XResolution, Rational { n: xn, d: xd })
            .map_err(encode_err)?;
        dir.write_tag(Tag::YResolution, Rational { n: yn, d: yd })
            .map_err(encode_err)?;
    }
    if let Some(orientation) = tags.orientation {
        image
            .encoder()
            .write_tag(Tag::Orientation, orientation)
            .map_err(encode_err)?;
    }
    if let Some(icc) = tags.icc {
        image
            .encoder()
            .write_tag(Tag::IccProfile, icc)
            .map_err(encode_err)?;
    }
    image.write_data(data).map_err(encode_err)
}

fn write_native<W: Write + Seek>(
    enc: &mut TiffEncoder<W, TiffKindStandard>,
    p: &Native,
) -> Result<(), OptimiseError> {
    let tags = Tags {
        icc: p.icc.as_deref(),
        orientation: p.orientation,
        resolution: p.resolution,
    };
    let (w, h, a) = (p.width, p.height, p.alpha);
    match (p.colour, &p.samples) {
        (ColorType::Gray(8), Samples::Eight(d)) => {
            write_page::<colortype::Gray8, _>(enc, w, h, d, None, &tags)
        }
        (ColorType::Gray(16), Samples::Sixteen(d)) => {
            write_page::<colortype::Gray16, _>(enc, w, h, d, None, &tags)
        }
        (ColorType::RGB(8), Samples::Eight(d)) => {
            write_page::<colortype::RGB8, _>(enc, w, h, d, None, &tags)
        }
        (ColorType::RGB(16), Samples::Sixteen(d)) => {
            write_page::<colortype::RGB16, _>(enc, w, h, d, None, &tags)
        }
        (ColorType::RGBA(8), Samples::Eight(d)) => {
            write_page::<colortype::RGBA8, _>(enc, w, h, d, Some(a), &tags)
        }
        (ColorType::RGBA(16), Samples::Sixteen(d)) => {
            write_page::<colortype::RGBA16, _>(enc, w, h, d, Some(a), &tags)
        }
        (ColorType::CMYK(8), Samples::Eight(d)) => {
            write_page::<colortype::CMYK8, _>(enc, w, h, d, None, &tags)
        }
        _ => Err(unsupported()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimise::Outcome;
    use crate::sniff::Format;
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
    fn optimising_keeps_every_page_and_every_pixel() {
        let icc = vec![3_u8; 200];
        let input = testutil::tiff(&[
            TiffPage {
                size: (200, 150),
                icc: Some(icc.clone()),
                ..TiffPage::default()
            },
            TiffPage {
                size: (100, 80),
                colour: TiffColour::Gray16,
                ..TiffPage::default()
            },
        ]);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        assert_eq!(crate::sniff::tiff_page_count(&out), 2);
        assert_eq!(decode_pages(&out).unwrap(), decode_pages(&input).unwrap());
        assert_eq!(decode_pages(&out).unwrap()[0].icc, Some(icc));
    }

    #[test]
    fn every_colour_type_survives_optimising() {
        for colour in [
            TiffColour::Rgb8,
            TiffColour::Rgba8,
            TiffColour::Gray8,
            TiffColour::Gray16,
            TiffColour::Cmyk8,
        ] {
            let input = testutil::tiff(&[TiffPage {
                colour,
                orientation: 3,
                ..TiffPage::default()
            }]);
            let out = optimise(&input, &OptimiseOptions::default()).unwrap();
            assert_eq!(
                decode_pages(&out).unwrap(),
                decode_pages(&input).unwrap(),
                "{colour:?}"
            );
            assert_eq!(crate::sniff::tiff_orientation(&out), 3, "{colour:?}");
        }
    }

    #[test]
    fn resolution_survives_optimising() {
        let input = testutil::tiff_with_resolution(300);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert_eq!(testutil::tiff_resolution(&out), Some(300));
    }

    #[test]
    fn a_tiff_already_deflated_comes_out_already_optimal() {
        let once = optimise(
            &testutil::tiff(&[TiffPage::default()]),
            &OptimiseOptions::default(),
        )
        .unwrap();
        assert_eq!(
            crate::optimise::optimise(&once, Format::Tiff, &OptimiseOptions::default()).unwrap(),
            Outcome::AlreadyOptimal
        );
    }

    #[test]
    fn rasters_are_written_as_rgb_unless_something_is_transparent() {
        let opaque = Raster {
            width: 8,
            height: 8,
            rgba: [10, 20, 30, 255].repeat(64),
            icc: None,
        };
        let mut clear = opaque.clone();
        clear.rgba[3] = 0;
        let one = |r: &Raster| decode_raster(&encode_rasters(std::slice::from_ref(r)).unwrap());
        assert_eq!(one(&opaque).unwrap(), opaque);
        assert_eq!(one(&clear).unwrap(), clear);
        let two = encode_rasters(&[opaque.clone(), clear.clone()]).unwrap();
        assert_eq!(crate::sniff::tiff_page_count(&two), 2);
        assert_eq!(decode_pages(&two).unwrap(), [opaque, clear]);
    }

    #[test]
    fn cmyk_black_is_black_and_no_ink_is_white() {
        assert_eq!(
            to_rgba(ColorType::CMYK(8), &[0, 0, 0, 255, 0, 0, 0, 0]).unwrap(),
            [0, 0, 0, 255, 255, 255, 255, 255]
        );
    }
}
