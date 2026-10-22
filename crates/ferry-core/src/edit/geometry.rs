//! Pixel maths for an edit: the window an aspect crop cuts and the size the
//! result ends up. Pure functions, so every rule is pinned here.

use super::{Edits, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The largest window of `ratio` that fits in `dims`, rounded, at least 1×1.
pub fn fit_ratio((w, h): (u32, u32), (rw, rh): (u32, u32)) -> (u32, u32) {
    let (w64, h64, rw, rh) = (u64::from(w), u64::from(h), u64::from(rw), u64::from(rh));
    if w64 * rh > h64 * rw {
        // Wider than the ratio: keep the height.
        (((h64 * rw + rh / 2) / rh).clamp(1, w64) as u32, h)
    } else {
        (w, ((w64 * rh + rw / 2) / rw).clamp(1, h64) as u32)
    }
}

/// The window cut from the (oriented) original: an `Exact` size's ratio, or
/// else the aspect, or else the whole picture.
pub fn crop_size(dims: (u32, u32), edits: &Edits) -> (u32, u32) {
    let ratio = match edits.size {
        Some(Size::Exact { width, height }) => Some((width, height)),
        _ => edits.aspect,
    };
    ratio.map_or(dims, |ratio| fit_ratio(dims, ratio))
}

pub fn centre((w, h): (u32, u32), (cw, ch): (u32, u32)) -> Rect {
    Rect {
        x: (w - cw) / 2,
        y: (h - ch) / 2,
        width: cw,
        height: ch,
    }
}

/// The result's size from the cut `window`; never larger than the window.
pub fn output_size((cw, ch): (u32, u32), edits: &Edits) -> (u32, u32) {
    let by_width = |w: u32| {
        let w = w.min(cw);
        (w, scaled(ch, w, cw))
    };
    let by_height = |h: u32| {
        let h = h.min(ch);
        (scaled(cw, h, ch), h)
    };
    match (edits.size, edits.scale) {
        (Some(Size::Exact { width, height }), _) => {
            if width >= cw || height >= ch {
                (cw, ch)
            } else {
                (width, height)
            }
        }
        (Some(Size::Width { width }), _) => by_width(width),
        (Some(Size::Height { height }), _) => by_height(height),
        (Some(Size::LongEdge { edge }), _) if cw >= ch => by_width(edge),
        (Some(Size::LongEdge { edge }), _) => by_height(edge),
        (None, Some(percent)) => (
            scaled(cw, u32::from(percent), 100),
            scaled(ch, u32::from(percent), 100),
        ),
        (None, None) => (cw, ch),
    }
}

/// `side * num / den`, rounded, at least 1.
fn scaled(side: u32, num: u32, den: u32) -> u32 {
    let (side, num, den) = (u64::from(side), u64::from(num), u64::from(den));
    ((side * num + den / 2) / den).max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Edits, Size};

    fn with(f: impl FnOnce(&mut Edits)) -> Edits {
        let mut edits = Edits::default();
        f(&mut edits);
        edits
    }

    #[test]
    fn every_aspect_preset_fits_landscape_and_portrait() {
        let landscape = (2880, 1800);
        assert_eq!(fit_ratio(landscape, (16, 9)), (2880, 1620));
        assert_eq!(fit_ratio(landscape, (4, 3)), (2400, 1800));
        assert_eq!(fit_ratio(landscape, (1, 1)), (1800, 1800));
        assert_eq!(fit_ratio(landscape, (191, 100)), (2880, 1508));
        assert_eq!(fit_ratio(landscape, (9, 16)), (1013, 1800));
        let portrait = (1000, 2000);
        assert_eq!(fit_ratio(portrait, (16, 9)), (1000, 563));
        assert_eq!(fit_ratio(portrait, (9, 16)), (1000, 1778));
    }

    #[test]
    fn an_exact_size_cuts_its_own_ratio_and_wins_over_the_aspect() {
        let edits = with(|e| {
            e.aspect = Some((1, 1));
            e.size = Some(Size::Exact {
                width: 1200,
                height: 628,
            });
        });
        assert_eq!(
            crop_size((2880, 1800), &edits),
            fit_ratio((2880, 1800), (1200, 628))
        );
        assert_eq!(crop_size((2880, 1800), &Edits::default()), (2880, 1800));
    }

    #[test]
    fn the_centre_window_is_centred() {
        assert_eq!(
            centre((2880, 1800), (1800, 1800)),
            Rect {
                x: 540,
                y: 0,
                width: 1800,
                height: 1800
            }
        );
    }

    #[test]
    fn sizes_and_scales_from_the_window() {
        let window = (2880, 1800);
        let out = |f: fn(&mut Edits)| output_size(window, &with(f));
        assert_eq!(out(|e| e.scale = Some(50)), (1440, 900));
        assert_eq!(
            out(|e| e.size = Some(Size::Width { width: 1440 })),
            (1440, 900)
        );
        assert_eq!(
            out(|e| e.size = Some(Size::Height { height: 900 })),
            (1440, 900)
        );
        assert_eq!(
            out(|e| e.size = Some(Size::LongEdge { edge: 1440 })),
            (1440, 900)
        );
        assert_eq!(
            output_size(
                (1800, 2880),
                &with(|e| e.size = Some(Size::LongEdge { edge: 1440 }))
            ),
            (900, 1440)
        );
        assert_eq!(
            out(|e| e.size = Some(Size::Exact {
                width: 1200,
                height: 750
            })),
            (1200, 750)
        );
        assert_eq!(out(|_| {}), window);
    }

    #[test]
    fn nothing_is_ever_upscaled() {
        let window = (2880, 1800);
        let out = |f: fn(&mut Edits)| output_size(window, &with(f));
        assert_eq!(out(|e| e.size = Some(Size::Width { width: 5000 })), window);
        assert_eq!(
            out(|e| e.size = Some(Size::LongEdge { edge: 9000 })),
            window
        );
        assert_eq!(
            out(|e| e.size = Some(Size::Exact {
                width: 5760,
                height: 3600
            })),
            window
        );
    }

    #[test]
    fn tiny_and_thin_pictures_never_reach_zero() {
        assert_eq!(
            output_size((1, 400), &with(|e| e.scale = Some(10))),
            (1, 40)
        );
        assert_eq!(output_size((3, 3), &with(|e| e.scale = Some(10))), (1, 1));
        assert_eq!(fit_ratio((1, 400), (16, 9)), (1, 1));
        assert_eq!(fit_ratio((400, 1), (9, 16)), (1, 1));
        assert_eq!(
            output_size(
                (2000, 1),
                &with(|e| e.size = Some(Size::Width { width: 10 }))
            ),
            (10, 1)
        );
    }
}
