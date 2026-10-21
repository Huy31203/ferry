//! Where the results stack window goes on screen.

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
}
