//! Where Ferry's windows go on screen: the results stack and the compare window.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// A screen rectangle in physical pixels (a monitor's work area).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Top-left position for a window of `size` placed in `corner` of `area`,
/// `margin` pixels in from both edges. A window larger than the area is
/// pinned to the area's top/left edge so it never starts off-screen.
pub fn place(area: Rect, size: (u32, u32), corner: Corner, margin: u32) -> (i32, i32) {
    let (x0, y0) = (i64::from(area.x), i64::from(area.y));
    let (w, h, m) = (i64::from(size.0), i64::from(size.1), i64::from(margin));
    let right = (x0 + i64::from(area.width) - m - w).max(x0);
    let bottom = (y0 + i64::from(area.height) - m - h).max(y0);
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => x0 + m,
        Corner::TopRight | Corner::BottomRight => right,
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => y0 + m,
        Corner::BottomLeft | Corner::BottomRight => bottom,
    };
    (x as i32, y as i32)
}

/// `size` clamped so a window with `margin` pixels on every side still fits in
/// `area`. Without this, a tall stack would run under the taskbar or off-screen.
pub fn fit(area: Rect, size: (u32, u32), margin: u32) -> (u32, u32) {
    let room = |extent: u32| extent.saturating_sub(margin.saturating_mul(2));
    (size.0.min(room(area.width)), size.1.min(room(area.height)))
}

/// The size of a window that shows an image of `image` pixels at 1:1 plus
/// `chrome` (its toolbars, in pixels). A larger image is shrunk, keeping its
/// shape, until the window takes at most `share` of `area` each way. The
/// window is never smaller than `min`, nor larger than `area`.
pub fn fit_image(
    area: Rect,
    image: (u32, u32),
    chrome: (u32, u32),
    share: f64,
    min: (u32, u32),
) -> (u32, u32) {
    let most = |extent: u32| (f64::from(extent) * share).floor();
    let (most_w, most_h) = (most(area.width), most(area.height));
    let (w, h) = (f64::from(image.0.max(1)), f64::from(image.1.max(1)));
    let room_w = (most_w - f64::from(chrome.0)).max(1.0);
    let room_h = (most_h - f64::from(chrome.1)).max(1.0);
    let k = (room_w / w).min(room_h / h).min(1.0);
    let side = |pixels: f64, chrome: u32, min: u32, extent: u32| {
        ((pixels * k).round() as u32 + chrome).max(min).min(extent)
    };
    (
        side(w, chrome.0, min.0, area.width),
        side(h, chrome.1, min.1, area.height),
    )
}

/// Top-left position that centres a window of `size` in `area`; a window
/// larger than the area starts at its top-left corner.
pub fn center(area: Rect, size: (u32, u32)) -> (i32, i32) {
    let gap = |extent: u32, side: u32| i64::from(extent.saturating_sub(side) / 2);
    (
        (i64::from(area.x) + gap(area.width, size.0)) as i32,
        (i64::from(area.y) + gap(area.height, size.1)) as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    #[test]
    fn bottom_right_sits_margin_from_both_edges() {
        assert_eq!(
            place(FULL_HD, (300, 200), Corner::BottomRight, 16),
            (1604, 864)
        );
    }

    #[test]
    fn work_area_above_a_taskbar_is_respected() {
        let above_taskbar = Rect {
            height: 1032,
            ..FULL_HD
        };
        assert_eq!(
            place(above_taskbar, (300, 200), Corner::BottomRight, 16),
            (1604, 816)
        );
    }

    #[test]
    fn monitor_left_of_primary_has_negative_origin() {
        let left = Rect {
            x: -1440,
            y: 0,
            width: 1440,
            height: 900,
        };
        assert_eq!(
            place(left, (300, 200), Corner::BottomRight, 16),
            (-316, 684)
        );
    }

    #[test]
    fn every_corner() {
        assert_eq!(place(FULL_HD, (300, 200), Corner::TopLeft, 16), (16, 16));
        assert_eq!(place(FULL_HD, (300, 200), Corner::TopRight, 16), (1604, 16));
        assert_eq!(
            place(FULL_HD, (300, 200), Corner::BottomLeft, 16),
            (16, 864)
        );
    }

    #[test]
    fn window_larger_than_area_is_pinned_inside_top_left() {
        assert_eq!(
            place(FULL_HD, (2000, 1200), Corner::BottomRight, 16),
            (0, 0)
        );
    }

    #[test]
    fn fit_leaves_a_window_that_already_fits() {
        assert_eq!(fit(FULL_HD, (300, 200), 16), (300, 200));
    }

    #[test]
    fn fit_clamps_a_stack_taller_than_the_work_area() {
        let above_taskbar = Rect {
            height: 1032,
            ..FULL_HD
        };
        // five 200 px cards plus gaps and padding are 1056 px tall
        assert_eq!(fit(above_taskbar, (324, 1056), 4), (324, 1024));
    }

    #[test]
    fn fit_clamps_width_and_never_underflows() {
        let tiny = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        assert_eq!(fit(FULL_HD, (2000, 200), 16), (1888, 200));
        assert_eq!(fit(tiny, (300, 200), 16), (0, 0));
    }

    const CHROME: (u32, u32) = (0, 80);
    const MIN: (u32, u32) = (520, 360);

    #[test]
    fn a_small_image_is_shown_at_its_own_size() {
        assert_eq!(fit_image(FULL_HD, (800, 600), CHROME, 0.8, MIN), (800, 680));
    }

    #[test]
    fn a_large_image_shrinks_to_eighty_percent_keeping_its_shape() {
        // 80% of 1080 is 864; 784 px are left for the image under the bars.
        assert_eq!(
            fit_image(FULL_HD, (2880, 1800), CHROME, 0.8, MIN),
            (1254, 864)
        );
    }

    #[test]
    fn a_tiny_or_thin_image_still_gets_a_usable_window() {
        assert_eq!(fit_image(FULL_HD, (16, 16), CHROME, 0.8, MIN), (520, 360));
        assert_eq!(
            fit_image(FULL_HD, (100, 5000), CHROME, 0.8, MIN),
            (520, 864)
        );
        assert_eq!(fit_image(FULL_HD, (0, 0), CHROME, 0.8, MIN), (520, 360));
    }

    #[test]
    fn the_minimum_never_outgrows_a_small_screen() {
        let small = Rect {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        };
        assert_eq!(fit_image(small, (16, 16), CHROME, 0.8, MIN), (400, 300));
    }

    #[test]
    fn center_puts_the_window_in_the_middle_of_its_monitor() {
        assert_eq!(center(FULL_HD, (1254, 864)), (333, 108));
        let left = Rect {
            x: -1440,
            y: 0,
            width: 1440,
            height: 900,
        };
        assert_eq!(center(left, (500, 300)), (-970, 300));
        assert_eq!(center(FULL_HD, (2000, 1200)), (0, 0));
    }
}
