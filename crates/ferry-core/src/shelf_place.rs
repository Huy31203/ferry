//! Where the shelf goes, and how the stack and the drop zone make room for it
//! (shelf spec §4). Rects are physical pixels; the constants are points.

use crate::geometry::{Corner, Rect};

pub const WIDTH: f64 = 150.0;
pub const EXPANDED: f64 = 170.0;
/// 16 % of the expanded height.
pub const STRIP: f64 = 27.0;
/// How near a drag must come to the strip to expand it.
pub const REACH: f64 = 80.0;

fn at_right(corner: Corner) -> bool {
    matches!(corner, Corner::TopRight | Corner::BottomRight)
}

fn at_bottom(corner: Corner) -> bool {
    matches!(corner, Corner::BottomLeft | Corner::BottomRight)
}

/// On the bottom edge of `area` whatever the corner, on the corner's side.
pub fn shelf_rect(area: Rect, corner: Corner, size: (u32, u32), margin: u32) -> Rect {
    let (w, h, m) = (size.0 as i32, size.1 as i32, margin as i32);
    let x = if at_right(corner) {
        area.x + area.width as i32 - m - w
    } else {
        area.x + m
    };
    let y = area.y + area.height as i32 - h;
    Rect {
        x,
        y,
        width: size.0,
        height: size.1,
    }
}

/// What the shelf takes from the stack: the expanded height when open (empty
/// or not), the strip when it rests with items, nothing otherwise.
pub fn inset(has_items: bool, height: u32, strip: u32) -> u32 {
    if height > strip {
        height
    } else if has_items && height > 0 {
        height
    } else {
        0
    }
}

/// The area the stack is placed in. Top-corner stacks keep all of it.
pub fn stack_area(area: Rect, corner: Corner, inset: u32) -> Rect {
    if at_bottom(corner) {
        Rect {
            height: area.height.saturating_sub(inset),
            ..area
        }
    } else {
        area
    }
}

pub fn contains(r: Rect, p: (i32, i32)) -> bool {
    p.0 >= r.x && p.0 < r.x + r.width as i32 && p.1 >= r.y && p.1 < r.y + r.height as i32
}

pub fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width as i32
        && b.x < a.x + a.width as i32
        && a.y < b.y + b.height as i32
        && b.y < a.y + a.height as i32
}

/// The zone's top-left, moved up so it ends where the shelf starts, never above `area`.
pub fn clear_of(zone: Rect, shelf: Rect, area: Rect) -> (i32, i32) {
    if intersects(zone, shelf) {
        (zone.x, (shelf.y - zone.height as i32).max(area.y))
    } else {
        (zone.x, zone.y)
    }
}

/// The zone wins: the shelf stays resting when expanding would cover it.
pub fn may_expand(zone: Option<Rect>, expanded: Rect) -> bool {
    !zone.is_some_and(|z| intersects(z, expanded))
}

/// A drag within `reach` of the strip (above or beside it), not inside the zone.
pub fn drag_near(cursor: (i32, i32), strip: Rect, reach: u32, zone: Option<Rect>) -> bool {
    if zone.is_some_and(|z| contains(z, cursor)) {
        return false;
    }
    let r = reach as i32;
    let grown = Rect {
        x: strip.x - r,
        y: strip.y - r,
        width: strip.width + 2 * reach,
        height: strip.height + reach,
    };
    contains(grown, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drag::zone_near_results;

    const AREA: Rect = Rect {
        x: 0,
        y: 25,
        width: 1440,
        height: 875,
    };
    const SIZE: (u32, u32) = (150, 170);

    #[test]
    fn the_shelf_sits_on_the_bottom_edge_on_the_stacks_side() {
        let r = shelf_rect(AREA, Corner::BottomRight, SIZE, 4);
        assert_eq!(
            r,
            Rect {
                x: 1440 - 4 - 150,
                y: 900 - 170,
                width: 150,
                height: 170
            }
        );
        let l = shelf_rect(AREA, Corner::BottomLeft, (150, 27), 4);
        assert_eq!(
            l,
            Rect {
                x: 4,
                y: 900 - 27,
                width: 150,
                height: 27
            }
        );
    }

    #[test]
    fn top_corners_keep_the_shelf_at_the_bottom() {
        let r = shelf_rect(AREA, Corner::TopRight, SIZE, 4);
        assert_eq!(r.y, 900 - 170);
        assert_eq!(r.x, 1440 - 4 - 150);
        // and the stack keeps the whole area
        assert_eq!(stack_area(AREA, Corner::TopRight, 170), AREA);
    }

    #[test]
    fn the_stack_moves_up_by_the_strip_only_when_the_shelf_has_items() {
        assert_eq!(inset(true, 27, 27), 27);
        assert_eq!(inset(false, 27, 27), 0); // empty strip during a drag overlays
        assert_eq!(inset(false, 0, 27), 0);
        assert_eq!(inset(true, 0, 27), 0); // hidden window takes nothing
        assert_eq!(stack_area(AREA, Corner::BottomRight, 27).height, 875 - 27);
    }

    #[test]
    fn the_expanded_shelf_pushes_the_stack_up() {
        assert_eq!(inset(true, 170, 27), 170);
        assert_eq!(inset(false, 170, 27), 170); // drop target while empty
        assert_eq!(stack_area(AREA, Corner::BottomLeft, 170).height, 875 - 170);
    }

    #[test]
    fn a_zone_over_the_shelf_moves_up_clear_of_it() {
        let shelf = shelf_rect(AREA, Corner::BottomRight, SIZE, 4);
        let zone = Rect {
            x: 1200,
            y: 800,
            width: 220,
            height: 140,
        };
        let (x, y) = clear_of(zone, shelf, AREA);
        assert_eq!((x, y), (1200, shelf.y - 140));
        assert!(!intersects(Rect { x, y, ..zone }, shelf));
        let apart = Rect {
            x: 100,
            y: 100,
            ..zone
        };
        assert_eq!(clear_of(apart, shelf, AREA), (100, 100));
    }

    #[test]
    fn every_zone_placement_clears_the_expanded_shelf() {
        let shelf = shelf_rect(AREA, Corner::BottomRight, SIZE, 4);
        let zone = (220, 140);
        // near results, with and without a stack, in the area the pushed stack leaves
        let room = stack_area(AREA, Corner::BottomRight, 170);
        for stack in [None, Some((300, 200))] {
            let (x, y) = zone_near_results(room, Corner::BottomRight, zone, stack, 4, 0);
            assert!(
                !intersects(
                    Rect {
                        x,
                        y,
                        width: 220,
                        height: 140
                    },
                    shelf
                ),
                "{stack:?}"
            );
        }
        // tab, from-tab and under-cursor placements land anywhere: clear_of fixes them
        for (x, y) in [(1440 - 220, 760), (1300, 880), (1250, 790)] {
            let (x, y) = clear_of(
                Rect {
                    x,
                    y,
                    width: 220,
                    height: 140,
                },
                shelf,
                AREA,
            );
            assert!(!intersects(
                Rect {
                    x,
                    y,
                    width: 220,
                    height: 140
                },
                shelf
            ));
        }
    }

    #[test]
    fn the_zone_wins_when_both_cannot_fit() {
        let tiny = Rect {
            x: 0,
            y: 0,
            width: 400,
            height: 200,
        };
        let shelf = shelf_rect(tiny, Corner::BottomRight, SIZE, 4);
        let zone = Rect {
            x: 180,
            y: 40,
            width: 220,
            height: 140,
        };
        let (_, y) = clear_of(zone, shelf, tiny);
        assert_eq!(y, 0); // pinned to the top, still overlapping
        assert!(!may_expand(Some(Rect { y, ..zone }), shelf));
        assert!(may_expand(None, shelf));
    }

    #[test]
    fn a_drag_near_the_strip_expands_it_but_not_over_the_zone() {
        let strip = shelf_rect(AREA, Corner::BottomRight, (150, 27), 4);
        assert!(drag_near((strip.x + 10, strip.y + 5), strip, 80, None));
        assert!(drag_near((strip.x - 70, strip.y - 70), strip, 80, None));
        assert!(!drag_near((strip.x - 90, strip.y), strip, 80, None));
        assert!(!drag_near((strip.x + 10, strip.y - 81), strip, 80, None));
        let zone = Rect {
            x: strip.x - 50,
            y: strip.y - 60,
            width: 100,
            height: 50,
        };
        assert!(!drag_near(
            (strip.x - 20, strip.y - 40),
            strip,
            80,
            Some(zone)
        ));
    }
}
