//! Seeing a drag and a modifier tap from polled samples, and where the drop zone goes.

use serde::Serialize;

use crate::geometry::{self, Corner, Rect};
use crate::settings::{DragKey, DropZone, ZoneShowOn};

/// A tap is one modifier alone, down and up within this long.
pub const TAP_MS: u64 = 350;
/// A press moving this far, for `PRESS_MS`, counts as a drag where no drag signal exists.
pub const MOVE_PX: f64 = 24.0;
pub const PRESS_MS: u64 = 300;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub alt: bool,
    pub shift: bool,
    pub control: bool,
    pub command: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    Alt,
    Shift,
    Control,
    Command,
}

impl Mods {
    /// The one modifier held, if exactly one is.
    pub fn only(self) -> Option<Modifier> {
        let held = [
            (self.alt, Modifier::Alt),
            (self.shift, Modifier::Shift),
            (self.control, Modifier::Control),
            (self.command, Modifier::Command),
        ];
        let mut it = held.iter().filter(|(on, _)| *on);
        match (it.next(), it.next()) {
            (Some((_, m)), None) => Some(*m),
            _ => None,
        }
    }

    pub fn has(self, m: Modifier) -> bool {
        match m {
            Modifier::Alt => self.alt,
            Modifier::Shift => self.shift,
            Modifier::Control => self.control,
            Modifier::Command => self.command,
        }
    }

    pub fn none(self) -> bool {
        self == Mods::default()
    }
}

impl From<DragKey> for Modifier {
    fn from(k: DragKey) -> Self {
        match k {
            DragKey::Alt => Modifier::Alt,
            DragKey::Shift => Modifier::Shift,
        }
    }
}

/// One poll of the mouse, the modifiers and the drag pasteboard.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sample {
    pub at_ms: u64,
    pub button: bool,
    /// The drag pasteboard's change count; `None` where the OS has no such thing.
    pub drag_seq: Option<i64>,
    pub mods: Mods,
    pub cursor: (f64, f64),
    /// The press started on one of Ferry's own windows.
    pub over_own: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    DragStarted,
    PressMoved,
    DragEnded,
    Tap(Modifier),
}

#[derive(Debug, Default)]
pub struct DragWatch {
    idle_seq: Option<i64>,
    prev_mods: Mods,
    down: bool,
    press_at: u64,
    press_at_cursor: (f64, f64),
    press_seq: Option<i64>,
    press_over_own: bool,
    started: bool,
    tap: Option<(Modifier, u64)>,
}

impl DragWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// True between a drag (or a moving press) starting and the release.
    pub fn active(&self) -> bool {
        self.started
    }

    pub fn feed(&mut self, s: &Sample) -> Vec<Signal> {
        let mut out = Vec::new();
        if !s.button {
            if self.down && self.started {
                out.push(Signal::DragEnded);
            }
            self.down = false;
            self.started = false;
            self.tap = None;
            self.idle_seq = s.drag_seq;
            self.prev_mods = s.mods;
            return out;
        }
        if !self.down {
            self.down = true;
            self.press_at = s.at_ms;
            self.press_at_cursor = s.cursor;
            self.press_seq = self.idle_seq.or(s.drag_seq);
            self.press_over_own = s.over_own;
        }
        if !self.started {
            let (dx, dy) = (
                s.cursor.0 - self.press_at_cursor.0,
                s.cursor.1 - self.press_at_cursor.1,
            );
            if s.drag_seq.is_some() && s.drag_seq != self.press_seq {
                self.started = true;
                out.push(Signal::DragStarted);
            } else if s.drag_seq.is_none()
                && !self.press_over_own
                && dx.hypot(dy) >= MOVE_PX
                && s.at_ms - self.press_at >= PRESS_MS
            {
                self.started = true;
                out.push(Signal::PressMoved);
            }
        }
        match self.tap {
            None => {
                if let (Some(m), true) = (s.mods.only(), self.prev_mods.none()) {
                    self.tap = Some((m, s.at_ms));
                }
            }
            Some((m, at)) => {
                if s.mods.none() {
                    if s.at_ms - at <= TAP_MS {
                        out.push(Signal::Tap(m));
                    }
                    self.tap = None;
                } else if s.mods.only() != Some(m) || s.at_ms - at > TAP_MS {
                    self.tap = None;
                }
            }
        }
        self.prev_mods = s.mods;
        out
    }
}

/// A case-insensitive prefix match; blank entries match nothing.
pub fn is_ignored_app(ignored: &[String], app: &str) -> bool {
    let app = app.to_lowercase();
    ignored
        .iter()
        .map(|a| a.trim().to_lowercase())
        .any(|a| !a.is_empty() && app.starts_with(&a))
}

/// Whether a drag shows the zone (macOS) or the edge tab (Windows) by itself.
/// With `show_on == Modifier`, the Windows default, nothing shows without a tap.
pub fn should_auto_show(
    zone: &DropZone,
    has_files: bool,
    all_own: bool,
    frontmost: Option<&str>,
) -> bool {
    zone.enabled
        && zone.show_on == ZoneShowOn::AnyDrag
        && has_files
        && !all_own
        && !frontmost.is_some_and(|a| is_ignored_app(&zone.ignored_apps, a))
}

/// Beyond the results stack, or beside it when it is too tall to clear.
/// Physical pixels, top-left of the zone.
pub fn zone_near_results(
    area: Rect,
    corner: Corner,
    zone: (u32, u32),
    stack: Option<(u32, u32)>,
    margin: u32,
    gap: u32,
) -> (i32, i32) {
    let Some((sw, sh)) = stack else {
        return geometry::place(area, zone, corner, margin);
    };
    let (zw, zh, sw, sh) = (zone.0 as i32, zone.1 as i32, sw as i32, sh as i32);
    let (m, g) = (margin as i32, gap as i32);
    let right = area.x + area.width as i32;
    let bottom = area.y + area.height as i32;
    let at_right = matches!(corner, Corner::TopRight | Corner::BottomRight);
    let at_bottom = matches!(corner, Corner::BottomLeft | Corner::BottomRight);
    let x = if at_right { right - m - zw } else { area.x + m };
    let (y, fits) = if at_bottom {
        let y = bottom - m - sh - g - zh;
        (y, y >= area.y + m)
    } else {
        let y = area.y + m + sh + g;
        (y, y + zh <= bottom - m)
    };
    if fits {
        return (x, y);
    }
    let x = if at_right {
        right - m - sw - g - zw
    } else {
        area.x + m + sw + g
    };
    let y = if at_bottom {
        bottom - m - zh
    } else {
        area.y + m
    };
    (x, y)
}

fn clamp_to(v: i32, lo: i32, hi: i32) -> i32 {
    v.min(hi).max(lo)
}

/// Centred on the cursor, kept inside the area.
pub fn zone_under_cursor(area: Rect, zone: (u32, u32), cursor: (i32, i32)) -> (i32, i32) {
    let (zw, zh) = (zone.0 as i32, zone.1 as i32);
    (
        clamp_to(cursor.0 - zw / 2, area.x, area.x + area.width as i32 - zw),
        clamp_to(cursor.1 - zh / 2, area.y, area.y + area.height as i32 - zh),
    )
}

fn edge_x(area: Rect, corner: Corner, width: u32) -> i32 {
    match corner {
        Corner::TopLeft | Corner::BottomLeft => area.x,
        Corner::TopRight | Corner::BottomRight => area.x + area.width as i32 - width as i32,
    }
}

/// The slim tab, flush to the results side, vertically centred.
pub fn tab_at_edge(area: Rect, corner: Corner, tab: (u32, u32)) -> (i32, i32) {
    (
        edge_x(area, corner, tab.0),
        area.y + (area.height as i32 - tab.1 as i32) / 2,
    )
}

/// The full zone that replaces the tab: same edge, centred on the cursor.
pub fn zone_from_tab(area: Rect, corner: Corner, zone: (u32, u32), cursor_y: i32) -> (i32, i32) {
    let zh = zone.1 as i32;
    (
        edge_x(area, corner, zone.0),
        clamp_to(cursor_y - zh / 2, area.y, area.y + area.height as i32 - zh),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALT: Mods = Mods {
        alt: true,
        shift: false,
        control: false,
        command: false,
    };
    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 1440,
        height: 900,
    };

    fn s(at_ms: u64, button: bool, seq: Option<i64>, mods: Mods) -> Sample {
        Sample {
            at_ms,
            button,
            drag_seq: seq,
            mods,
            ..Sample::default()
        }
    }

    fn at(at_ms: u64, button: bool, x: f64, over_own: bool) -> Sample {
        Sample {
            at_ms,
            button,
            cursor: (x, 100.0),
            over_own,
            ..Sample::default()
        }
    }

    const NONE: Mods = Mods {
        alt: false,
        shift: false,
        control: false,
        command: false,
    };

    #[test]
    fn a_drag_is_seen_when_the_drag_pasteboard_changes_while_the_button_is_down() {
        let mut w = DragWatch::new();
        assert!(w.feed(&s(0, false, Some(5), NONE)).is_empty());
        assert!(w.feed(&s(50, true, Some(5), NONE)).is_empty());
        assert_eq!(w.feed(&s(80, true, Some(6), NONE)), [Signal::DragStarted]);
        assert!(w.feed(&s(110, true, Some(6), NONE)).is_empty());
        assert!(w.active());
    }

    #[test]
    fn a_drag_that_starts_between_samples_is_seen() {
        let mut w = DragWatch::new();
        w.feed(&s(0, false, Some(5), NONE));
        assert_eq!(w.feed(&s(30, true, Some(6), NONE)), [Signal::DragStarted]);
    }

    #[test]
    fn a_click_without_a_drag_signals_nothing() {
        let mut w = DragWatch::new();
        for sample in [
            s(0, false, Some(5), NONE),
            s(30, true, Some(5), NONE),
            s(60, false, Some(5), NONE),
        ] {
            assert!(w.feed(&sample).is_empty());
        }
    }

    #[test]
    fn the_drag_ends_on_release() {
        let mut w = DragWatch::new();
        w.feed(&s(0, false, Some(5), NONE));
        w.feed(&s(30, true, Some(6), NONE));
        assert_eq!(w.feed(&s(400, false, Some(6), NONE)), [Signal::DragEnded]);
        assert!(!w.active());
    }

    #[test]
    fn windows_sees_a_press_that_moves_and_lasts() {
        let mut w = DragWatch::new();
        w.feed(&at(0, false, 100.0, false));
        assert!(w.feed(&at(0, true, 100.0, false)).is_empty());
        assert!(w.feed(&at(100, true, 140.0, false)).is_empty());
        assert_eq!(w.feed(&at(350, true, 140.0, false)), [Signal::PressMoved]);
        w.feed(&at(400, false, 140.0, false));
        assert!(w.feed(&at(500, true, 100.0, false)).is_empty());
        assert!(w.feed(&at(1000, true, 110.0, false)).is_empty());
    }

    #[test]
    fn a_press_on_a_ferry_window_never_shows_the_tab() {
        let mut w = DragWatch::new();
        w.feed(&at(0, false, 100.0, false));
        assert!(w.feed(&at(0, true, 100.0, true)).is_empty());
        assert!(w.feed(&at(100, true, 140.0, true)).is_empty());
        assert!(w.feed(&at(350, true, 140.0, true)).is_empty());
    }

    #[test]
    fn a_quick_modifier_tap_during_a_drag_is_a_tap() {
        let mut w = DragWatch::new();
        w.feed(&s(900, true, None, NONE));
        assert!(w.feed(&s(1000, true, None, ALT)).is_empty());
        assert_eq!(
            w.feed(&s(1200, true, None, NONE)),
            [Signal::Tap(Modifier::Alt)]
        );
    }

    #[test]
    fn a_held_modifier_or_a_chord_is_not_a_tap() {
        let shift_too = Mods { shift: true, ..ALT };
        let mut w = DragWatch::new();
        w.feed(&s(900, true, None, NONE));
        w.feed(&s(1000, true, None, ALT));
        assert!(w.feed(&s(1500, true, None, NONE)).is_empty());
        let mut w = DragWatch::new();
        w.feed(&s(900, true, None, NONE));
        w.feed(&s(1000, true, None, ALT));
        assert!(w.feed(&s(1050, true, None, shift_too)).is_empty());
        assert!(w.feed(&s(1100, true, None, NONE)).is_empty());
    }

    #[test]
    fn a_tap_with_the_button_up_is_ignored() {
        let mut w = DragWatch::new();
        assert!(w.feed(&s(0, false, None, ALT)).is_empty());
        assert!(w.feed(&s(100, false, None, NONE)).is_empty());
    }

    #[test]
    fn ignored_apps_match_by_prefix_without_case() {
        let list = |a: &str| vec![a.to_string()];
        assert!(is_ignored_app(
            &list("com.adobe.PremierePro"),
            "com.adobe.premierepro.24"
        ));
        assert!(is_ignored_app(&list("Figma.exe"), "figma.exe"));
        assert!(!is_ignored_app(&list(""), "com.apple.finder"));
        assert!(!is_ignored_app(&list("com.figma"), "org.figma"));
    }

    #[test]
    fn the_zone_auto_shows_only_for_other_apps_file_drags() {
        // Built explicitly: the defaults differ by OS and CI runs the tests on Windows too.
        let zone = DropZone {
            show_on: ZoneShowOn::AnyDrag,
            ignored_apps: vec!["com.figma.Desktop".into()],
            ..DropZone::default()
        };
        let finder = Some("com.apple.finder");
        assert!(should_auto_show(&zone, true, false, finder));
        assert!(!should_auto_show(&zone, false, false, finder));
        assert!(!should_auto_show(&zone, true, true, finder));
        assert!(!should_auto_show(
            &zone,
            true,
            false,
            Some("com.figma.Desktop")
        ));
        let tap_only = DropZone {
            show_on: ZoneShowOn::Modifier,
            ..zone.clone()
        };
        assert!(!should_auto_show(&tap_only, true, false, finder));
        let off = DropZone {
            enabled: false,
            ..zone
        };
        assert!(!should_auto_show(&off, true, false, finder));
    }

    #[test]
    fn the_zone_sits_above_a_bottom_right_stack() {
        assert_eq!(
            zone_near_results(
                AREA,
                Corner::BottomRight,
                (244, 164),
                Some((324, 400)),
                16,
                8
            ),
            (1180, 312)
        );
    }

    #[test]
    fn the_zone_goes_beside_a_stack_too_tall_to_clear() {
        assert_eq!(
            zone_near_results(
                AREA,
                Corner::BottomRight,
                (244, 164),
                Some((324, 800)),
                16,
                8
            ),
            (848, 720)
        );
    }

    #[test]
    fn without_a_stack_the_zone_takes_the_corner() {
        assert_eq!(
            zone_near_results(AREA, Corner::BottomRight, (244, 164), None, 16, 8),
            (1180, 720)
        );
        assert_eq!(
            zone_near_results(AREA, Corner::TopLeft, (244, 164), Some((324, 400)), 16, 8),
            (16, 424)
        );
    }

    #[test]
    fn under_the_cursor_the_zone_is_centred_and_kept_on_screen() {
        assert_eq!(zone_under_cursor(AREA, (244, 164), (700, 450)), (578, 368));
        assert_eq!(zone_under_cursor(AREA, (244, 164), (10, 890)), (0, 736));
    }

    #[test]
    fn the_tab_hugs_the_results_edge() {
        assert_eq!(
            tab_at_edge(AREA, Corner::BottomRight, (24, 140)),
            (1416, 380)
        );
        assert_eq!(tab_at_edge(AREA, Corner::BottomLeft, (24, 140)), (0, 380));
        assert_eq!(
            zone_from_tab(AREA, Corner::BottomRight, (244, 164), 450),
            (1196, 368)
        );
    }
}
