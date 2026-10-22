//! Tray icon states (spec §5.6), drawn into the dot of the "F." logo so the
//! icon stays a one-colour template image that macOS tints for light and dark
//! menu bars. Coordinates are in the 64-unit grid of `src-tauri/icons/tray.svg`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayGlyph {
    /// The logo as drawn.
    Running,
    /// The dot becomes a pause sign.
    Paused,
    /// The whole glyph is dimmed, as macOS draws a disabled item.
    Stopped,
    /// The dot becomes a hollow square while any job runs.
    Working,
    /// The dot becomes an exclamation mark until the menu is opened.
    Error,
}

/// The logo's dot: x 32..48, y 44..60.
const DOT: (f32, f32, f32, f32) = (32.0, 44.0, 48.0, 60.0);
const STOPPED_ALPHA: f32 = 0.4;

/// Draw `glyph` over `base`, a square RGBA image `size` pixels wide.
pub fn render(base: &[u8], size: u32, glyph: TrayGlyph) -> Vec<u8> {
    let mut out = base.to_vec();
    let scale = size as f32 / 64.0;
    let mut fill = |x0: f32, y0: f32, x1: f32, y1: f32, alpha: u8| {
        let (x0, y0) = ((x0 * scale).round() as u32, (y0 * scale).round() as u32);
        let (x1, y1) = ((x1 * scale).round() as u32, (y1 * scale).round() as u32);
        for y in y0..y1.min(size) {
            for x in x0..x1.min(size) {
                let i = ((y * size + x) * 4) as usize;
                out[i..i + 4].copy_from_slice(&[0, 0, 0, alpha]);
            }
        }
    };
    let (x0, y0, x1, y1) = DOT;
    match glyph {
        TrayGlyph::Running => {}
        TrayGlyph::Paused => {
            fill(x0, y0, x1, y1, 0);
            fill(x0, y0, x0 + 6.0, y1, 255);
            fill(x1 - 6.0, y0, x1, y1, 255);
        }
        TrayGlyph::Working => {
            fill(x0, y0, x1, y1, 255);
            fill(x0 + 4.0, y0 + 4.0, x1 - 4.0, y1 - 4.0, 0);
        }
        TrayGlyph::Error => {
            fill(x0, y0, x1, y1, 0);
            fill(x0 + 5.0, y0, x1 - 5.0, y1 - 6.0, 255);
            fill(x0 + 5.0, y1 - 4.0, x1 - 5.0, y1, 255);
        }
        TrayGlyph::Stopped => {
            for pixel in out.as_chunks_mut::<4>().0 {
                pixel[3] = (f32::from(pixel[3]) * STOPPED_ALPHA).round() as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the logo: transparent, with the dot filled.
    fn logo(size: u32) -> Vec<u8> {
        let mut px = vec![0; (size * size * 4) as usize];
        let s = size as f32 / 64.0;
        for y in (44.0 * s) as u32..(60.0 * s) as u32 {
            for x in (32.0 * s) as u32..(48.0 * s) as u32 {
                px[((y * size + x) * 4 + 3) as usize] = 255;
            }
        }
        px
    }

    fn alpha(px: &[u8], size: u32, x: u32, y: u32) -> u8 {
        let s = size as f32 / 64.0;
        let (x, y) = ((x as f32 * s) as u32, (y as f32 * s) as u32);
        px[((y * size + x) * 4 + 3) as usize]
    }

    #[test]
    fn running_is_the_logo_unchanged() {
        assert_eq!(render(&logo(64), 64, TrayGlyph::Running), logo(64));
    }

    #[test]
    fn paused_turns_the_dot_into_two_bars() {
        let px = render(&logo(64), 64, TrayGlyph::Paused);
        assert_eq!(alpha(&px, 64, 34, 52), 255);
        assert_eq!(alpha(&px, 64, 40, 52), 0, "gap between the bars");
        assert_eq!(alpha(&px, 64, 46, 52), 255);
    }

    #[test]
    fn working_hollows_the_dot() {
        let px = render(&logo(64), 64, TrayGlyph::Working);
        assert_eq!(alpha(&px, 64, 33, 45), 255);
        assert_eq!(alpha(&px, 64, 40, 52), 0);
    }

    #[test]
    fn error_draws_an_exclamation_mark() {
        let px = render(&logo(64), 64, TrayGlyph::Error);
        assert_eq!(alpha(&px, 64, 40, 46), 255, "the stroke");
        assert_eq!(alpha(&px, 64, 40, 55), 0, "the gap");
        assert_eq!(alpha(&px, 64, 40, 58), 255, "the point");
        assert_eq!(alpha(&px, 64, 33, 46), 0, "narrower than the dot");
    }

    #[test]
    fn stopped_dims_everything() {
        let px = render(&logo(64), 64, TrayGlyph::Stopped);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] <= 102));
        assert_eq!(alpha(&px, 64, 40, 52), 102);
    }

    #[test]
    fn glyphs_scale_with_the_icon() {
        let px = render(&logo(32), 32, TrayGlyph::Paused);
        assert_eq!(alpha(&px, 32, 34, 52), 255);
        assert_eq!(alpha(&px, 32, 40, 52), 0);
    }
}
