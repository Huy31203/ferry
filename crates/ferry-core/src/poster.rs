//! Poster PNGs for originals the webviews cannot draw (TIFF, HEIC, PDF): the
//! card thumbnail, and later the compare view.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::edit::geometry::Rect;
use crate::edit::raster;
use crate::optimise::OptimiseError;
use crate::sniff::{self, Format};

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
        Format::Pdf => pdf_page(bytes, page, long_edge)?,
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

/// The one place a PDF page becomes pixels for a poster: PDFium at its
/// highest DPI, shrunk to `long_edge`, so the long side is exactly that. A
/// page has no pixel size of its own, so nothing is "enlarged".
fn pdf_page(bytes: &[u8], page: u32, long_edge: u32) -> Result<raster::Raster, OptimiseError> {
    crate::pdf::render::render_page(bytes, page, crate::edit::MAX_DPI, long_edge)
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

// ---- sE pdf-formats: compare's views ----

/// What the compare window draws for one file: the file itself, or a poster.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub src: PathBuf,
    /// The poster's size; `None` when `src` is the file itself.
    pub dims: Option<(u32, u32)>,
    /// A PDF's page count.
    pub pages: Option<u32>,
}

/// The file at `path` as something the webview can draw: itself, or for a
/// TIFF, HEIC or PDF its page `page` as a poster PNG in `<cache>/posters/`
/// (reused while the file is unchanged).
pub fn viewable(path: &Path, page: u32, cache: &Path) -> Result<View, String> {
    let itself = View {
        src: path.to_owned(),
        dims: None,
        pages: None,
    };
    let mut head = Vec::new();
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    (&mut file)
        .take(4096)
        .read_to_end(&mut head)
        .map_err(|e| e.to_string())?;
    let Some(format) = sniff::sniff(&head).filter(|&f| needs_poster(f)) else {
        return Ok(itself);
    };
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let pages = if format == Format::Pdf {
        Some(crate::pdf::render::page_count(&bytes).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let poster = cache
        .join("posters")
        .join(format!("{}.png", key(path, page)));
    let dims = match std::fs::read(&poster) {
        Ok(cached) => sniff::dimensions(&cached, Format::Png),
        Err(_) => None,
    };
    let dims = match dims {
        Some(dims) => dims,
        None => {
            let (png, dims) = png(&bytes, format, page, VIEW_EDGE).map_err(|e| e.to_string())?;
            write_poster(&poster, &png).map_err(|e| e.to_string())?;
            dims
        }
    };
    Ok(View {
        src: poster,
        dims: Some(dims),
        pages,
    })
}

/// A 64-bit FNV-1a of the file's path, length, mtime (ms) and `page`, as 16
/// hex digits: another file, another version or another page is another poster.
fn key(path: &Path, page: u32) -> String {
    let meta = std::fs::metadata(path).ok();
    let len = meta.as_ref().map_or(0, |m| m.len());
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis());
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let parts = [
        path.to_string_lossy().as_bytes().to_vec(),
        len.to_le_bytes().to_vec(),
        mtime.to_le_bytes().to_vec(),
        page.to_le_bytes().to_vec(),
    ];
    for byte in parts.concat() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn write_poster(poster: &Path, png: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = poster.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = poster.with_extension("png.tmp");
    std::fs::write(&temp, png)?;
    std::fs::rename(&temp, poster)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use std::fs;

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

    #[test]
    fn a_pdf_poster_is_its_first_page() {
        if !testutil::pdfium() {
            return;
        }
        let pdf = testutil::blank_pdf(&[(612.0, 792.0)]);
        assert_eq!(png(&pdf, Format::Pdf, 1, CARD_EDGE).unwrap().1, (464, 600));
    }

    #[test]
    fn a_png_is_its_own_view() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        fs::write(&path, testutil::png(10, 10)).unwrap();
        let view = viewable(&path, 1, dir.path()).unwrap();
        assert_eq!(view.src, path);
        assert_eq!((view.dims, view.pages), (None, None));
    }

    #[test]
    fn a_pdf_view_is_a_cached_poster_that_knows_its_pages() {
        if !testutil::pdfium() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pdf");
        fs::write(
            &path,
            testutil::blank_pdf(&[(612.0, 792.0), (612.0, 792.0)]),
        )
        .unwrap();
        let two = viewable(&path, 2, dir.path()).unwrap();
        assert_eq!((two.pages, two.dims), (Some(2), Some((1855, 2400))));
        assert!(two.src.starts_with(dir.path().join("posters")));
        assert_eq!(viewable(&path, 2, dir.path()).unwrap(), two);
        assert_ne!(viewable(&path, 1, dir.path()).unwrap().src, two.src);
    }
}
