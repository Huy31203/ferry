//! Smart framing (spec §4.2): put the crop window over the busiest part of
//! the picture, judged by edge energy (Sobel) on a small grey copy. Ties go
//! to the window nearest the centre, so a flat picture is cut from the centre.

use super::geometry::{Rect, centre};

/// Long edge of the grey copy the search runs on.
const PROBE: u32 = 256;

/// Where to cut `window` from the `width`×`height` RGBA picture.
pub fn frame(rgba: &[u8], width: u32, height: u32, window: (u32, u32)) -> Rect {
    let step = width.max(height).div_ceil(PROBE).max(1);
    let (sw, sh) = (width.div_ceil(step), height.div_ceil(step));
    let grey = shrink_grey(rgba, width, height, step, sw);
    let sums = integral(&sobel(&grey, sw, sh), sw, sh);
    let (ww, wh) = (
        (window.0 / step).clamp(1, sw),
        (window.1 / step).clamp(1, sh),
    );
    // Twice the centred origin, so distances stay in integers.
    let (cx, cy) = (i64::from(sw - ww), i64::from(sh - wh));
    let mut best = (0_u64, u64::MAX, 0_u32, 0_u32); // energy, distance², x, y
    for y in 0..=sh - wh {
        for x in 0..=sw - ww {
            let energy = window_sum(&sums, sw, x, y, ww, wh);
            let dx = 2 * i64::from(x) - cx;
            let dy = 2 * i64::from(y) - cy;
            let distance = (dx * dx + dy * dy) as u64;
            if energy > best.0 || (energy == best.0 && distance < best.1) {
                best = (energy, distance, x, y);
            }
        }
    }
    // The grid cannot sit on a half cell: when the middle window is as busy
    // as the best one, cut from the exact centre.
    let middle = window_sum(&sums, sw, (sw - ww) / 2, (sh - wh) / 2, ww, wh);
    if middle == best.0 {
        return centre((width, height), window);
    }
    Rect {
        x: (best.2 * step).min(width - window.0),
        y: (best.3 * step).min(height - window.1),
        width: window.0,
        height: window.1,
    }
}

/// Mean luma (alpha-weighted) of each `step`×`step` block.
fn shrink_grey(rgba: &[u8], width: u32, height: u32, step: u32, sw: u32) -> Vec<u32> {
    let sh = height.div_ceil(step);
    let mut sum = vec![0_u32; (sw * sh) as usize];
    let mut count = vec![0_u32; (sw * sh) as usize];
    for (i, p) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = (i as u32 % width, i as u32 / width);
        let luma = (299 * u32::from(p[0]) + 587 * u32::from(p[1]) + 114 * u32::from(p[2])) / 1000;
        let cell = ((y / step) * sw + x / step) as usize;
        sum[cell] += luma * u32::from(p[3]) / 255;
        count[cell] += 1;
    }
    sum.iter().zip(count).map(|(s, c)| s / c.max(1)).collect()
}

/// |Gx| + |Gy| for inner cells; the border stays 0.
fn sobel(grey: &[u32], w: u32, h: u32) -> Vec<u64> {
    let mut out = vec![0_u64; (w * h) as usize];
    let at = |x: u32, y: u32| i64::from(grey[(y * w + x) as usize]);
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let gx = at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2 * at(x - 1, y)
                - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2 * at(x, y - 1)
                - at(x + 1, y - 1);
            out[(y * w + x) as usize] = gx.unsigned_abs() + gy.unsigned_abs();
        }
    }
    out
}

/// Summed-area table, one row and column larger than the grid.
fn integral(values: &[u64], w: u32, h: u32) -> Vec<u64> {
    let stride = (w + 1) as usize;
    let mut sums = vec![0_u64; stride * (h + 1) as usize];
    for y in 0..h as usize {
        let mut row = 0;
        for x in 0..w as usize {
            row += values[y * w as usize + x];
            sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row;
        }
    }
    sums
}

fn window_sum(sums: &[u64], w: u32, x: u32, y: u32, ww: u32, wh: u32) -> u64 {
    let stride = (w + 1) as usize;
    let (x0, y0, x1, y1) = (x as usize, y as usize, (x + ww) as usize, (y + wh) as usize);
    sums[y1 * stride + x1] + sums[y0 * stride + x0]
        - sums[y0 * stride + x1]
        - sums[y1 * stride + x0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::geometry::centre;

    /// A white `w`×`h` picture; `busy` paints a black-and-white checkerboard.
    fn picture(w: u32, h: u32, busy: Option<Rect>) -> Vec<u8> {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let inside = busy.is_some_and(|b| {
                    (b.x..b.x + b.width).contains(&x) && (b.y..b.y + b.height).contains(&y)
                });
                let v = if inside && (x / 3 + y / 3) % 2 == 0 {
                    0
                } else {
                    255
                };
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        px
    }

    #[test]
    fn a_busy_block_pulls_the_window_to_it() {
        let block = Rect {
            x: 300,
            y: 70,
            width: 60,
            height: 60,
        };
        let rect = frame(&picture(400, 200, Some(block)), 400, 200, (150, 150));
        assert!(rect.x <= 300 && rect.x + 150 >= 360, "{rect:?}");
        assert!(rect.y <= 70 && rect.y + 150 >= 130, "{rect:?}");
    }

    #[test]
    fn a_flat_picture_is_cut_from_the_centre() {
        assert_eq!(
            frame(&picture(400, 200, None), 400, 200, (150, 150)),
            centre((400, 200), (150, 150))
        );
    }

    #[test]
    fn the_window_always_fits_large_odd_pictures() {
        let (w, h) = (1333, 777);
        let block = Rect {
            x: 1200,
            y: 600,
            width: 120,
            height: 120,
        };
        let rect = frame(&picture(w, h, Some(block)), w, h, (437, 777));
        assert!(
            rect.x + rect.width <= w && rect.y + rect.height <= h,
            "{rect:?}"
        );
        assert_eq!((rect.width, rect.height), (437, 777));
    }

    #[test]
    fn a_window_as_large_as_the_picture_is_the_whole_picture() {
        assert_eq!(
            frame(&picture(3, 3, None), 3, 3, (3, 3)),
            Rect {
                x: 0,
                y: 0,
                width: 3,
                height: 3
            }
        );
        assert_eq!(frame(&picture(1, 50, None), 1, 50, (1, 1)).width, 1);
    }
}
