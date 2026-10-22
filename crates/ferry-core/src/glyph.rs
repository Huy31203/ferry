//! Tray icon states (spec §5.6), drawn into the dot of the "F." logo so the
//! icon stays a one-colour template image that macOS tints for light and dark
//! menu bars. Coordinates are in the 64-unit grid of `src-tauri/icons/tray.svg`.

use crate::settings::TrayStyle;

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

/// Draw `glyph` over `base`, a square RGBA image `size` pixels wide, in the
/// default style.
pub fn render(base: &[u8], size: u32, glyph: TrayGlyph) -> Vec<u8> {
    render_styled(base, size, glyph, TrayStyle::Default)
}

/// Draw `glyph` over `base`, then give it the look of `style`. The state is
/// drawn first so every state exists in every style; dimming comes last so a
/// Stopped icon is faded whatever its style.
pub fn render_styled(base: &[u8], size: u32, glyph: TrayGlyph, style: TrayStyle) -> Vec<u8> {
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
        TrayGlyph::Running | TrayGlyph::Stopped => {}
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
    }
    let mut out = match style {
        TrayStyle::Default => out,
        TrayStyle::Classic => outline(&out, size),
        TrayStyle::Flat => badge(&out, size),
    };
    if glyph == TrayGlyph::Stopped {
        for pixel in out.as_chunks_mut::<4>().0 {
            pixel[3] = (f32::from(pixel[3]) * STOPPED_ALPHA).round() as u8;
        }
    }
    out
}

fn is_solid(px: &[u8], size: u32, x: i64, y: i64) -> bool {
    let size = i64::from(size);
    if x < 0 || y < 0 || x >= size || y >= size {
        return false;
    }
    px[((y * size + x) * 4 + 3) as usize] >= 128
}

/// Classic: only the rim of each shape, 1/32 of the icon wide (one pixel at least).
fn outline(px: &[u8], size: u32) -> Vec<u8> {
    let rim = i64::from((size / 32).max(1));
    let mut out = vec![0; px.len()];
    for y in 0..i64::from(size) {
        for x in 0..i64::from(size) {
            if !is_solid(px, size, x, y) {
                continue;
            }
            let on_rim =
                (-rim..=rim).any(|dy| (-rim..=rim).any(|dx| !is_solid(px, size, x + dx, y + dy)));
            if on_rim {
                out[((y * i64::from(size) + x) * 4 + 3) as usize] = 255;
            }
        }
    }
    out
}

/// Flat: a solid rounded badge with the shapes cut out of it.
fn badge(px: &[u8], size: u32) -> Vec<u8> {
    let scale = size as f32 / 64.0;
    let (margin, radius) = (2.0 * scale, 10.0 * scale);
    let (low, high) = (margin + radius, size as f32 - margin - radius);
    let mut out = vec![0; px.len()];
    for y in 0..size {
        for x in 0..size {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (nx, ny) = (cx.clamp(low, high), cy.clamp(low, high));
            if (cx - nx).hypot(cy - ny) <= radius {
                let i = ((y * size + x) * 4) as usize;
                out[i + 3] = 255 - px[i + 3];
            }
        }
    }
    out
}

/// Encode a square RGBA image as PNG (the Settings window's style thumbnails).
pub fn to_png(rgba: &[u8], size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(rgba))
        .expect("writing a PNG to memory cannot fail");
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

    #[test]
    fn the_default_style_is_render_itself() {
        for glyph in [TrayGlyph::Running, TrayGlyph::Paused, TrayGlyph::Stopped] {
            assert_eq!(
                render_styled(&logo(64), 64, glyph, TrayStyle::Default),
                render(&logo(64), 64, glyph)
            );
        }
    }

    #[test]
    fn classic_keeps_only_the_rim_of_a_shape() {
        let px = render_styled(&logo(64), 64, TrayGlyph::Running, TrayStyle::Classic);
        assert_eq!(alpha(&px, 64, 33, 45), 255, "the rim");
        assert_eq!(alpha(&px, 64, 40, 52), 0, "the hollow middle");
        assert_eq!(alpha(&px, 64, 10, 10), 0, "outside the shape");
    }

    #[test]
    fn flat_cuts_the_logo_out_of_a_solid_badge() {
        let px = render_styled(&logo(64), 64, TrayGlyph::Running, TrayStyle::Flat);
        assert_eq!(alpha(&px, 64, 40, 52), 0, "the dot is cut out");
        assert_eq!(alpha(&px, 64, 10, 10), 255, "the badge");
        assert_eq!(alpha(&px, 64, 0, 0), 0, "outside the rounded corner");
    }

    #[test]
    fn a_state_glyph_shows_in_every_style() {
        // Paused draws a gap between two bars; each style must keep it visible.
        let plain = render_styled(&logo(64), 64, TrayGlyph::Paused, TrayStyle::Default);
        for style in [TrayStyle::Classic, TrayStyle::Flat] {
            let styled = render_styled(&logo(64), 64, TrayGlyph::Paused, style);
            assert_ne!(
                styled,
                render_styled(&logo(64), 64, TrayGlyph::Running, style)
            );
            assert_ne!(styled, plain);
        }
    }

    #[test]
    fn stopped_dims_a_styled_icon_too() {
        let px = render_styled(&logo(64), 64, TrayGlyph::Stopped, TrayStyle::Flat);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] <= 102));
        assert_eq!(alpha(&px, 64, 10, 10), 102);
    }

    #[test]
    fn a_png_round_trips() {
        let png = to_png(&logo(16), 16);
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (16, 16));
        assert_eq!(&buf[..info.buffer_size()], &logo(16)[..]);
    }
}
