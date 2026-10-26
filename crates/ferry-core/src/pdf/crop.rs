//! Cropping PDF pages (sE pdf-formats). A page shows its `CropBox` where it
//! overlaps its `MediaBox`; cropping sets each page's own `CropBox` and never
//! touches the content, so nothing is lost and uncropping brings it back.

use lopdf::{Document, Object, ObjectId};

use super::{inherited, read_box};
use crate::optimise::OptimiseError;

/// `[x0, y0, x1, y1]` in points, normalised so `x0 < x1` and `y0 < y1`.
pub type BoxPt = [f32; 4];

/// The part of `media` the crop shows: the overlap, or all of `media` when the
/// two do not overlap.
pub fn visible_box(media: BoxPt, crop: Option<BoxPt>) -> BoxPt {
    let Some(crop) = crop else {
        return media;
    };
    let overlap = [
        media[0].max(crop[0]),
        media[1].max(crop[1]),
        media[2].min(crop[2]),
        media[3].min(crop[3]),
    ];
    if overlap[2] > overlap[0] && overlap[3] > overlap[1] {
        overlap
    } else {
        media
    }
}

/// The largest centred box of `ratio` inside `visible`. The ratio is judged as
/// the page is displayed: landscape on a landscape page, portrait on a
/// portrait one, and turned back for a page rotated by 90 or 270.
pub fn ratio_box(visible: BoxPt, ratio: (u32, u32), rotate: i64) -> BoxPt {
    let (w, h) = (visible[2] - visible[0], visible[3] - visible[1]);
    let turned = matches!(rotate.rem_euclid(360), 90 | 270);
    let (shown_w, shown_h) = if turned { (h, w) } else { (w, h) };
    let (long, short) = (ratio.0.max(ratio.1) as f32, ratio.0.min(ratio.1) as f32);
    let (rw, rh) = match shown_w.partial_cmp(&shown_h) {
        Some(std::cmp::Ordering::Greater) => (long, short),
        Some(std::cmp::Ordering::Less) => (short, long),
        _ => (ratio.0 as f32, ratio.1 as f32),
    };
    // In the page's own space.
    let (rw, rh) = if turned { (rh, rw) } else { (rw, rh) };
    let (bw, bh) = if w * rh > h * rw {
        (h * rw / rh, h)
    } else {
        (w, w * rh / rw)
    };
    let (x, y) = (visible[0] + (w - bw) / 2.0, visible[1] + (h - bh) / 2.0);
    [x, y, x + bw, y + bh]
}

/// The page's own box for `key`, or the one it inherits.
pub(crate) fn page_box(doc: &Document, page: ObjectId, key: &[u8]) -> Option<BoxPt> {
    inherited(doc, page, key).and_then(|o| read_box(doc, o))
}

/// Set every page's `CropBox`: to `aspect` (a ratio centred in what is
/// visible), or to what is visible. `uncrop` starts from the whole `MediaBox`
/// instead, showing what an earlier crop hid.
pub fn apply_crop(
    doc: &mut Document,
    aspect: Option<(u32, u32)>,
    uncrop: bool,
) -> Result<(), OptimiseError> {
    let mut boxes = Vec::new();
    for page in doc.page_iter().collect::<Vec<_>>() {
        let media = page_box(doc, page, b"MediaBox").unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let visible = if uncrop {
            media
        } else {
            visible_box(media, page_box(doc, page, b"CropBox"))
        };
        let rotate = inherited(doc, page, b"Rotate")
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        let crop = aspect.map_or(visible, |ratio| ratio_box(visible, ratio, rotate));
        boxes.push((page, crop));
    }
    for (page, crop) in boxes {
        let round = |v: f32| Object::Real((v * 100.0).round() / 100.0);
        let dict = doc
            .get_dictionary_mut(page)
            .map_err(|e| OptimiseError::Encode(e.to_string()))?;
        dict.set(
            "CropBox",
            crop.iter().map(|&v| round(v)).collect::<Vec<_>>(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, PdfExtras, PdfPage};

    fn close(a: BoxPt, b: BoxPt) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
    }

    #[test]
    fn the_visible_box_is_the_overlap() {
        assert!(close(
            visible_box([0.0, 0.0, 612.0, 792.0], Some([36.0, 36.0, 700.0, 756.0])),
            [36.0, 36.0, 612.0, 756.0]
        ));
        assert!(close(
            visible_box([0.0, 0.0, 612.0, 792.0], Some([900.0, 900.0, 950.0, 950.0])),
            [0.0, 0.0, 612.0, 792.0]
        ));
    }

    #[test]
    fn an_a4_page_crops_to_an_ipad_by_trimming_its_sides() {
        let b = ratio_box([0.0, 0.0, 595.0, 842.0], (1640, 2360), 0);
        let w = 842.0 * 1640.0 / 2360.0;
        assert!(close(b, [(595.0 - w) / 2.0, 0.0, (595.0 + w) / 2.0, 842.0]));
    }

    #[test]
    fn a_landscape_page_gets_the_landscape_ratio() {
        let b = ratio_box([0.0, 0.0, 842.0, 595.0], (9, 16), 0);
        assert!(((b[2] - b[0]) / (b[3] - b[1]) - 16.0 / 9.0).abs() < 0.01);
    }

    #[test]
    fn a_rotated_page_is_judged_as_displayed() {
        // Stored portrait, shown landscape: a 16:9 ratio must look 16:9 on screen.
        let b = ratio_box([0.0, 0.0, 595.0, 842.0], (16, 9), 90);
        assert!(((b[3] - b[1]) / (b[2] - b[0]) - 16.0 / 9.0).abs() < 0.01);
    }

    #[test]
    fn uncrop_shows_the_whole_media_box_even_through_an_inherited_crop() {
        // testutil::pdf puts the crop on the page tree when every page shares it.
        let page = PdfPage {
            crop: Some([50.0, 50.0, 500.0, 700.0]),
            ..PdfPage::default()
        };
        let pdf = testutil::pdf(&[page.clone(), page], &PdfExtras::default());
        let mut doc = Document::load_mem(&pdf).unwrap();
        apply_crop(&mut doc, None, true).unwrap();
        for id in doc.get_pages().values() {
            assert_eq!(
                page_box(&doc, *id, b"CropBox"),
                Some([0.0, 0.0, 612.0, 792.0])
            );
        }
    }
}
