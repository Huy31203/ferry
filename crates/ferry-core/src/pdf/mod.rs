//! PDF jobs (sE pdf-formats): the file structure is edited with lopdf, pages
//! are rendered with PDFium, which is loaded at run time from the bundled
//! library (`render::init`).

mod images;
pub mod render;

use lopdf::{Document, LoadOptions, Object, ObjectId};

use crate::edit::{self, Edits, MAX_DPI, MIN_DPI};
use crate::i18n::{t, tf};
use crate::optimise::{Level, OptimiseError, OptimiseOptions};
use crate::settings;
use crate::sniff::Format;

/// No single stream may inflate beyond this while a PDF loads.
const MAX_DECOMPRESSED: usize = 1 << 30;

/// The DPI stops of the card's slider, highest first.
pub const DPI_STOPS: [u16; 7] = [300, 250, 200, 150, 100, 72, 48];

/// The resolutions the settings give: images when optimising (normal and
/// aggressive) and pages extracted as images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Defaults {
    pub dpi: u16,
    pub dpi_aggressive: u16,
    pub extract_dpi: u16,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            dpi: 150,
            dpi_aggressive: 100,
            extract_dpi: 150,
        }
    }
}

impl From<&settings::Pdf> for Defaults {
    fn from(p: &settings::Pdf) -> Self {
        Self {
            dpi: p.dpi,
            dpi_aggressive: p.dpi_aggressive,
            extract_dpi: p.extract_dpi,
        }
    }
}

/// What a PDF job needs to know before it decides anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    pub pages: u32,
    /// Page 1 as displayed, in points.
    pub size: (u32, u32),
    pub encrypted: bool,
    pub signed: bool,
    /// Declares PDF/A-1, which forbids object streams.
    pub pdfa1: bool,
}

/// What `apply` made.
#[derive(Debug)]
pub enum PdfResult {
    Pdf {
        bytes: Vec<u8>,
        missed_target: bool,
    },
    Pages {
        format: Format,
        /// 1-based page number and its image.
        pages: Vec<(u32, Vec<u8>)>,
        /// The first image's pixels.
        size: (u32, u32),
    },
}

fn load(bytes: &[u8]) -> Result<Document, OptimiseError> {
    let options = LoadOptions::with_max_decompressed_size(MAX_DECOMPRESSED);
    match Document::load_mem_with_options(bytes, options) {
        Ok(doc) if doc.was_encrypted() || doc.trailer.has(b"Encrypt") => {
            Err(OptimiseError::Unsupported(t("pdf.error.encrypted")))
        }
        Ok(doc) if doc.get_pages().is_empty() => Err(OptimiseError::Decode(t("pdf.error.read"))),
        Ok(doc) => Ok(doc),
        // A file lopdf cannot open that names an /Encrypt dictionary is encrypted.
        Err(_) if bytes.windows(8).any(|w| w == b"/Encrypt") => {
            Err(OptimiseError::Unsupported(t("pdf.error.encrypted")))
        }
        Err(_) => Err(OptimiseError::Decode(t("pdf.error.read"))),
    }
}

/// An attribute a page may inherit from its page tree.
pub(crate) fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut id = page;
    for _ in 0..64 {
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(value) = dict.get(key) {
            return doc.dereference(value).ok().map(|(_, o)| o);
        }
        id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// `[x0, y0, x1, y1]`, normalised so `x0 < x1` and `y0 < y1`.
fn read_box(doc: &Document, object: &Object) -> Option<[f32; 4]> {
    let numbers: Vec<f32> = object
        .as_array()
        .ok()?
        .iter()
        .map(|n| doc.dereference(n).ok()?.1.as_float().ok())
        .collect::<Option<_>>()?;
    let [a, b, c, d] = numbers[..] else {
        return None;
    };
    let normal = [a.min(c), b.min(d), a.max(c), b.max(d)];
    (normal[2] > normal[0] && normal[3] > normal[1]).then_some(normal)
}

/// Page 1 as displayed: `MediaBox` and `CropBox` overlap, turned by `/Rotate`.
fn displayed_size(doc: &Document, page: ObjectId) -> (u32, u32) {
    let get = |key: &[u8]| inherited(doc, page, key).and_then(|o| read_box(doc, o));
    let media = get(b"MediaBox").unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let crop = get(b"CropBox").unwrap_or(media);
    let (x0, y0) = (media[0].max(crop[0]), media[1].max(crop[1]));
    let (x1, y1) = (media[2].min(crop[2]), media[3].min(crop[3]));
    let (w, h) = if x1 > x0 && y1 > y0 {
        (x1 - x0, y1 - y0)
    } else {
        (media[2] - media[0], media[3] - media[1])
    };
    let rotate = inherited(doc, page, b"Rotate")
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    let (w, h) = if rotate == 90 || rotate == 270 {
        (h, w)
    } else {
        (w, h)
    };
    (w.round() as u32, h.round() as u32)
}

/// Whether the file carries a digital signature: the AcroForm says signatures
/// exist, or a signature field has a value.
fn is_signed(doc: &Document) -> bool {
    let form = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_dict().ok());
    let Some(form) = form else {
        return false;
    };
    if form
        .get(b"SigFlags")
        .and_then(Object::as_i64)
        .is_ok_and(|flags| flags & 1 != 0)
    {
        return true;
    }
    let mut todo: Vec<&Object> = form
        .get(b"Fields")
        .and_then(Object::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let mut seen = 0;
    while let Some(field) = todo.pop() {
        seen += 1;
        if seen > 100_000 {
            break;
        }
        let Some(dict) = doc
            .dereference(field)
            .ok()
            .and_then(|(_, o)| o.as_dict().ok())
        else {
            continue;
        };
        if dict
            .get(b"FT")
            .and_then(Object::as_name)
            .is_ok_and(|n| n == b"Sig")
            && dict.has(b"V")
        {
            return true;
        }
        if let Ok(kids) = dict.get(b"Kids").and_then(Object::as_array) {
            todo.extend(kids);
        }
    }
    false
}

/// Whether the XMP metadata declares PDF/A-1.
fn is_pdfa1(doc: &Document) -> bool {
    let xmp = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Metadata").ok())
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_stream().ok())
        .and_then(|s| s.get_plain_content_with_limit(1 << 24).ok())
        .unwrap_or_default();
    let has = |needle: &[u8]| xmp.windows(needle.len()).any(|w| w == needle);
    has(b"pdfaid:part=\"1\"") || has(b"<pdfaid:part>1<")
}

/// Open the file and read what the job needs. An encrypted file is a result,
/// not an error, so the job can say so.
pub fn facts(bytes: &[u8]) -> Result<Facts, OptimiseError> {
    let doc = match load(bytes) {
        Ok(doc) => doc,
        Err(OptimiseError::Unsupported(_)) => {
            return Ok(Facts {
                encrypted: true,
                ..Facts::default()
            });
        }
        Err(e) => return Err(e),
    };
    let pages = doc.get_pages();
    let first = pages.values().next().copied();
    Ok(Facts {
        pages: pages.len() as u32,
        size: first.map_or((0, 0), |page| displayed_size(&doc, page)),
        encrypted: false,
        signed: is_signed(&doc),
        pdfa1: is_pdfa1(&doc),
    })
}

/// Recompress the file's structure: unused objects go, streams are deflated
/// and objects share object streams (not for PDF/A-1, which forbids them).
/// Content streams, fonts, links and forms are kept byte for byte.
pub fn optimise(
    input: &[u8],
    dpi: u16,
    o: &OptimiseOptions,
    progress: &mut dyn FnMut(f32),
) -> Result<Vec<u8>, OptimiseError> {
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    let mut doc = load(input)?;
    images::recompress_all(&mut doc, dpi, o, &mut |p| progress(p * 0.95))?;
    doc.prune_objects();
    doc.delete_zero_length_streams();
    // The old object and xref streams are gone; their numbers are free again,
    // so optimising an optimised file gives the same file.
    doc.max_id = doc.objects.keys().map(|id| id.0).max().unwrap_or(0);
    doc.compress();
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    let mut out = Vec::new();
    let saved = if is_pdfa1(&doc) {
        doc.save_to(&mut out)
    } else {
        doc.save_modern(&mut out)
    };
    saved.map_err(|e| OptimiseError::Encode(e.to_string()))?;
    progress(1.0);
    Ok(out)
}

/// The resolution a recipe asks for: its own, else the settings' for
/// extracting pages, aggressive runs or normal runs.
pub fn target_dpi(edits: &Edits, d: &Defaults, extracting: bool) -> u16 {
    edits
        .dpi
        .unwrap_or(if extracting {
            d.extract_dpi
        } else if edits.level == Level::Aggressive {
            d.dpi_aggressive
        } else {
            d.dpi
        })
        .clamp(MIN_DPI, MAX_DPI)
}

/// Run the whole recipe on a PDF original. `progress` gets a fraction and the
/// step's label.
pub fn apply(
    original: &[u8],
    edits: &Edits,
    o: &OptimiseOptions,
    d: &Defaults,
    progress: &mut dyn FnMut(f32, String),
) -> Result<PdfResult, OptimiseError> {
    if edits.aspect.is_some()
        || edits.uncrop
        || matches!(edits.format, Some(Format::Png | Format::Jpeg))
    {
        return Err(OptimiseError::Unsupported(tf(
            "pdf.write_unsupported",
            &[("format", Format::Pdf.label())],
        )));
    }
    let dpi = target_dpi(edits, d, false);
    let step = t("pdf.step.pdf");
    let mut base = o.clone();
    base.level = edits.level;
    if let Some(compression) = edits.compression {
        base.compression = compression;
    }
    let (bytes, missed_target) = match edits.fit_under {
        Some(limit) => {
            let mut runs = 0.0;
            let fitted = edit::fit::search(limit, |q| {
                runs += 1.0;
                progress((runs / 7.0_f32).min(0.99), step.clone());
                let attempt = OptimiseOptions {
                    compression: q,
                    level: Level::Normal,
                    ..base.clone()
                };
                optimise(original, dpi, &attempt, &mut |_| {})
            })?;
            (fitted.bytes, fitted.missed)
        }
        None => (
            optimise(original, dpi, &base, &mut |p| progress(p, step.clone()))?,
            false,
        ),
    };
    Ok(PdfResult::Pdf {
        bytes,
        missed_target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, PdfExtras, PdfPage};

    #[test]
    fn the_defaults_are_the_settings_defaults() {
        assert_eq!(
            Defaults::from(&settings::Pdf::default()),
            Defaults::default()
        );
        assert!(DPI_STOPS.windows(2).all(|w| w[0] > w[1]));
    }

    #[test]
    fn facts_count_pages_and_turn_rotated_pages() {
        let pdf = testutil::pdf(
            &[
                PdfPage {
                    rotate: 90,
                    ..PdfPage::default()
                },
                PdfPage::default(),
            ],
            &PdfExtras::default(),
        );
        let f = facts(&pdf).unwrap();
        assert_eq!(
            (f.pages, f.size, f.encrypted, f.signed),
            (2, (792, 612), false, false)
        );
    }

    #[test]
    fn signatures_encryption_and_pdfa_are_noticed() {
        let one =
            |extras: PdfExtras| facts(&testutil::pdf(&[PdfPage::default()], &extras)).unwrap();
        assert!(
            one(PdfExtras {
                signed: true,
                ..Default::default()
            })
            .signed
        );
        assert!(
            one(PdfExtras {
                encrypt_dict: true,
                ..Default::default()
            })
            .encrypted
        );
        assert!(
            one(PdfExtras {
                pdfa1: true,
                ..Default::default()
            })
            .pdfa1
        );
    }

    #[test]
    fn a_bloated_pdf_shrinks_and_keeps_its_pages() {
        let input = testutil::pdf(
            &vec![
                PdfPage {
                    padding: 200_000,
                    ..PdfPage::default()
                };
                3
            ],
            &PdfExtras::default(),
        );
        let out = optimise(&input, 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
        assert!(out.len() * 2 < input.len());
        assert_eq!(facts(&out).unwrap().pages, 3);
    }

    #[test]
    fn pdfa1_is_saved_without_object_streams() {
        let input = testutil::pdf(
            &[PdfPage {
                padding: 50_000,
                ..PdfPage::default()
            }],
            &PdfExtras {
                pdfa1: true,
                ..Default::default()
            },
        );
        let out = optimise(&input, 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
        assert!(!out.windows(7).any(|w| w == b"/ObjStm"));
    }

    #[test]
    fn the_dpi_follows_recipe_level_and_extraction() {
        let d = Defaults::default();
        assert_eq!(target_dpi(&Edits::default(), &d, false), 150);
        assert_eq!(
            target_dpi(
                &Edits {
                    level: Level::Aggressive,
                    ..Edits::default()
                },
                &d,
                false
            ),
            100
        );
        assert_eq!(target_dpi(&Edits::default(), &d, true), 150);
        assert_eq!(
            target_dpi(
                &Edits {
                    dpi: Some(72),
                    ..Edits::default()
                },
                &d,
                true
            ),
            72
        );
    }
}
