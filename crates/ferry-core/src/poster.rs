//! Poster PNGs for originals the webviews cannot draw (TIFF, HEIC, PDF): the
//! card thumbnail, and later the compare view.

use crate::edit::geometry::Rect;
use crate::edit::raster;
use crate::i18n::tf;
use crate::optimise::OptimiseError;
use crate::sniff::Format;

/// The longest side of a card's poster.
pub const CARD_EDGE: u32 = 600;
/// The longest side of a poster for the compare and preview windows.
pub const VIEW_EDGE: u32 = 2400;

/// Whether `format` needs a poster. WKWebView and WebView2 both draw PNG,
/// JPEG, GIF, WebP and AVIF.
pub fn needs_poster(format: Format) -> bool {
    matches!(format, Format::Tiff | Format::Heic | Format::Pdf)
}

/// `bytes` (page `page`, 1-based, for a document) as a PNG whose long side is
/// at most `long_edge` (never enlarged), and its size.
pub fn png(
    bytes: &[u8],
    format: Format,
    page: u32,
    long_edge: u32,
) -> Result<(Vec<u8>, (u32, u32)), OptimiseError> {
    let full = match format {
        Format::Pdf => pdf_page(bytes, page)?,
        _ => raster::decode(bytes, format)?,
    };
    let long = full.width.max(full.height);
    let shown = if long > long_edge {
        let scale = f64::from(long_edge) / f64::from(long);
        let size = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
        let out = (size(full.width), size(full.height));
        let whole = Rect {
            x: 0,
            y: 0,
            width: full.width,
            height: full.height,
        };
        raster::cut(&full, whole, out)?
    } else {
        full
    };
    let size = (shown.width, shown.height);
    Ok((write_png(&shown)?, size))
}

/// The one place a PDF page becomes pixels for a poster. The PDF stream
/// replaces this body with a PDFium render.
fn pdf_page(_bytes: &[u8], _page: u32) -> Result<raster::Raster, OptimiseError> {
    Err(OptimiseError::Unsupported(tf(
        "pdf.read_unsupported",
        &[("format", Format::Pdf.label())],
    )))
}

fn write_png(r: &raster::Raster) -> Result<Vec<u8>, OptimiseError> {
    let encode_err = |e: png::EncodingError| OptimiseError::Encode(e.to_string());
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, r.width, r.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(encode_err)?;
    writer.write_image_data(&r.rgba).map_err(encode_err)?;
    writer.finish().map_err(encode_err)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn a_poster_fits_its_long_edge_and_keeps_the_ratio() {
        let (png, size) = png(&testutil::png(1200, 800), Format::Png, 1, CARD_EDGE).unwrap();
        assert_eq!(size, (600, 400));
        assert_eq!(
            crate::sniff::dimensions(&png, Format::Png),
            Some((600, 400))
        );
    }

    #[test]
    fn a_small_picture_is_never_enlarged() {
        assert_eq!(
            png(&testutil::png(120, 80), Format::Png, 1, CARD_EDGE)
                .unwrap()
                .1,
            (120, 80)
        );
    }

    #[test]
    fn only_formats_the_webviews_cannot_draw_need_a_poster() {
        let need: Vec<Format> = Format::ALL
            .into_iter()
            .filter(|&f| needs_poster(f))
            .collect();
        assert_eq!(need, [Format::Tiff, Format::Heic, Format::Pdf]);
    }

    #[test]
    fn an_unreadable_file_has_no_poster() {
        assert!(png(b"II*\0garbage", Format::Tiff, 1, CARD_EDGE).is_err());
    }
}
