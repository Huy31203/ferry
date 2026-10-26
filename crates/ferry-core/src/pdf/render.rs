//! Rendering PDF pages with PDFium (sE pdf-formats). The library is bundled
//! beside the app and loaded at run time, never linked; `init` is told where
//! it is and the first render binds it once.

use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pdfium_render::prelude::*;

use crate::cancel::CancelToken;
use crate::edit::MAX_SIDE;
use crate::edit::raster::Raster;
use crate::i18n::t;
use crate::optimise::OptimiseError;
use crate::optimise::png::MAX_PIXELS;

static LIBRARY: OnceLock<PathBuf> = OnceLock::new();
static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

/// Say where the PDFium library is. The first call wins.
pub fn init(library: PathBuf) {
    let _ = LIBRARY.set(library);
}

/// Whether a library path is set and the file is there.
pub fn available() -> bool {
    LIBRARY.get().is_some_and(|path| path.is_file())
}

/// Bind the library. Failing to find or load it means a broken install.
fn load(path: &Path) -> Result<Pdfium, String> {
    Pdfium::bind_to_library(path)
        .map(Pdfium::new)
        .map_err(|_| t("pdf.error.renderer"))
}

fn pdfium() -> Result<&'static Pdfium, OptimiseError> {
    PDFIUM
        .get_or_init(|| match LIBRARY.get() {
            Some(path) => load(path),
            None => Err(t("pdf.error.renderer")),
        })
        .as_ref()
        .map_err(|message| OptimiseError::Unsupported(message.clone()))
}

fn open<'a>(pdfium: &'a Pdfium, bytes: &'a [u8]) -> Result<PdfDocument<'a>, OptimiseError> {
    pdfium.load_pdf_from_byte_slice(bytes, None).map_err(|e| {
        if matches!(
            e,
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError)
        ) {
            OptimiseError::Unsupported(t("pdf.error.encrypted"))
        } else {
            OptimiseError::Decode(t("pdf.error.read"))
        }
    })
}

pub fn page_count(bytes: &[u8]) -> Result<u32, OptimiseError> {
    let pdfium = pdfium()?;
    let document = open(pdfium, bytes)?;
    Ok(document.pages().len().max(0) as u32)
}

/// Render the 1-based `pages` in order at `dpi`, handing each to `on_page`.
/// The document is opened once; a cancel stops before the next page.
pub fn render_pages(
    bytes: &[u8],
    pages: RangeInclusive<u32>,
    dpi: u16,
    cancel: &CancelToken,
    on_page: &mut dyn FnMut(u32, Raster) -> Result<(), OptimiseError>,
) -> Result<(), OptimiseError> {
    let pdfium = pdfium()?;
    let document = open(pdfium, bytes)?;
    for number in pages {
        if cancel.is_cancelled() {
            return Err(OptimiseError::Cancelled);
        }
        on_page(number, render(&document, number, dpi, MAX_SIDE)?)?;
    }
    Ok(())
}

/// One page, with its long side also capped at `max_side`.
pub fn render_page(
    bytes: &[u8],
    page: u32,
    dpi: u16,
    max_side: u32,
) -> Result<Raster, OptimiseError> {
    let pdfium = pdfium()?;
    render(&open(pdfium, bytes)?, page, dpi, max_side)
}

/// The pixel size of a page of `points` at `dpi`: both sides shrink by the
/// same factor when the long side exceeds `max_side` or the area `MAX_PIXELS`.
fn pixel_size(points: (f32, f32), dpi: u16, max_side: u32) -> (u32, u32) {
    let scale = f64::from(dpi) / 72.0;
    let (w, h) = (f64::from(points.0) * scale, f64::from(points.1) * scale);
    let long = w.max(h).max(1.0);
    let by_side = (f64::from(max_side.min(MAX_SIDE)) / long).min(1.0);
    let by_area = (MAX_PIXELS as f64 / (w * h).max(1.0)).sqrt().min(1.0);
    let f = by_side.min(by_area);
    (
        ((w * f).round() as u32).max(1),
        ((h * f).round() as u32).max(1),
    )
}

fn render(
    document: &PdfDocument,
    number: u32,
    dpi: u16,
    max_side: u32,
) -> Result<Raster, OptimiseError> {
    let index = i32::try_from(number.saturating_sub(1))
        .map_err(|_| OptimiseError::Decode(t("pdf.error.read")))?;
    let page = document
        .pages()
        .get(index)
        .map_err(|_| OptimiseError::Decode(t("pdf.error.read")))?;
    let (w, h) = pixel_size((page.width().value, page.height().value), dpi, max_side);
    let config = PdfRenderConfig::new()
        .set_target_width(w as i32)
        .set_maximum_height(h as i32)
        .render_form_data(true)
        .render_annotations(true)
        .set_clear_color(PdfColor::WHITE);
    let bitmap = page
        .render_with_config(&config)
        .map_err(|_| OptimiseError::Decode(t("pdf.error.read")))?;
    Ok(Raster {
        width: bitmap.width() as u32,
        height: bitmap.height() as u32,
        rgba: bitmap.as_rgba_bytes(),
        icc: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn ci_always_has_pdfium() {
        if std::env::var_os("CI").is_some() {
            assert!(
                testutil::pdfium(),
                "FERRY_PDFIUM must point at the fetched library in CI"
            );
        }
    }

    #[test]
    fn a_letter_page_renders_at_the_asked_dpi_on_white() {
        if !testutil::pdfium() {
            return;
        }
        let r = render_page(&testutil::blank_pdf(&[(612.0, 792.0)]), 1, 150, u32::MAX).unwrap();
        assert_eq!((r.width, r.height), (1275, 1650));
        assert_eq!(r.rgba[..4], [255, 255, 255, 255]);
    }

    #[test]
    fn the_long_side_is_capped() {
        if !testutil::pdfium() {
            return;
        }
        let r = render_page(&testutil::blank_pdf(&[(612.0, 792.0)]), 1, 150, 600).unwrap();
        assert_eq!(r.height, 600);
    }

    #[test]
    fn every_page_is_counted_and_rendered_in_order() {
        if !testutil::pdfium() {
            return;
        }
        let pdf = testutil::blank_pdf(&[(100.0, 100.0), (200.0, 100.0), (100.0, 300.0)]);
        assert_eq!(page_count(&pdf).unwrap(), 3);
        let mut seen = Vec::new();
        render_pages(&pdf, 2..=3, 72, &CancelToken::new(), &mut |n, r| {
            seen.push((n, r.width, r.height));
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, [(2, 200, 100), (3, 100, 300)]);
    }

    #[test]
    fn a_cancel_stops_before_the_next_page() {
        if !testutil::pdfium() {
            return;
        }
        let pdf = testutil::blank_pdf(&[(100.0, 100.0); 3]);
        let cancel = CancelToken::new();
        let mut pages = 0;
        let result = render_pages(&pdf, 1..=3, 72, &cancel, &mut |_, _| {
            pages += 1;
            cancel.cancel();
            Ok(())
        });
        assert!(matches!(result, Err(OptimiseError::Cancelled)));
        assert_eq!(pages, 1);
    }

    #[test]
    fn a_cropped_page_renders_at_its_crop_box() {
        if !testutil::pdfium() {
            return;
        }
        let page = testutil::PdfPage {
            crop: Some([0.0, 0.0, 306.0, 396.0]),
            ..testutil::PdfPage::default()
        };
        let pdf = testutil::pdf(&[page], &testutil::PdfExtras::default());
        let r = render_page(&pdf, 1, 72, u32::MAX).unwrap();
        assert_eq!((r.width, r.height), (306, 396));
    }

    #[test]
    fn a_library_that_is_not_there_says_reinstall() {
        assert_eq!(
            load(Path::new("/nowhere/libpdfium.dylib")).err(),
            Some(t("pdf.error.renderer"))
        );
    }

    #[test]
    fn a_page_is_sized_from_its_points_and_capped() {
        assert_eq!(pixel_size((612.0, 792.0), 150, u32::MAX), (1275, 1650));
        assert_eq!(pixel_size((612.0, 792.0), 1200, 600), (464, 600));
        // 150 M pixels: a poster-sized page at 1200 dpi shrinks to fit.
        let (w, h) = pixel_size((3000.0, 3000.0), 1200, u32::MAX);
        assert!(u64::from(w) * u64::from(h) <= MAX_PIXELS && w <= MAX_SIDE);
    }
}
